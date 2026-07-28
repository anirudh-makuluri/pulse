//! Inference pipeline: discover → extract → LLM/heuristic → Inbox + evidence + check-ins.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{Local, Timelike, Utc};
use pulse_core::{
    compute_dedup_key, CheckInKind, Config, NewCheckIn, NewEvidence, NewSession,
    NewSessionSyncState, NewTask, SourceWatermark, Store, SyncOutcome, TaskSource, TaskStatus,
    TaskUpdate,
};
use pulse_llm::{
    redact_for_remote, resolve_llm_client, HeuristicClient, InferRequest, LlmClient, SummaryRequest,
};
use pulse_sources::{
    ClaudeSource, CodexSource, DiscoveredArtifact, ExtractedBatch, SourceAdapter, SourceId,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub struct PipelineHandle {
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecentSessionSyncResult {
    pub sessions_reviewed: u32,
    pub sessions_already_imported: u32,
    pub tasks_created: u32,
    pub tasks_updated: u32,
    pub sessions_skipped_unchanged: u32,
    pub sessions_without_actionable_work: u32,
    pub sources_checked: Vec<String>,
}

struct SessionSyncInput {
    batch: ExtractedBatch,
    external_id: String,
    existing_session_id: Option<Uuid>,
    existing_task_id: Option<Uuid>,
    redacted_text: String,
    content_fingerprint: String,
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
        let mut last_summary_day = String::new();

        while !stop2.load(Ordering::SeqCst) {
            let cfg = match config.lock() {
                Ok(c) => c.clone(),
                Err(_) => break,
            };

            if let Ok(mut store) = store.lock() {
                // Transcript analysis is intentionally user initiated. The old
                // background path turned archival conversation into a noisy
                // Inbox before the user had a chance to review it.
                maybe_auto_summary(&mut store, &cfg, &mut last_summary_day);
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

/// Review at most the five most recently modified session transcripts from
/// each enabled source. Each source is sent in a single labelled LLM batch so
/// the model can produce one Inbox entry and outcome per source session.
pub fn sync_recent_sessions(
    store: &mut Store,
    cfg: &Config,
) -> Result<RecentSessionSyncResult, String> {
    if !cfg.inference.enabled {
        return Err("Session sync is disabled in Pulse settings.".into());
    }

    let client = resolve_llm_client(&cfg.llm, &cfg.privacy);
    if client.backend_id() == "heuristic" {
        return Err(
            "Session sync requires a configured agent CLI and remote-LLM acknowledgement; heuristic extraction is disabled for this action."
                .into(),
        );
    }

    let max_bytes = cfg.inference.max_candidate_text_bytes as usize;
    let mut adapters: Vec<Box<dyn SourceAdapter>> = Vec::new();
    if cfg.sources.claude.enabled {
        let mut source = ClaudeSource::from_env(max_bytes);
        source.extra_roots = cfg
            .sources
            .claude
            .extra_roots
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        adapters.push(Box::new(source));
    }
    if cfg.sources.codex.enabled {
        let mut source = CodexSource::from_env(max_bytes);
        source.extra_roots = cfg
            .sources
            .codex
            .extra_roots
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        adapters.push(Box::new(source));
    }
    if adapters.is_empty() {
        return Err("Enable Claude or Codex session tracking before syncing.".into());
    }

    let mut result = RecentSessionSyncResult {
        sessions_reviewed: 0,
        sessions_already_imported: 0,
        tasks_created: 0,
        tasks_updated: 0,
        sessions_skipped_unchanged: 0,
        sessions_without_actionable_work: 0,
        sources_checked: Vec::new(),
    };

    for adapter in adapters {
        let source_name = adapter.id().as_str().to_string();
        result.sources_checked.push(source_name.clone());
        let mut artifacts = adapter.discover().map_err(|e| e.to_string())?;
        artifacts.sort_by(|a, b| b.mtime_ms.cmp(&a.mtime_ms));

        let task_source = match adapter.id() {
            SourceId::Claude => TaskSource::Claude,
            SourceId::Codex => TaskSource::Codex,
        };
        let mut inputs = Vec::new();

        for artifact in artifacts.into_iter().take(5) {
            result.sessions_reviewed += 1;
            let external_id = format!("{source_name}:{}", artifact.session_id);
            let checkpoint = store
                .get_session_sync_state(&external_id)
                .map_err(|e| e.to_string())?;
            if checkpoint.as_ref().is_some_and(|state| {
                state.source_mtime_ms == artifact.mtime_ms
                    && state.source_size_bytes == artifact.size_bytes as i64
            }) {
                if checkpoint
                    .as_ref()
                    .and_then(|state| state.task_id)
                    .is_some()
                {
                    result.sessions_already_imported += 1;
                }
                result.sessions_skipped_unchanged += 1;
                continue;
            }
            let existing = store
                .get_session_by_external_id(&external_id)
                .map_err(|e| e.to_string())?;
            if existing.is_some() {
                result.sessions_already_imported += 1;
            }

            let batch = adapter
                .extract(&artifact, Some(0))
                .map_err(|e| e.to_string())?;
            let content_fingerprint = session_fingerprint(&batch.candidate_text);
            if batch.candidate_text.trim().is_empty() {
                store
                    .upsert_session_sync_state(NewSessionSyncState {
                        external_id,
                        source: source_name.clone(),
                        source_session_id: batch.session_id.clone(),
                        task_id: checkpoint
                            .as_ref()
                            .and_then(|state| state.task_id)
                            .or_else(|| existing.as_ref().map(|session| session.task_id)),
                        content_fingerprint,
                        source_mtime_ms: batch.mtime_ms,
                        source_size_bytes: batch.size_bytes as i64,
                        result: "no_actionable_work".into(),
                        last_checked_at: Utc::now(),
                    })
                    .map_err(|e| e.to_string())?;
                result.sessions_without_actionable_work += 1;
                continue;
            }

            if checkpoint
                .as_ref()
                .is_some_and(|state| state.content_fingerprint == content_fingerprint)
            {
                store
                    .upsert_session_sync_state(NewSessionSyncState {
                        external_id,
                        source: source_name.clone(),
                        source_session_id: batch.session_id.clone(),
                        task_id: checkpoint.as_ref().and_then(|state| state.task_id),
                        content_fingerprint,
                        source_mtime_ms: batch.mtime_ms,
                        source_size_bytes: batch.size_bytes as i64,
                        result: checkpoint
                            .as_ref()
                            .map(|state| state.result.clone())
                            .unwrap_or_else(|| "no_actionable_work".into()),
                        last_checked_at: Utc::now(),
                    })
                    .map_err(|e| e.to_string())?;
                result.sessions_skipped_unchanged += 1;
                continue;
            }

            let redacted = redact_for_remote(&batch.candidate_text);
            inputs.push(SessionSyncInput {
                batch,
                external_id,
                existing_session_id: existing.as_ref().map(|session| session.id),
                existing_task_id: checkpoint
                    .as_ref()
                    .and_then(|state| state.task_id)
                    .or_else(|| existing.map(|session| session.task_id)),
                redacted_text: session_excerpt(&redacted.text, 4_000),
                content_fingerprint,
            });
        }

        if inputs.is_empty() {
            continue;
        }

        let candidate_text = inputs
            .iter()
            .map(|input| {
                format!(
                    "SOURCE SESSION\nsource_session_id: {}\nsource_ref: {}\nproject: {}\nBEGIN TRANSCRIPT\n{}\nEND TRANSCRIPT",
                    input.batch.session_id,
                    input.batch.source_ref,
                    input.batch.project.as_deref().unwrap_or("unknown"),
                    input.redacted_text,
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let candidates = client
            .infer_tasks(&InferRequest {
                source: source_name.clone(),
                source_ref: format!("manual_session_sync:{source_name}"),
                session_id: "batch".into(),
                project: None,
                candidate_text,
                max_candidates: inputs.len(),
            })
            .map_err(|e| format!("{source_name} session analysis failed: {e}"))?;

        let inputs_by_session: HashMap<&str, usize> = inputs
            .iter()
            .enumerate()
            .map(|(index, input)| (input.batch.session_id.as_str(), index))
            .collect();
        let mut handled_sessions = HashSet::new();

        for candidate in candidates {
            let Some(session_id) = candidate.source_session_id.as_deref() else {
                continue;
            };
            let Some(&input_index) = inputs_by_session.get(session_id) else {
                continue;
            };
            if handled_sessions.contains(session_id) {
                continue;
            }

            let input = &inputs[input_index];
            let title = candidate.title.trim();
            if title.chars().count() < 12 {
                continue;
            }
            handled_sessions.insert(session_id.to_owned());
            let outcome = parse_sync_outcome(candidate.sync_outcome.as_deref());
            let outcome_confidence = candidate
                .sync_outcome_confidence
                .unwrap_or(candidate.confidence)
                .clamp(0.0, 1.0);
            let dedup = compute_dedup_key(task_source, &input.batch.session_id, title);
            let (task, sync_result) = if let Some(task_id) = input.existing_task_id {
                // Preserve the user's workflow state. Sync only refreshes the
                // AI-written summary, next action, and observed outcome.
                let task = store
                    .update_task(
                        task_id,
                        TaskUpdate {
                            title: Some(title.to_string()),
                            notes: candidate.notes.clone(),
                            project: input.batch.project.clone(),
                            suggested_next_action: candidate.suggested_next_action.clone(),
                            confidence: Some(candidate.confidence.clamp(0.0, 1.0)),
                            sync_outcome: Some(outcome),
                            sync_outcome_confidence: Some(outcome_confidence),
                            ..Default::default()
                        },
                    )
                    .map_err(|e| e.to_string())?;
                result.tasks_updated += 1;
                (task, "updated")
            } else if let Some(existing) =
                store.find_by_dedup_key(&dedup).map_err(|e| e.to_string())?
            {
                let task = store
                    .update_task(
                        existing.id,
                        TaskUpdate {
                            notes: candidate.notes.clone(),
                            project: input.batch.project.clone(),
                            suggested_next_action: candidate.suggested_next_action.clone(),
                            confidence: Some(candidate.confidence.clamp(0.0, 1.0)),
                            sync_outcome: Some(outcome),
                            sync_outcome_confidence: Some(outcome_confidence),
                            ..Default::default()
                        },
                    )
                    .map_err(|e| e.to_string())?;
                result.tasks_updated += 1;
                (task, "updated")
            } else {
                let mut task = NewTask::manual(title);
                task.status = TaskStatus::Inbox;
                task.source = task_source;
                task.confidence = Some(candidate.confidence.clamp(0.0, 1.0));
                task.project = input.batch.project.clone();
                task.notes = candidate.notes.clone();
                task.suggested_next_action = candidate.suggested_next_action.clone();
                task.dedup_key = Some(dedup);
                task.source_session_id = Some(input.batch.session_id.clone());
                task.sync_outcome = Some(outcome);
                task.sync_outcome_confidence = Some(outcome_confidence);
                let task = store.create_task(task).map_err(|e| e.to_string())?;
                result.tasks_created += 1;
                (task, "created")
            };

            store
                .upsert_session_sync_state(NewSessionSyncState {
                    external_id: input.external_id.clone(),
                    source: source_name.clone(),
                    source_session_id: input.batch.session_id.clone(),
                    task_id: Some(task.id),
                    content_fingerprint: input.content_fingerprint.clone(),
                    source_mtime_ms: input.batch.mtime_ms,
                    source_size_bytes: input.batch.size_bytes as i64,
                    result: sync_result.into(),
                    last_checked_at: Utc::now(),
                })
                .map_err(|e| e.to_string())?;

            let snippet = candidate
                .evidence_snippet
                .map(|text| redact_for_remote(&text).text)
                .or_else(|| Some(input.redacted_text.chars().take(200).collect()));
            store
                .add_evidence(NewEvidence {
                    task_id: task.id,
                    kind: "session_snippet".into(),
                    source_ref: input.batch.source_ref.clone(),
                    snippet,
                    metadata_json: Some(
                        serde_json::json!({
                            "backend": client.backend_id(),
                            "outcome": outcome.as_str(),
                            "outcome_confidence": outcome_confidence,
                        })
                        .to_string(),
                    ),
                    observed_at: Utc::now(),
                })
                .map_err(|e| e.to_string())?;

            let session_id = if let Some(session_id) = input.existing_session_id {
                session_id
            } else {
                let started_at =
                    chrono::DateTime::<Utc>::from_timestamp_millis(input.batch.mtime_ms)
                        .unwrap_or_else(Utc::now);
                store
                    .create_session(NewSession {
                        task_id: task.id,
                        agent: Some(source_name.clone()),
                        application: Some(source_name.clone()),
                        repository_path: input.batch.project.clone(),
                        external_id: Some(input.external_id.clone()),
                        source_ref: Some(input.batch.source_ref.clone()),
                        started_at,
                        ended_at: Some(started_at),
                        metadata_json: serde_json::json!({
                            "sync": "manual_recent_sessions",
                            "path": input.batch.path,
                        })
                        .to_string(),
                    })
                    .map_err(|e| e.to_string())?
                    .id
            };
            let _ = store.record_event(pulse_core::NewActivityEvent {
                task_id: task.id,
                session_id: Some(session_id),
                kind: "session_synced".into(),
                summary: format!("Reviewed {source_name} session for Inbox"),
                payload_json: Some(
                    serde_json::json!({
                        "source_ref": input.batch.source_ref,
                        "outcome": outcome.as_str(),
                    })
                    .to_string(),
                ),
                source_ref: Some("manual_session_sync".into()),
                occurred_at: Utc::now(),
            });
        }

        for input in &inputs {
            if handled_sessions.contains(&input.batch.session_id) {
                continue;
            }
            store
                .upsert_session_sync_state(NewSessionSyncState {
                    external_id: input.external_id.clone(),
                    source: source_name.clone(),
                    source_session_id: input.batch.session_id.clone(),
                    task_id: input.existing_task_id,
                    content_fingerprint: input.content_fingerprint.clone(),
                    source_mtime_ms: input.batch.mtime_ms,
                    source_size_bytes: input.batch.size_bytes as i64,
                    result: "no_actionable_work".into(),
                    last_checked_at: Utc::now(),
                })
                .map_err(|e| e.to_string())?;
        }
        result.sessions_without_actionable_work += (inputs.len() - handled_sessions.len()) as u32;
    }

    Ok(result)
}

fn session_excerpt(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }

    let head_len = max_chars / 4;
    let tail_len = max_chars - head_len;
    let head: String = text.chars().take(head_len).collect();
    let tail: String = text.chars().skip(count.saturating_sub(tail_len)).collect();
    format!("{head}\n… [middle omitted] …\n{tail}")
}

fn session_fingerprint(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn parse_sync_outcome(value: Option<&str>) -> SyncOutcome {
    value
        .and_then(SyncOutcome::parse)
        .unwrap_or(SyncOutcome::Unclear)
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
                    store.find_by_dedup_key(&dedup).map_err(|e| e.to_string())?
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
