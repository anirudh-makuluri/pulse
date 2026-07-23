//! Inference pipeline: discover → extract → LLM/heuristic → Inbox + evidence + check-ins.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{Local, Timelike, Utc};
use pulse_core::{
    compute_dedup_key, Config, NewCheckIn, NewEvidence, NewTask, SourceWatermark, Store,
    TaskSource, TaskStatus, TaskUpdate, CheckInKind,
};
use pulse_llm::{
    redact_for_remote, resolve_llm_client, HeuristicClient, InferRequest, LlmClient, SummaryRequest,
};
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

/// Start poll loop + optional end-of-day summary at local 23:55.
pub fn start_pipeline(
    store: Arc<Mutex<Store>>,
    config: Arc<Mutex<Config>>,
    poll_secs: u64,
) -> PipelineHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = Arc::clone(&stop);
    let join = thread::spawn(move || {
        let mut last_seen: HashMap<String, Instant> = HashMap::new();
        let mut hour_inserts: u32 = 0;
        let mut hour_start = Instant::now();
        let mut last_summary_day = String::new();

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
                    let _ = run_once(&mut store, &cfg, &mut last_seen, &mut hour_inserts);
                    maybe_auto_summary(&mut store, &cfg, &mut last_summary_day);
                }
            }

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

fn maybe_auto_summary(store: &mut Store, cfg: &Config, last_day: &mut String) {
    let now = Local::now();
    let day = now.format("%Y-%m-%d").to_string();
    // Once at/after 23:55 local, if no row for day.
    if now.time().hour() < 23 || now.time().minute() < 55 {
        return;
    }
    if *last_day == day {
        return;
    }
    if store.get_summary(&day).ok().flatten().is_some() {
        *last_day = day;
        return;
    }
    if generate_summary(store, cfg, &day).is_ok() {
        *last_day = day;
    }
}

/// Generate (or replace) daily summary for `day` (YYYY-MM-DD).
pub fn generate_summary(store: &mut Store, cfg: &Config, day: &str) -> Result<String, String> {
    let client = resolve_llm_client(&cfg.llm, &cfg.privacy);
    let tasks = store.list_tasks(None).map_err(|e| e.to_string())?;
    let lines: Vec<String> = tasks
        .iter()
        .filter(|t| {
            t.updated_at
                .with_timezone(&Local)
                .format("%Y-%m-%d")
                .to_string()
                == day
                || t.created_at
                    .with_timezone(&Local)
                    .format("%Y-%m-%d")
                    .to_string()
                    == day
        })
        .map(|t| format!("[{}] {} ({})", t.status, t.title, t.source))
        .collect();

    let req = SummaryRequest {
        day: day.to_string(),
        task_lines: lines,
        activity_notes: None,
    };
    let out = client.summarize_day(&req).map_err(|e| e.to_string())?;
    let offset = Local::now().offset().local_minus_utc() / 60;
    let highlights = serde_json::to_string(&out.highlights).unwrap_or_else(|_| "[]".into());
    let summary = store
        .upsert_summary(day, offset, &out.text, &highlights, "[]")
        .map_err(|e| e.to_string())?;
    Ok(summary.text)
}

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

    let client = resolve_llm_client(&cfg.llm, &cfg.privacy);
    let is_heuristic = client.backend_id() == "heuristic";

    for adapter in adapters {
        let arts = adapter.discover().map_err(|e| e.to_string())?;
        for art in arts {
            if let Some(t) = last_seen.get(&art.source_ref) {
                if t.elapsed() < debounce {
                    continue;
                }
            }

            let wm = store
                .get_watermark(&art.source_ref)
                .map_err(|e| e.to_string())?;
            let mut since = wm.as_ref().map(|w| w.byte_offset as u64);
            if let Some(w) = &wm {
                if (art.size_bytes as i64) < w.size_bytes {
                    since = Some(0);
                }
            }
            if let Some(w) = &wm {
                if w.size_bytes == art.size_bytes as i64
                    && w.byte_offset == art.size_bytes as i64
                    && w.mtime_ms == art.mtime_ms
                {
                    last_seen.insert(art.source_ref.clone(), Instant::now());
                    continue;
                }
            }

            let batch = adapter.extract(&art, since).map_err(|e| e.to_string())?;
            last_seen.insert(art.source_ref.clone(), Instant::now());

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

            let candidates = match client.infer_tasks(&req) {
                Ok(c) => c,
                Err(_) if !is_heuristic => {
                    // Fallback to heuristic on CLI failure
                    let h = HeuristicClient::default();
                    h.infer_tasks(&req).map_err(|e| e.to_string())?
                }
                Err(e) => return Err(e.to_string()),
            };

            let source = match adapter.id() {
                SourceId::Claude => TaskSource::Claude,
                SourceId::Codex => TaskSource::Codex,
            };

            for cand in candidates {
                if *hour_inserts >= hour_cap && is_heuristic {
                    break;
                }
                let title = cand.title.trim();
                if title.chars().count() < 12 {
                    continue;
                }

                let conf = if is_heuristic {
                    cand.confidence.min(0.45)
                } else {
                    cand.confidence.clamp(0.0, 1.0)
                };

                // Pre-existing: match_task_id or dedup
                let existing = if let Some(ref mid) = cand.match_task_id {
                    store.resolve_task(mid).ok()
                } else {
                    let dedup = compute_dedup_key(source, &batch.session_id, title);
                    store
                        .find_by_dedup_key(&dedup)
                        .map_err(|e| e.to_string())?
                };

                if let Some(task) = existing {
                    apply_update_for_existing(
                        store,
                        &task.id.to_string(),
                        &cand.proposed_status,
                        conf,
                        cand.notes.clone(),
                        cand.suggested_next_action.clone(),
                        cfg,
                    )?;
                    if let Some(sn) = cand.evidence_snippet {
                        let _ = store.add_evidence(NewEvidence {
                            task_id: task.id,
                            kind: "session_snippet".into(),
                            source_ref: batch.source_ref.clone(),
                            snippet: Some(redact_for_remote(&sn).text),
                            metadata_json: Some(
                                serde_json::json!({"backend": client.backend_id()}).to_string(),
                            ),
                            observed_at: Utc::now(),
                        });
                    }
                    continue;
                }

                // Create always Inbox
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
                new.confidence = Some(conf);
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
                                "backend": client.backend_id(),
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

                if is_heuristic {
                    *hour_inserts += 1;
                }
                created += 1;
            }
        }
    }

    Ok(created)
}

fn apply_update_for_existing(
    store: &mut Store,
    task_id: &str,
    proposed_status: &Option<String>,
    conf: f64,
    notes: Option<String>,
    next: Option<String>,
    cfg: &Config,
) -> Result<(), String> {
    let task = store.resolve_task(task_id).map_err(|e| e.to_string())?;
    let mut update = TaskUpdate {
        notes,
        suggested_next_action: next,
        confidence: Some(conf),
        ..Default::default()
    };

    if let Some(ps) = proposed_status {
        if let Some(status) = TaskStatus::parse(ps) {
            if status == TaskStatus::Done {
                if conf >= cfg.inference.strong_done_threshold {
                    update.status = Some(TaskStatus::Done);
                } else if conf >= cfg.inference.checkin_threshold {
                    let _ = store.create_checkin(NewCheckIn {
                        task_id: Some(task.id),
                        question: format!("Is \"{}\" done?", task.title),
                        kind: CheckInKind::IsDone,
                    });
                }
            } else if conf >= cfg.inference.auto_status_threshold {
                update.status = Some(status);
            } else if conf >= cfg.inference.checkin_threshold {
                let _ = store.create_checkin(NewCheckIn {
                    task_id: Some(task.id),
                    question: format!("Is \"{}\" still active? Suggested: {ps}", task.title),
                    kind: CheckInKind::StillActive,
                });
            }
        }
    }

    store
        .update_task(task.id, update)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Test helper: adapters + forced heuristic (or resolved client if ack).
pub fn run_once_with_adapters(
    store: &mut Store,
    cfg: &Config,
    adapters: Vec<Box<dyn SourceAdapter>>,
) -> Result<u32, String> {
    let mut last_seen = HashMap::new();
    // Build a synthetic cfg that only uses given adapters by enabling both
    // but we process adapters directly:
    let mut hour = 0u32;
    let client = resolve_llm_client(&cfg.llm, &cfg.privacy);
    let is_heuristic = client.backend_id() == "heuristic";
    let max_cand = cfg.inference.max_candidates_per_batch as usize;
    let hour_cap = cfg.inference.heuristic_inbox_inserts_per_hour;
    let mut created = 0u32;

    for adapter in adapters {
        let arts = adapter.discover().map_err(|e| e.to_string())?;
        for art in arts {
            created += process_artifact(
                store,
                adapter.as_ref(),
                &art,
                client.as_ref(),
                is_heuristic,
                max_cand,
                hour_cap,
                &mut hour,
                cfg,
            )?;
            let _ = last_seen.insert(art.source_ref.clone(), Instant::now());
        }
    }
    Ok(created)
}

fn process_artifact(
    store: &mut Store,
    adapter: &dyn SourceAdapter,
    art: &DiscoveredArtifact,
    client: &dyn LlmClient,
    is_heuristic: bool,
    max_cand: usize,
    hour_cap: u32,
    hour_inserts: &mut u32,
    cfg: &Config,
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
    let candidates = client.infer_tasks(&req).map_err(|e| e.to_string())?;
    let source = match adapter.id() {
        SourceId::Claude => TaskSource::Claude,
        SourceId::Codex => TaskSource::Codex,
    };
    let mut created = 0u32;
    for cand in candidates {
        if is_heuristic && *hour_inserts >= hour_cap {
            break;
        }
        let title = cand.title.trim();
        if title.chars().count() < 12 {
            continue;
        }
        let conf = if is_heuristic {
            cand.confidence.min(0.45)
        } else {
            cand.confidence.clamp(0.0, 1.0)
        };
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
        new.confidence = Some(conf);
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
                metadata_json: Some(
                    serde_json::json!({"backend": client.backend_id()}).to_string(),
                ),
                observed_at: Utc::now(),
            })
            .map_err(|e| e.to_string())?;
        if is_heuristic {
            *hour_inserts += 1;
        }
        created += 1;
        let _ = cfg; // thresholds used in run_once path
    }
    Ok(created)
}
