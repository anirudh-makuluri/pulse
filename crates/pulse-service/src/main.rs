//! Pulse background service: named-pipe JSON-RPC server + source inference poller.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::collections::HashMap;

use chrono::Utc;
use clap::{Parser, Subcommand};
use pulse_core::ipc::pid::{remove_pid_file_if_matches, write_pid_file, ServicePidFile};
use pulse_core::ipc::pipe::{self, current_pid};
use pulse_core::ipc::rpc::{RpcCode, RpcErrorObject, RpcHandler};
use pulse_core::{
    apply_checkin_answer, export_history, load_config, open_db, parse_answer_input, write_config,
    Config, ExportFormat, NewCheckpoint, NewSession, NewTask, PulseError, PulsePaths, Store,
    TaskStatus, TaskUpdate,
};
use pulse_llm::llm_status;
use pulse_service::pipeline;
use serde_json::{json, Value};

#[derive(Parser, Debug)]
#[command(name = "pulse-service", version, about = "Pulse background service")]
struct Cli {
    #[arg(long, global = true, value_name = "DIR")]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Run {
        #[arg(long)]
        quiet: bool,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("pulse-service error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run { quiet } => run_service(cli.data_dir, quiet)?,
    }
    Ok(())
}

fn run_service(data_dir: Option<PathBuf>, quiet: bool) -> Result<(), Box<dyn std::error::Error>> {
    let paths = match data_dir {
        Some(d) => PulsePaths::new(d),
        None => PulsePaths::default()?,
    };
    paths.ensure_layout()?;

    let config = load_config(&paths.config_path())?;
    let pipe_name = config.service.pipe_name.clone();
    let conn = open_db(&paths.db_path())?;
    let store = Arc::new(Mutex::new(Store::new(conn)));
    let config = Arc::new(Mutex::new(config));

    let state = Arc::new(ServiceState {
        paths: paths.clone(),
        store: Arc::clone(&store),
        config: Arc::clone(&config),
        shutdown: Arc::new(AtomicBool::new(false)),
        started_at: Utc::now(),
        pid: current_pid(),
    });

    pipe::probe_bind(&pipe_name)?;

    let exe_path = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "pulse-service".into());

    let pid_info = ServicePidFile {
        pid: state.pid,
        started_at: state.started_at.to_rfc3339(),
        exe_path: exe_path.clone(),
        pipe_name: pipe_name.clone(),
    };
    write_pid_file(&paths.service_pid_path(), &pid_info)?;

    // Start source inference poller (PR4: heuristic only).
    let pipeline = pipeline::start_pipeline(Arc::clone(&store), Arc::clone(&config), 30);
    let reminder_scheduler = start_reminder_scheduler(Arc::clone(&store));

    if !quiet {
        eprintln!(
            "pulse-service listening on pipe '{}' (pid {})",
            pipe_name, state.pid
        );
        eprintln!("inference poller active (enable sources via `pulse sources enable`)");
        eprintln!("stop with: pulse service stop");
    }

    let shutdown = Arc::clone(&state.shutdown);
    let handler = Arc::clone(&state);
    let result = pipe::serve_loop(&pipe_name, handler, shutdown);

    pipeline.stop();
    reminder_scheduler.stop();
    let _ = remove_pid_file_if_matches(&paths.service_pid_path(), state.pid, Some(&exe_path));
    result?;
    Ok(())
}

/// Local-only reminder scheduler. A due item is recorded once per daemon run
/// and surfaced by the desktop pet, which owns the user-visible actions.
fn start_reminder_scheduler(store: Arc<Mutex<Store>>) -> ReminderScheduler {
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        // Keep the scheduled timestamp, rather than merely the reminder id: a
        // snoozed reminder is deliberately eligible to fire again at its new time.
        let mut surfaced = HashMap::new();
        while !worker_stop.load(Ordering::SeqCst) {
            if let Ok(store) = store.lock() {
                if let Ok(reminders) = store.list_due_reminders(Utc::now()) {
                    for reminder in reminders {
                        if surfaced.get(&reminder.id) != Some(&reminder.due_at) {
                            surfaced.insert(reminder.id, reminder.due_at);
                            let payload = json!({
                                "reminder_id": reminder.id,
                                "actions": ["open_context", "continue_coding", "snooze", "done"],
                            }).to_string();
                            let _ = store.record_event(pulse_core::NewActivityEvent {
                                task_id: reminder.task_id,
                                session_id: None,
                                kind: "reminder_due".into(),
                                summary: format!("Reminder due: {}", reminder.title),
                                payload_json: Some(payload),
                                source_ref: Some("local_scheduler".into()),
                                occurred_at: Utc::now(),
                            });
                            eprintln!("pulse reminder due: {}", reminder.title);
                        }
                    }
                }
            }
            for _ in 0..10 { if worker_stop.load(Ordering::SeqCst) { break; } thread::sleep(Duration::from_secs(1)); }
        }
    });
    ReminderScheduler { stop, handle: Some(handle) }
}

struct ReminderScheduler { stop: Arc<AtomicBool>, handle: Option<thread::JoinHandle<()>> }
impl ReminderScheduler {
    fn stop(mut self) { self.stop.store(true, Ordering::SeqCst); if let Some(handle) = self.handle.take() { let _ = handle.join(); } }
}

struct ServiceState {
    paths: PulsePaths,
    store: Arc<Mutex<Store>>,
    config: Arc<Mutex<Config>>,
    shutdown: Arc<AtomicBool>,
    started_at: chrono::DateTime<Utc>,
    pid: u32,
}

impl RpcHandler for ServiceState {
    fn handle(&self, method: &str, params: Value) -> Result<Value, RpcErrorObject> {
        match method {
            "ping" => Ok(json!({
                "ok": true,
                "version": env!("CARGO_PKG_VERSION"),
            })),
            "service.status" => {
                let cfg = self.config.lock().map_err(|_| internal("config lock"))?;
                let st = llm_status(&cfg.llm, &cfg.privacy);
                Ok(json!({
                    "ok": true,
                    "version": env!("CARGO_PKG_VERSION"),
                    "pid": self.pid,
                    "pipe_name": cfg.service.pipe_name,
                    "started_at": self.started_at.to_rfc3339(),
                    "data_dir": self.paths.root,
                    "llm_mode": st.backend_id,
                    "llm_path": st.path,
                    "llm_reason": st.reason,
                    "privacy_ack": st.privacy_ack,
                    "queue_depth": 0,
                    "sources": {
                        "claude": cfg.sources.claude.enabled,
                        "codex": cfg.sources.codex.enabled,
                    },
                    "inference_enabled": cfg.inference.enabled,
                }))
            }
            "service.shutdown" => {
                self.shutdown.store(true, Ordering::SeqCst);
                let pipe = self
                    .config
                    .lock()
                    .map(|c| c.service.pipe_name.clone())
                    .unwrap_or_else(|_| "pulse-service".into());
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(50));
                    pipe::poke(&pipe);
                });
                Ok(json!({ "ok": true }))
            }
            "config.reload" => {
                let cfg = load_config(&self.paths.config_path())
                    .map_err(|e| RpcErrorObject::new(RpcCode::ConfigError, e.to_string()))?;
                let mut guard = self.config.lock().map_err(|_| internal("config lock"))?;
                *guard = cfg;
                Ok(json!({ "ok": true }))
            }
            "sources.list" => {
                let cfg = self.config.lock().map_err(|_| internal("config lock"))?;
                Ok(json!({
                    "sources": [
                        {
                            "id": "claude",
                            "enabled": cfg.sources.claude.enabled,
                            "extra_roots": cfg.sources.claude.extra_roots,
                        },
                        {
                            "id": "codex",
                            "enabled": cfg.sources.codex.enabled,
                            "extra_roots": cfg.sources.codex.extra_roots,
                        }
                    ]
                }))
            }
            "sources.set_enabled" => {
                let id = param_str(&params, "id")?;
                let enabled = params
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| {
                        RpcErrorObject::new(RpcCode::InvalidParams, "missing bool 'enabled'")
                    })?;
                let mut cfg = self.config.lock().map_err(|_| internal("config lock"))?;
                match id.as_str() {
                    "claude" => cfg.sources.claude.enabled = enabled,
                    "codex" => cfg.sources.codex.enabled = enabled,
                    other => {
                        return Err(RpcErrorObject::new(
                            RpcCode::InvalidParams,
                            format!("unknown source '{other}'"),
                        ));
                    }
                }
                write_config(&self.paths.config_path(), &cfg)
                    .map_err(|e| RpcErrorObject::new(RpcCode::ConfigError, e.to_string()))?;
                Ok(json!({ "ok": true, "id": id, "enabled": enabled }))
            }
            "inference.run_once" => {
                let cfg = self
                    .config
                    .lock()
                    .map_err(|_| internal("config lock"))?
                    .clone();
                let mut store = self.store.lock().map_err(|_| internal("store lock"))?;
                let mut last = std::collections::HashMap::new();
                let mut hour = 0u32;
                let n = pipeline::run_once(&mut store, &cfg, &mut last, &mut hour)
                    .map_err(|e| RpcErrorObject::new(RpcCode::InternalError, e))?;
                Ok(json!({ "ok": true, "created": n }))
            }
            "privacy.acknowledge" => {
                let mut cfg = self.config.lock().map_err(|_| internal("config lock"))?;
                cfg.privacy.acknowledge_remote_llm = true;
                write_config(&self.paths.config_path(), &cfg)
                    .map_err(|e| RpcErrorObject::new(RpcCode::ConfigError, e.to_string()))?;
                Ok(json!({ "ok": true, "acknowledge_remote_llm": true }))
            }
            "llm.status" => {
                let cfg = self.config.lock().map_err(|_| internal("config lock"))?;
                let st = llm_status(&cfg.llm, &cfg.privacy);
                Ok(json!({
                    "backend_id": st.backend_id,
                    "path": st.path,
                    "privacy_ack": st.privacy_ack,
                    "provider_setting": st.provider_setting,
                    "reason": st.reason,
                    "preference": cfg.llm.preference,
                }))
            }
            "summary.generate" => {
                let day = params
                    .get("date")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
                let cfg = self
                    .config
                    .lock()
                    .map_err(|_| internal("config lock"))?
                    .clone();
                let mut store = self.store.lock().map_err(|_| internal("store lock"))?;
                let text = pipeline::generate_summary(&mut store, &cfg, &day)
                    .map_err(|e| RpcErrorObject::new(RpcCode::InternalError, e))?;
                let summary = store
                    .get_summary(&day)
                    .map_err(map_store_err)?
                    .ok_or_else(|| {
                        RpcErrorObject::new(RpcCode::InternalError, "summary missing")
                    })?;
                Ok(json!({ "summary": summary, "text": text }))
            }
            "summary.get" => {
                let day = param_str(&params, "date")?;
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let summary = store.get_summary(&day).map_err(map_store_err)?;
                Ok(json!({ "summary": summary }))
            }
            "checkin.list" => {
                let open_only = params
                    .get("open_only")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let items = store.list_checkins(open_only).map_err(map_store_err)?;
                Ok(json!({ "items": items }))
            }
            "checkin.answer" => {
                let id = param_str(&params, "id")?;
                let answer = params
                    .get("answer")
                    .cloned()
                    .ok_or_else(|| RpcErrorObject::new(RpcCode::InvalidParams, "missing answer"))?;
                // Allow string answers via JSON string
                let answer = if let Some(s) = answer.as_str() {
                    parse_answer_input(s).map_err(map_store_err)?
                } else {
                    answer
                };
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let checkin = store.resolve_checkin(&id).map_err(map_store_err)?;
                let patch = apply_checkin_answer(checkin.kind, &answer).map_err(map_store_err)?;
                let task = if let Some(tid) = checkin.task_id {
                    Some(store.update_task(tid, patch).map_err(map_store_err)?)
                } else {
                    None
                };
                let answered = store
                    .answer_checkin(checkin.id, &answer.to_string())
                    .map_err(map_store_err)?;
                Ok(json!({ "ok": true, "checkin": answered, "task": task }))
            }
            "export.history" => {
                let format = params
                    .get("format")
                    .and_then(|v| v.as_str())
                    .unwrap_or("json");
                let fmt = ExportFormat::parse(format).ok_or_else(|| {
                    RpcErrorObject::new(RpcCode::InvalidParams, "format must be json or md")
                })?;
                let from = params.get("from").and_then(|v| v.as_str());
                let to = params.get("to").and_then(|v| v.as_str());
                let out = params
                    .get("out")
                    .and_then(|v| v.as_str())
                    .map(std::path::PathBuf::from);
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let path = export_history(&store, &self.paths, fmt, from, to, out.as_deref())
                    .map_err(map_store_err)?;
                Ok(json!({ "path": path }))
            }
            "activities.create" => {
                let title = param_str(&params, "title")?;
                let notes = params
                    .get("notes")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let mut new = NewTask::manual(title);
                new.notes = notes;
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let activity = store.create_task(new).map_err(map_store_err)?;
                Ok(json!({ "activity": activity }))
            }
            "sessions.attach" => {
                let activity_id = param_str(&params, "activity_id")?;
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let activity = store.resolve_task(&activity_id).map_err(map_store_err)?;
                let metadata_json = params
                    .get("metadata")
                    .cloned()
                    .unwrap_or_else(|| json!({}))
                    .to_string();
                let session = store
                    .create_session(NewSession {
                        task_id: activity.id,
                        agent: optional_str(&params, "agent"),
                        application: optional_str(&params, "application"),
                        repository_path: optional_str(&params, "repository_path"),
                        external_id: optional_str(&params, "external_id"),
                        source_ref: optional_str(&params, "source_ref"),
                        started_at: Utc::now(),
                        ended_at: None,
                        metadata_json,
                    })
                    .map_err(map_store_err)?;
                Ok(json!({ "session": session }))
            }
            "checkpoints.create" => {
                let activity_id = param_str(&params, "activity_id")?;
                let summary = param_str(&params, "summary")?;
                let session_id = optional_uuid(&params, "session_id")?;
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let activity = store.resolve_task(&activity_id).map_err(map_store_err)?;
                let checkpoint = store
                    .create_checkpoint(NewCheckpoint {
                        task_id: activity.id,
                        session_id,
                        summary,
                        decisions: string_list(&params, "decisions")?,
                        failures: string_list(&params, "failures")?,
                        next_actions: string_list(&params, "next_actions")?,
                        source_ref: optional_str(&params, "source_ref"),
                    })
                    .map_err(map_store_err)?;
                Ok(json!({ "checkpoint": checkpoint }))
            }
            "activities.timeline" => {
                let activity_id = param_str(&params, "activity_id")?;
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let activity = store.resolve_task(&activity_id).map_err(map_store_err)?;
                let evidence = store.list_evidence(activity.id).map_err(map_store_err)?;
                let sessions = store.list_sessions(activity.id).map_err(map_store_err)?;
                let events = store.list_events(activity.id).map_err(map_store_err)?;
                let checkpoints = store.list_checkpoints(activity.id).map_err(map_store_err)?;
                let reminders = store.list_reminders(activity.id).map_err(map_store_err)?;
                let memories = store.list_memories(activity.id).map_err(map_store_err)?;
                let artifacts = store.list_artifacts(activity.id).map_err(map_store_err)?;
                Ok(json!({
                    "activity": activity,
                    "evidence": evidence,
                    "sessions": sessions,
                    "events": events,
                    "checkpoints": checkpoints,
                    "reminders": reminders,
                    "memories": memories,
                    "artifacts": artifacts,
                }))
            }
            "tasks.list" => {
                let status = parse_status_filter(&params)?;
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let tasks = store.list_tasks(status).map_err(map_store_err)?;
                Ok(json!({ "tasks": tasks }))
            }
            "tasks.get" => {
                let id = param_str(&params, "id")?;
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let task = store.resolve_task(&id).map_err(map_store_err)?;
                let evidence = store.list_evidence(task.id).map_err(map_store_err)?;
                Ok(json!({ "task": task, "evidence": evidence }))
            }
            "tasks.create" => {
                let title = param_str(&params, "title")?;
                let status = match params.get("status") {
                    None | Some(Value::Null) => TaskStatus::Inbox,
                    Some(v) => parse_status_value(v)?,
                };
                if !matches!(status, TaskStatus::Inbox | TaskStatus::Today) {
                    return Err(RpcErrorObject::new(
                        RpcCode::InvalidParams,
                        "create status must be Inbox or Today",
                    ));
                }
                let notes = params
                    .get("notes")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let mut new = NewTask::manual(title);
                new.status = status;
                new.notes = notes;
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let task = store.create_task(new).map_err(map_store_err)?;
                Ok(json!({ "task": task }))
            }
            "tasks.update" => {
                let id = param_str(&params, "id")?;
                let title = params
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let status = match params.get("status") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(parse_status_value(v)?),
                };
                let notes = params
                    .get("notes")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let task = store.resolve_task(&id).map_err(map_store_err)?;
                let task = store
                    .update_task(
                        task.id,
                        TaskUpdate {
                            title,
                            status,
                            notes,
                            ..Default::default()
                        },
                    )
                    .map_err(map_store_err)?;
                Ok(json!({ "task": task }))
            }
            "tasks.done" => {
                let id = param_str(&params, "id")?;
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let task = store.resolve_task(&id).map_err(map_store_err)?;
                let task = store.mark_done(task.id).map_err(map_store_err)?;
                Ok(json!({ "task": task }))
            }
            _ => Err(RpcErrorObject::new(
                RpcCode::MethodNotFound,
                format!("method not found: {method}"),
            )),
        }
    }
}

fn param_str(params: &Value, key: &str) -> Result<String, RpcErrorObject> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            RpcErrorObject::new(
                RpcCode::InvalidParams,
                format!("missing string param '{key}'"),
            )
        })
}

fn optional_str(params: &Value, key: &str) -> Option<String> {
    params.get(key).and_then(|v| v.as_str()).map(str::to_owned)
}

fn optional_uuid(params: &Value, key: &str) -> Result<Option<uuid::Uuid>, RpcErrorObject> {
    let Some(value) = params.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let raw = value.as_str().ok_or_else(|| {
        RpcErrorObject::new(
            RpcCode::InvalidParams,
            format!("{key} must be a UUID string"),
        )
    })?;
    uuid::Uuid::parse_str(raw).map(Some).map_err(|_| {
        RpcErrorObject::new(
            RpcCode::InvalidParams,
            format!("{key} must be a valid UUID"),
        )
    })
}

fn string_list(params: &Value, key: &str) -> Result<Vec<String>, RpcErrorObject> {
    let Some(value) = params.get(key) else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| {
        RpcErrorObject::new(
            RpcCode::InvalidParams,
            format!("{key} must be an array of strings"),
        )
    })?;
    values
        .iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                RpcErrorObject::new(
                    RpcCode::InvalidParams,
                    format!("{key} must be an array of strings"),
                )
            })
        })
        .collect()
}

fn parse_status_value(v: &Value) -> Result<TaskStatus, RpcErrorObject> {
    let s = v
        .as_str()
        .ok_or_else(|| RpcErrorObject::new(RpcCode::InvalidParams, "status must be a string"))?;
    TaskStatus::parse(s)
        .ok_or_else(|| RpcErrorObject::new(RpcCode::InvalidParams, format!("invalid status '{s}'")))
}

fn parse_status_filter(params: &Value) -> Result<Option<TaskStatus>, RpcErrorObject> {
    match params.get("status") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(arr)) if arr.is_empty() => Ok(None),
        Some(Value::Array(arr)) => parse_status_value(&arr[0]).map(Some),
        Some(v) => parse_status_value(v).map(Some),
    }
}

fn map_store_err(e: PulseError) -> RpcErrorObject {
    match e {
        PulseError::TaskNotFound(ref id) => {
            RpcErrorObject::with_data(RpcCode::TaskNotFound, e.to_string(), json!({ "id": id }))
        }
        PulseError::InvalidTransition { .. } => {
            RpcErrorObject::new(RpcCode::InvalidTransition, e.to_string())
        }
        PulseError::Validation(msg) => RpcErrorObject::new(RpcCode::InvalidParams, msg),
        PulseError::AmbiguousTaskId(p) => {
            RpcErrorObject::new(RpcCode::InvalidParams, format!("ambiguous id prefix '{p}'"))
        }
        PulseError::Config(msg) => RpcErrorObject::new(RpcCode::ConfigError, msg),
        other => RpcErrorObject::new(RpcCode::InternalError, other.to_string()),
    }
}

fn internal(msg: &str) -> RpcErrorObject {
    RpcErrorObject::new(RpcCode::InternalError, msg)
}
