//! Pulse background service: named-pipe JSON-RPC server + source inference poller.

use std::collections::{HashMap, HashSet};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::Utc;
use clap::{Parser, Subcommand};
use pulse_core::ipc::pid::{remove_pid_file_if_matches, write_pid_file, ServicePidFile};
use pulse_core::ipc::pipe::{self, current_pid};
use pulse_core::ipc::rpc::{RpcCode, RpcErrorObject, RpcHandler};
use pulse_core::{
    apply_checkin_answer, export_history, load_config, open_db, parse_answer_input, write_config,
    Config, ExportFormat, NewCheckpoint, NewSession, NewTask, PulseError, PulsePaths, Store,
    SyncOutcome, Task, TaskStatus, TaskUpdate,
};
use pulse_llm::{
    llm_status, resolve_llm_client, HuggingFaceEmbeddingClient, TaskCopilotAgentRequest,
    TaskCopilotStep, TaskCopilotToolResult,
};
use pulse_service::{
    copilot::{CopilotMemorySearch, CopilotToolContext, CopilotToolRegistry},
    pipeline, sync,
};
use serde_json::{json, Value};
use tungstenite::{accept, Message};
use uuid::Uuid;

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

/// A local, ephemeral event stream for one copilot request. The listener binds
/// only to loopback and subscriptions require the per-operation token returned
/// by `copilot.start`.
#[derive(Default)]
struct CopilotProgressBroker {
    operations: Mutex<HashMap<Uuid, CopilotOperation>>,
}

struct CopilotOperation {
    token: String,
    events: Vec<Value>,
    complete: bool,
    subscribers: Vec<mpsc::Sender<Value>>,
}

impl CopilotProgressBroker {
    fn create(&self) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let token = Uuid::new_v4().to_string();
        let mut operations = self.operations.lock().expect("copilot progress lock");
        // Completed operations are only useful long enough for the client to
        // finish connecting. Keep the broker bounded during a long-lived daemon.
        if operations.len() > 100 {
            operations.retain(|_, operation| !operation.complete);
        }
        operations.insert(
            id,
            CopilotOperation {
                token: token.clone(),
                events: Vec::new(),
                complete: false,
                subscribers: Vec::new(),
            },
        );
        (id, token)
    }

    fn publish(&self, id: Uuid, event: Value, complete: bool) {
        let Ok(mut operations) = self.operations.lock() else {
            return;
        };
        let Some(operation) = operations.get_mut(&id) else {
            return;
        };
        operation.events.push(event.clone());
        operation.events = operation
            .events
            .drain(operation.events.len().saturating_sub(50)..)
            .collect();
        operation.complete |= complete;
        operation
            .subscribers
            .retain(|subscriber| subscriber.send(event.clone()).is_ok());
        if operation.complete {
            operation.subscribers.clear();
        }
    }

    fn subscribe(
        &self,
        id: Uuid,
        token: &str,
    ) -> Result<(Vec<Value>, mpsc::Receiver<Value>, bool), String> {
        let (sender, receiver) = mpsc::channel();
        let mut operations = self
            .operations
            .lock()
            .map_err(|_| "copilot progress lock failed")?;
        let operation = operations
            .get_mut(&id)
            .ok_or("copilot operation was not found")?;
        if operation.token != token {
            return Err("copilot operation token is invalid".into());
        }
        let complete = operation.complete;
        let events = operation.events.clone();
        if !complete {
            operation.subscribers.push(sender);
        }
        Ok((events, receiver, complete))
    }
}

#[derive(serde::Deserialize)]
struct CopilotSocketSubscription {
    operation_id: Uuid,
    token: String,
}

fn start_copilot_progress_server(
    broker: Arc<CopilotProgressBroker>,
) -> Result<u16, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let broker = Arc::clone(&broker);
            thread::spawn(move || serve_copilot_progress_socket(stream, broker));
        }
    });
    Ok(port)
}

fn serve_copilot_progress_socket(stream: TcpStream, broker: Arc<CopilotProgressBroker>) {
    let Ok(mut socket) = accept(stream) else {
        return;
    };
    let subscription = match socket.read() {
        Ok(Message::Text(text)) => serde_json::from_str::<CopilotSocketSubscription>(&text).ok(),
        _ => None,
    };
    let Some(subscription) = subscription else {
        let _ = socket.close(None);
        return;
    };
    let Ok((events, receiver, complete)) =
        broker.subscribe(subscription.operation_id, &subscription.token)
    else {
        let _ = socket.close(None);
        return;
    };
    for event in events {
        if socket.send(Message::Text(event.to_string())).is_err() {
            return;
        }
    }
    if complete {
        let _ = socket.close(None);
        return;
    }
    while let Ok(event) = receiver.recv() {
        let terminal = matches!(
            event.get("event").and_then(Value::as_str),
            Some("final") | Some("error")
        );
        if socket.send(Message::Text(event.to_string())).is_err() || terminal {
            let _ = socket.close(None);
            return;
        }
    }
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

    let copilot_progress = Arc::new(CopilotProgressBroker::default());
    let copilot_ws_port = start_copilot_progress_server(Arc::clone(&copilot_progress))?;
    let state = Arc::new(ServiceState {
        paths: paths.clone(),
        store: Arc::clone(&store),
        config: Arc::clone(&config),
        session_sync_active: Arc::new(AtomicBool::new(false)),
        semantic_search_embeddings: Arc::new(Mutex::new(None)),
        copilot_progress,
        copilot_ws_port,
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
    let session_sync_scheduler = start_session_sync_scheduler(
        paths.clone(),
        Arc::clone(&config),
        Arc::clone(&state.session_sync_active),
    );
    let sync_worker =
        pulse_service::sync::start_sync_worker(Arc::clone(&store), Arc::clone(&config));

    if !quiet {
        eprintln!(
            "pulse-service listening on pipe '{}' (pid {})",
            pipe_name, state.pid
        );
        eprintln!("inference poller active (enable sources via `pulse sources enable`)");
        eprintln!("copilot progress available on local WebSocket port {copilot_ws_port}");
        eprintln!("stop with: pulse service stop");
    }

    let shutdown = Arc::clone(&state.shutdown);
    let handler = Arc::clone(&state);
    let result = pipe::serve_loop(&pipe_name, handler, shutdown);

    pipeline.stop();
    reminder_scheduler.stop();
    session_sync_scheduler.stop();
    sync_worker.stop();
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
                            })
                            .to_string();
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
            for _ in 0..10 {
                if worker_stop.load(Ordering::SeqCst) {
                    break;
                }
                thread::sleep(Duration::from_secs(1));
            }
        }
    });
    ReminderScheduler {
        stop,
        handle: Some(handle),
    }
}

struct ReminderScheduler {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

/// Reviews recently modified source sessions once an hour while the local
/// service is running. The first review waits an hour so startup never causes
/// unexpected remote analysis; users can still request an immediate sync.
fn start_session_sync_scheduler(
    paths: PulsePaths,
    config: Arc<Mutex<Config>>,
    session_sync_active: Arc<AtomicBool>,
) -> SessionSyncScheduler {
    const SYNC_INTERVAL: Duration = Duration::from_secs(60 * 60);

    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        while !worker_stop.load(Ordering::SeqCst) {
            let mut waited = Duration::ZERO;
            while waited < SYNC_INTERVAL && !worker_stop.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_secs(1));
                waited += Duration::from_secs(1);
            }
            if worker_stop.load(Ordering::SeqCst) {
                break;
            }

            // Manual and scheduled syncs must never analyze the same
            // transcripts concurrently. If a manual sync is underway, the
            // next hourly pass will pick up any remaining changes.
            if session_sync_active.swap(true, Ordering::SeqCst) {
                eprintln!("skipping scheduled session sync: another sync is in progress");
                continue;
            }
            struct ResetSessionSync<'a>(&'a AtomicBool);
            impl Drop for ResetSessionSync<'_> {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::SeqCst);
                }
            }
            let _reset_session_sync = ResetSessionSync(&session_sync_active);

            let cfg = match config.lock() {
                Ok(config) => config.clone(),
                Err(_) => {
                    eprintln!("skipping scheduled session sync: configuration lock unavailable");
                    continue;
                }
            };
            let result = open_db(&paths.db_path())
                .map(Store::new)
                .map_err(|error| error.to_string())
                .and_then(|mut store| pipeline::sync_recent_sessions(&mut store, &cfg));
            match result {
                Ok(result) => eprintln!(
                    "scheduled session sync complete: {} added, {} updated, {} unchanged",
                    result.tasks_created, result.tasks_updated, result.sessions_skipped_unchanged
                ),
                Err(error) => eprintln!("scheduled session sync failed: {error}"),
            }
        }
    });

    SessionSyncScheduler {
        stop,
        handle: Some(handle),
    }
}

struct SessionSyncScheduler {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl SessionSyncScheduler {
    fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
impl ReminderScheduler {
    fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

struct ServiceState {
    paths: PulsePaths,
    store: Arc<Mutex<Store>>,
    config: Arc<Mutex<Config>>,
    session_sync_active: Arc<AtomicBool>,
    semantic_search_embeddings: Arc<Mutex<Option<HuggingFaceEmbeddingClient>>>,
    copilot_progress: Arc<CopilotProgressBroker>,
    copilot_ws_port: u16,
    shutdown: Arc<AtomicBool>,
    started_at: chrono::DateTime<Utc>,
    pid: u32,
}

/// Service-side adapter for the read-only Copilot memory tool. It deliberately
/// reuses the authenticated sync API instead of exposing CockroachDB credentials
/// or a network endpoint to the desktop renderer.
struct CloudCopilotMemory {
    config: Config,
    embeddings: Arc<Mutex<Option<HuggingFaceEmbeddingClient>>>,
}

impl CopilotMemorySearch for CloudCopilotMemory {
    fn search(&self, query: &str, limit: usize) -> Result<Vec<sync::SemanticSearchHit>, String> {
        let mut embeddings = self
            .embeddings
            .lock()
            .map_err(|_| "semantic search embedding lock unavailable".to_string())?;
        if embeddings.is_none() {
            *embeddings = Some(
                HuggingFaceEmbeddingClient::from_config(&self.config.embeddings)
                    .map_err(|error| format!("initialize semantic search: {error}"))?,
            );
        }
        sync::semantic_search(
            &self.config,
            query,
            limit,
            embeddings.as_mut().expect("embedding client initialized"),
        )
    }
}

fn publish_copilot_event(
    broker: &CopilotProgressBroker,
    operation_id: Uuid,
    event: &str,
    message: impl Into<String>,
    complete: bool,
) {
    broker.publish(
        operation_id,
        json!({ "event": event, "message": message.into() }),
        complete,
    );
}

fn run_copilot_agent(
    operation_id: Uuid,
    conversation_id: Uuid,
    query: String,
    cfg: Config,
    store: Arc<Mutex<Store>>,
    broker: Arc<CopilotProgressBroker>,
    tools: CopilotToolRegistry,
    cloud_memory: Option<Arc<dyn CopilotMemorySearch>>,
) {
    publish_copilot_event(
        &broker,
        operation_id,
        "status",
        "Planning the best task lookup…",
        false,
    );
    let client = resolve_llm_client(&cfg.llm, &cfg.privacy);
    let backend = client.backend_id().to_string();
    let mut transcript = Vec::new();
    let mut available_tasks = HashMap::new();
    let mut tool_calls = 0u8;
    let tool_definitions = tools.definitions();

    loop {
        let step = client.task_copilot_step(&TaskCopilotAgentRequest {
            query: query.clone(),
            tools: tool_definitions.clone(),
            transcript: transcript.clone(),
            remaining_tool_calls: 2u8.saturating_sub(tool_calls),
        });
        let step = match step {
            Ok(step) => step,
            Err(error) => {
                persist_copilot_reply(
                    &store,
                    conversation_id,
                    &format!("Copilot could not continue: {error}"),
                    Some(&backend),
                    &[],
                );
                publish_copilot_event(
                    &broker,
                    operation_id,
                    "error",
                    format!("Copilot could not continue: {error}"),
                    true,
                );
                return;
            }
        };
        match step {
            TaskCopilotStep::Final {
                answer,
                cited_task_ids,
            } => {
                let mut seen = HashSet::new();
                let tasks = cited_task_ids
                    .into_iter()
                    .filter(|id| seen.insert(id.clone()))
                    .filter_map(|id| available_tasks.get(&id).cloned())
                    .collect::<Vec<_>>();
                persist_copilot_reply(&store, conversation_id, &answer, Some(&backend), &tasks);
                broker.publish(
                    operation_id,
                    json!({
                        "event": "final",
                        "result": { "answer": answer, "tasks": tasks, "backend": backend },
                    }),
                    true,
                );
                return;
            }
            TaskCopilotStep::ToolCall { tool, arguments } if tool_calls < 2 => {
                tool_calls += 1;
                let tool_label = tools.progress_label(&tool);
                publish_copilot_event(&broker, operation_id, "tool_call", tool_label, false);
                let execution = match store.lock() {
                    Ok(store) => tools.execute(
                        CopilotToolContext {
                            store: &store,
                            cloud_memory: cloud_memory.as_deref(),
                        },
                        &tool,
                        &arguments,
                    ),
                    Err(_) => Err("task store is unavailable".into()),
                };
                let (payload, tasks, progress) = match execution {
                    Ok(execution) => {
                        let count = execution.tasks.len();
                        (
                            execution.payload,
                            execution.tasks,
                            format!(
                                "{tool_label} finished with {count} task{}.",
                                if count == 1 { "" } else { "s" }
                            ),
                        )
                    }
                    Err(error) => (
                        json!({ "error": error }),
                        Vec::new(),
                        format!("{tool_label} could not complete."),
                    ),
                };
                for task in &tasks {
                    available_tasks.insert(task.id.to_string(), task.clone());
                }
                broker.publish(
                    operation_id,
                    json!({ "event": "tool_result", "tool": tool, "message": progress }),
                    false,
                );
                transcript.push(TaskCopilotToolResult {
                    tool,
                    result: payload,
                });
                publish_copilot_event(
                    &broker,
                    operation_id,
                    "status",
                    "Reviewing the retrieved task context…",
                    false,
                );
            }
            TaskCopilotStep::ToolCall { .. } => {
                let tasks = available_tasks.into_values().take(3).collect::<Vec<_>>();
                let answer: String = if tasks.is_empty() {
                    "I used the available task tools but did not find enough task context to answer that.".into()
                } else {
                    "I completed the two available task operations. Here are the tasks that support the answer.".into()
                };
                persist_copilot_reply(&store, conversation_id, &answer, Some(&backend), &tasks);
                broker.publish(
                    operation_id,
                    json!({
                        "event": "final",
                        "result": { "answer": answer, "tasks": tasks, "backend": backend },
                    }),
                    true,
                );
                return;
            }
        }
    }
}

fn persist_copilot_reply(
    store: &Arc<Mutex<Store>>,
    conversation_id: Uuid,
    content: &str,
    backend: Option<&str>,
    tasks: &[Task],
) {
    let task_refs_json = serde_json::to_string(tasks).unwrap_or_else(|_| "[]".into());
    if let Ok(store) = store.lock() {
        let _ = store.append_copilot_message(
            conversation_id,
            "assistant",
            content,
            backend,
            &task_refs_json,
        );
    }
}

fn copilot_history_message(message: pulse_core::CopilotMessage) -> Value {
    let tasks: Vec<Task> = serde_json::from_str(&message.task_refs_json).unwrap_or_default();
    json!({
        "id": message.id,
        "role": message.role,
        "content": message.content,
        "backend": message.backend,
        "tasks": tasks,
        "created_at": message.created_at,
    })
}

fn copilot_conversation_title(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.chars().count() <= 80 {
        trimmed.into()
    } else {
        format!("{}…", trimmed.chars().take(79).collect::<String>())
    }
}

impl RpcHandler for ServiceState {
    fn handle(&self, method: &str, params: Value) -> Result<Value, RpcErrorObject> {
        // Session analysis can take minutes while the configured CLI runs. Do
        // not let normal UI requests wait behind its database lock: callers
        // get a quick, retryable response instead of appearing hung.
        if method != "inference.sync_recent"
            && method != "copilot.start"
            && self.session_sync_active.load(Ordering::SeqCst)
        {
            return Err(RpcErrorObject::new(
                RpcCode::ServiceBusy,
                "Session sync is in progress; please try again shortly.",
            ));
        }
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
                    "queue_depth": self.store.lock().ok().and_then(|store| store.pending_sync_count().ok()).unwrap_or(0),
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
            "inference.sync_recent" => {
                if self.session_sync_active.swap(true, Ordering::SeqCst) {
                    return Err(RpcErrorObject::new(
                        RpcCode::ServiceBusy,
                        "A session sync is already in progress.",
                    ));
                }
                struct ResetSessionSync<'a>(&'a AtomicBool);
                impl Drop for ResetSessionSync<'_> {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::SeqCst);
                    }
                }
                let _reset_session_sync = ResetSessionSync(&self.session_sync_active);
                let cfg = self
                    .config
                    .lock()
                    .map_err(|_| internal("config lock"))?
                    .clone();
                // Session discovery and remote analysis can take minutes.  Do
                // not borrow the service's shared Store for that whole time:
                // regular UI RPCs (tasks list/detail, reminders, etc.) would
                // otherwise wait on its mutex and make the desktop app appear
                // unresponsive. A dedicated WAL connection only holds SQLite
                // locks for its individual write statements.
                let conn = open_db(&self.paths.db_path())
                    .map_err(|e| RpcErrorObject::new(RpcCode::InternalError, e.to_string()))?;
                let mut store = Store::new(conn);
                let result = pipeline::sync_recent_sessions(&mut store, &cfg)
                    .map_err(|e| RpcErrorObject::new(RpcCode::InvalidParams, e))?;
                Ok(serde_json::to_value(result).unwrap_or_else(|_| json!({})))
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
                    "task": activity,
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
            "copilot.start" => {
                let query = param_str(&params, "query")?;
                if query.trim().is_empty() {
                    return Err(RpcErrorObject::new(
                        RpcCode::InvalidParams,
                        "query must not be empty",
                    ));
                }
                if query.chars().count() > 1_000 {
                    return Err(RpcErrorObject::new(
                        RpcCode::InvalidParams,
                        "query must be 1000 characters or fewer",
                    ));
                }
                let conversation_id = {
                    let store = self.store.lock().map_err(|_| internal("store lock"))?;
                    let conversation = match optional_str(&params, "conversation_id") {
                        Some(id) => {
                            let id = Uuid::parse_str(&id).map_err(|_| {
                                RpcErrorObject::new(
                                    RpcCode::InvalidParams,
                                    "invalid conversation id",
                                )
                            })?;
                            store
                                .get_copilot_conversation(id)
                                .map_err(map_store_err)?
                                .ok_or_else(|| {
                                    RpcErrorObject::new(
                                        RpcCode::InvalidParams,
                                        "copilot conversation not found",
                                    )
                                })?
                        }
                        None => store
                            .create_copilot_conversation(&copilot_conversation_title(&query))
                            .map_err(map_store_err)?,
                    };
                    store
                        .append_copilot_message(conversation.id, "user", &query, None, "[]")
                        .map_err(map_store_err)?;
                    conversation.id
                };
                let cfg = self
                    .config
                    .lock()
                    .map_err(|_| internal("config lock"))?
                    .clone();
                let (operation_id, token) = self.copilot_progress.create();
                let broker = Arc::clone(&self.copilot_progress);
                let store = Arc::clone(&self.store);
                let cloud_memory = cfg.sync.enabled.then(|| {
                    Arc::new(CloudCopilotMemory {
                        config: cfg.clone(),
                        embeddings: Arc::clone(&self.semantic_search_embeddings),
                    }) as Arc<dyn CopilotMemorySearch>
                });
                let tools = CopilotToolRegistry::task_tools(cloud_memory.is_some());
                thread::spawn(move || {
                    run_copilot_agent(
                        operation_id,
                        conversation_id,
                        query,
                        cfg,
                        store,
                        broker,
                        tools,
                        cloud_memory,
                    )
                });
                Ok(json!({
                    "operation_id": operation_id,
                    "conversation_id": conversation_id,
                    "token": token,
                    "websocket_url": format!("ws://127.0.0.1:{}", self.copilot_ws_port),
                }))
            }
            "copilot.sessions.list" => {
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let sessions = store
                    .list_recent_copilot_conversations(5)
                    .map_err(map_store_err)?;
                Ok(json!({ "sessions": sessions }))
            }
            "copilot.sessions.get" => {
                let id = param_str(&params, "id")?;
                let id = Uuid::parse_str(&id).map_err(|_| {
                    RpcErrorObject::new(RpcCode::InvalidParams, "invalid conversation id")
                })?;
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let session = store
                    .get_copilot_conversation(id)
                    .map_err(map_store_err)?
                    .ok_or_else(|| {
                        RpcErrorObject::new(
                            RpcCode::InvalidParams,
                            "copilot conversation not found",
                        )
                    })?;
                let messages = store
                    .list_copilot_messages(id)
                    .map_err(map_store_err)?
                    .into_iter()
                    .map(copilot_history_message)
                    .collect::<Vec<_>>();
                Ok(json!({ "session": session, "messages": messages }))
            }
            "activities.semantic_search" => {
                let query = param_str(&params, "query")?;
                let limit = params
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(10)
                    .clamp(1, 50) as usize;
                let cfg = self
                    .config
                    .lock()
                    .map_err(|_| internal("config lock"))?
                    .clone();
                let mut embeddings = self
                    .semantic_search_embeddings
                    .lock()
                    .map_err(|_| internal("semantic search embedding lock"))?;
                if embeddings.is_none() {
                    *embeddings = Some(
                        HuggingFaceEmbeddingClient::from_config(&cfg.embeddings).map_err(|e| {
                            RpcErrorObject::new(
                                RpcCode::Unavailable,
                                format!("semantic search is unavailable: {e}"),
                            )
                        })?,
                    );
                }
                let hits = sync::semantic_search(
                    &cfg,
                    &query,
                    limit,
                    embeddings.as_mut().expect("embedding client initialized"),
                )
                .map_err(|e| RpcErrorObject::new(RpcCode::Unavailable, e))?;
                drop(embeddings);

                // Multiple matching checkpoints or memories may point to the
                // same activity. Keep its closest match so Inbox presents a
                // concise activity-level result list.
                let mut closest_by_activity = HashMap::new();
                for hit in hits {
                    let Ok(activity_id) = Uuid::parse_str(&hit.activity_id) else {
                        continue;
                    };
                    let replace = closest_by_activity.get(&activity_id).is_none_or(
                        |current: &sync::SemanticSearchHit| {
                            hit.cosine_distance < current.cosine_distance
                        },
                    );
                    if replace {
                        closest_by_activity.insert(activity_id, hit);
                    }
                }
                let mut hits: Vec<_> = closest_by_activity.into_iter().collect();
                hits.sort_by(|(_, left), (_, right)| {
                    left.cosine_distance.total_cmp(&right.cosine_distance)
                });
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let results: Vec<_> = hits
                    .into_iter()
                    .filter_map(|(activity_id, hit)| {
                        store
                            .resolve_task(&activity_id.to_string())
                            .ok()
                            .map(|task| {
                                json!({
                                    "task": task,
                                    "cosine_distance": hit.cosine_distance,
                                    "source_type": hit.source_type,
                                })
                            })
                    })
                    .collect();
                Ok(json!({ "results": results }))
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
                let sync_outcome = match params.get("sync_outcome") {
                    None | Some(Value::Null) => None,
                    Some(value) => value
                        .as_str()
                        .and_then(SyncOutcome::parse)
                        .ok_or_else(|| {
                            RpcErrorObject::new(
                                RpcCode::InvalidParams,
                                "sync_outcome must be completed, in_progress, or unclear",
                            )
                        })
                        .map(Some)?,
                };
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let task = store.resolve_task(&id).map_err(map_store_err)?;
                let task = store
                    .update_task(
                        task.id,
                        TaskUpdate {
                            title,
                            status,
                            notes,
                            sync_outcome,
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
            "tasks.delete" => {
                let id = param_str(&params, "id")?;
                let store = self.store.lock().map_err(|_| internal("store lock"))?;
                let task = store.resolve_task(&id).map_err(map_store_err)?;
                store.delete_task(task.id).map_err(map_store_err)?;
                Ok(json!({ "ok": true }))
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
