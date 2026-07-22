//! Inference pipeline: discover → extract → heuristic → Inbox tasks + evidence.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use pulse_core::{
    compute_dedup_key, Config, NewEvidence, NewTask, SourceWatermark, Store, TaskSource,
    TaskStatus,
};
use pulse_llm::{redact_for_remote, HeuristicClient, InferRequest, LlmClient};
use pulse_sources::{ClaudeSource, CodexSource, DiscoveredArtifact, SourceAdapter, SourceId};

pub struct PipelineHandle {
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}

impl PipelineHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Start a background poll loop (every `poll_secs`) that runs inference when sources enabled.
pub fn start_pipeline(
    store: Arc<Mutex<Store>>,
    config: Arc<Mutex<Config>>,
    poll_secs: u64,
) -> PipelineHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = Arc::clone(&stop);
    let join = thread::spawn(move || {
        // Per-path debounce timestamps
        let mut last_seen: HashMap<String, Instant> = HashMap::new();
        let mut hour_inserts: u32 = 0;
        let mut hour_start = Instant::now();

        while !stop2.load(Ordering::SeqCst) {
            let cfg = match config.lock() {
                Ok(c) => c.clone(),
                Err(_) => break,
            };

            if cfg.inference.enabled {
                if hour_start.elapsed() > Duration::from_secs(3600) {
                    hour_inserts = 0;
                    hour_start = Instant::now();
                }

                if let Ok(mut store) = store.lock() {
                    let _ = run_once(
                        &mut store,
                        &cfg,
                        &mut last_seen,
                        &mut hour_inserts,
                    );
                }
            }

            // Sleep in slices so stop is responsive
            let slice = Duration::from_millis(200);
            let total = Duration::from_secs(poll_secs.max(5));
            let mut waited = Duration::ZERO;
            while waited < total && !stop2.load(Ordering::SeqCst) {
                thread::sleep(slice);
                waited += slice;
            }
        }
    });

    PipelineHandle {
        stop,
        join: Some(join),
    }
}

/// Single scan/process pass (also used by tests / manual trigger).
pub fn run_once(
    store: &mut Store,
    cfg: &Config,
    last_seen: &mut HashMap<String, Instant>,
    hour_inserts: &mut u32,
) -> Result<u32, String> {
    let mut created = 0u32;
    let debounce = Duration::from_millis(cfg.inference.debounce_ms.max(100));
    let max_bytes = cfg.inference.max_candidate_text_bytes as usize;
    let max_cand = cfg.inference.max_candidates_per_batch as usize;
    let hour_cap = cfg.inference.heuristic_inbox_inserts_per_hour;

    let mut adapters: Vec<Box<dyn SourceAdapter>> = Vec::new();
    if cfg.sources.claude.enabled {
        let mut s = ClaudeSource::from_env(max_bytes);
        s.extra_roots = cfg
            .sources
            .claude
            .extra_roots
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        adapters.push(Box::new(s));
    }
    if cfg.sources.codex.enabled {
        let mut s = CodexSource::from_env(max_bytes);
        s.extra_roots = cfg
            .sources
            .codex
            .extra_roots
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        adapters.push(Box::new(s));
    }

    if adapters.is_empty() {
        return Ok(0);
    }

    let heuristic = HeuristicClient::default();

    for adapter in adapters {
        let arts = adapter.discover().map_err(|e| e.to_string())?;
        for art in arts {
            // Debounce: skip if we processed this path very recently without growth
            if let Some(t) = last_seen.get(&art.source_ref) {
                if t.elapsed() < debounce {
                    continue;
                }
            }

            let wm = store
                .get_watermark(&art.source_ref)
                .map_err(|e| e.to_string())?;
            let mut since = wm.as_ref().map(|w| w.byte_offset as u64);
            // Shrink reset
            if let Some(w) = &wm {
                if (art.size_bytes as i64) < w.size_bytes {
                    since = Some(0);
                }
            }

            // Skip if no change in size and offset already at end
            if let Some(w) = &wm {
                if w.size_bytes == art.size_bytes as i64
                    && w.byte_offset == art.size_bytes as i64
                    && w.mtime_ms == art.mtime_ms
                {
                    last_seen.insert(art.source_ref.clone(), Instant::now());
                    continue;
                }
            }

            let batch = adapter
                .extract(&art, since)
                .map_err(|e| e.to_string())?;
            last_seen.insert(art.source_ref.clone(), Instant::now());

            // Update watermark even if no text (advanced offset)
            let wm_new = SourceWatermark {
                source_ref: art.source_ref.clone(),
                path: art.path.to_string_lossy().to_string(),
                size_bytes: batch.size_bytes as i64,
                mtime_ms: batch.mtime_ms,
                byte_offset: batch.new_byte_offset as i64,
                last_processed_at: Utc::now(),
            };
            store.upsert_watermark(&wm_new).map_err(|e| e.to_string())?;

            if batch.candidate_text.trim().is_empty() {
                continue;
            }

            let redacted = redact_for_remote(&batch.candidate_text);
            let req = InferRequest {
                source: adapter.id().as_str().into(),
                source_ref: batch.source_ref.clone(),
                session_id: batch.session_id.clone(),
                project: batch.project.clone(),
                candidate_text: redacted.text.clone(),
                max_candidates: max_cand,
            };

            let candidates = heuristic.infer_tasks(&req).map_err(|e| e.to_string())?;
            let source = match adapter.id() {
                SourceId::Claude => TaskSource::Claude,
                SourceId::Codex => TaskSource::Codex,
            };

            for cand in candidates {
                if *hour_inserts >= hour_cap {
                    break;
                }
                let title = cand.title.trim();
                if title.chars().count() < 12 {
                    continue;
                }
                let dedup = compute_dedup_key(source, &batch.session_id, title);
                if store
                    .find_by_dedup_key(&dedup)
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    continue;
                }

                // Always Inbox on create
                let mut new = NewTask::manual(title);
                new.status = TaskStatus::Inbox;
                new.source = source;
                new.confidence = Some(cand.confidence.min(0.45));
                new.project = batch.project.clone();
                new.notes = cand.notes;
                new.suggested_next_action = cand.suggested_next_action;
                new.dedup_key = Some(dedup);
                new.source_session_id = Some(batch.session_id.clone());

                let task = store.create_task(new).map_err(|e| e.to_string())?;
                let snippet = cand
                    .evidence_snippet
                    .map(|s| redact_for_remote(&s).text)
                    .or_else(|| Some(redacted.text.chars().take(200).collect()));

                store
                    .add_evidence(NewEvidence {
                        task_id: task.id,
                        kind: "session_snippet".into(),
                        source_ref: batch.source_ref.clone(),
                        snippet,
                        metadata_json: Some(
                            serde_json::json!({
                                "backend": heuristic.backend_id(),
                                "session_id": batch.session_id,
                            })
                            .to_string(),
                        ),
                        observed_at: Utc::now(),
                    })
                    .map_err(|e| e.to_string())?;

                let _ = store.insert_activity(
                    adapter.id().as_str(),
                    "task_inferred",
                    &batch.source_ref,
                    Some(&format!(r#"{{"task_id":"{}"}}"#, task.id)),
                    Some(task.id),
                );

                *hour_inserts += 1;
                created += 1;
            }
        }
    }

    Ok(created)
}

/// Test helper: run adapters against explicit roots (not env).
pub fn run_once_with_adapters(
    store: &mut Store,
    cfg: &Config,
    adapters: Vec<Box<dyn SourceAdapter>>,
) -> Result<u32, String> {
    let mut hour_inserts = 0u32;
    let max_cand = cfg.inference.max_candidates_per_batch as usize;
    let hour_cap = cfg.inference.heuristic_inbox_inserts_per_hour;
    let heuristic = HeuristicClient::default();
    let mut created = 0u32;

    for adapter in adapters {
        let arts = adapter.discover().map_err(|e| e.to_string())?;
        for art in arts {
            created += process_artifact(
                store,
                adapter.as_ref(),
                &art,
                &heuristic,
                max_cand,
                hour_cap,
                &mut hour_inserts,
            )?;
        }
    }
    Ok(created)
}

fn process_artifact(
    store: &mut Store,
    adapter: &dyn SourceAdapter,
    art: &DiscoveredArtifact,
    heuristic: &HeuristicClient,
    max_cand: usize,
    hour_cap: u32,
    hour_inserts: &mut u32,
) -> Result<u32, String> {
    let wm = store
        .get_watermark(&art.source_ref)
        .map_err(|e| e.to_string())?;
    let mut since = wm.as_ref().map(|w| w.byte_offset as u64);
    if let Some(w) = &wm {
        if (art.size_bytes as i64) < w.size_bytes {
            since = Some(0);
        }
    }
    let batch = adapter.extract(art, since).map_err(|e| e.to_string())?;
    store
        .upsert_watermark(&SourceWatermark {
            source_ref: art.source_ref.clone(),
            path: art.path.to_string_lossy().to_string(),
            size_bytes: batch.size_bytes as i64,
            mtime_ms: batch.mtime_ms,
            byte_offset: batch.new_byte_offset as i64,
            last_processed_at: Utc::now(),
        })
        .map_err(|e| e.to_string())?;

    if batch.candidate_text.trim().is_empty() {
        return Ok(0);
    }

    let redacted = redact_for_remote(&batch.candidate_text);
    let req = InferRequest {
        source: adapter.id().as_str().into(),
        source_ref: batch.source_ref.clone(),
        session_id: batch.session_id.clone(),
        project: batch.project.clone(),
        candidate_text: redacted.text.clone(),
        max_candidates: max_cand,
    };
    let candidates = heuristic.infer_tasks(&req).map_err(|e| e.to_string())?;
    let source = match adapter.id() {
        SourceId::Claude => TaskSource::Claude,
        SourceId::Codex => TaskSource::Codex,
    };
    let mut created = 0u32;
    for cand in candidates {
        if *hour_inserts >= hour_cap {
            break;
        }
        let title = cand.title.trim();
        if title.chars().count() < 12 {
            continue;
        }
        let dedup = compute_dedup_key(source, &batch.session_id, title);
        if store
            .find_by_dedup_key(&dedup)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        let mut new = NewTask::manual(title);
        new.status = TaskStatus::Inbox;
        new.source = source;
        new.confidence = Some(cand.confidence.min(0.45));
        new.project = batch.project.clone();
        new.notes = cand.notes;
        new.dedup_key = Some(dedup);
        new.source_session_id = Some(batch.session_id.clone());
        let task = store.create_task(new).map_err(|e| e.to_string())?;
        store
            .add_evidence(NewEvidence {
                task_id: task.id,
                kind: "session_snippet".into(),
                source_ref: batch.source_ref.clone(),
                snippet: cand
                    .evidence_snippet
                    .map(|s| redact_for_remote(&s).text)
                    .or_else(|| Some(redacted.text.chars().take(200).collect())),
                metadata_json: None,
                observed_at: Utc::now(),
            })
            .map_err(|e| e.to_string())?;
        *hour_inserts += 1;
        created += 1;
    }
    Ok(created)
}
