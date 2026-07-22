//! Pulse background service: named-pipe JSON-RPC server.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::Utc;
use clap::{Parser, Subcommand};
use pulse_core::ipc::pipe::{self, current_pid};
use pulse_core::ipc::pid::{remove_pid_file_if_matches, write_pid_file, ServicePidFile};
use pulse_core::ipc::rpc::{RpcCode, RpcErrorObject, RpcHandler};
use pulse_core::{
    load_config, open_db, Config, NewTask, PulseError, PulsePaths, Store, TaskStatus, TaskUpdate,
};
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
    /// Run the service in the foreground
    Run {
        /// Reduce console noise
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
    let store = Store::new(conn);

    let state = Arc::new(ServiceState {
        paths: paths.clone(),
        store: Mutex::new(store),
        config: Mutex::new(config),
        shutdown: Arc::new(AtomicBool::new(false)),
        started_at: Utc::now(),
        pid: current_pid(),
    });

    // Fail fast before writing PID.
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

    if !quiet {
        eprintln!(
            "pulse-service listening on pipe '{}' (pid {})",
            pipe_name, state.pid
        );
        eprintln!("stop with: pulse service stop");
    }

    let shutdown = Arc::clone(&state.shutdown);
    let handler = Arc::clone(&state);
    let result = pipe::serve_loop(&pipe_name, handler, shutdown);

    let _ = remove_pid_file_if_matches(&paths.service_pid_path(), state.pid, Some(&exe_path));
    result?;
    Ok(())
}

struct ServiceState {
    paths: PulsePaths,
    store: Mutex<Store>,
    config: Mutex<Config>,
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
                Ok(json!({
                    "ok": true,
                    "version": env!("CARGO_PKG_VERSION"),
                    "pid": self.pid,
                    "pipe_name": cfg.service.pipe_name,
                    "started_at": self.started_at.to_rfc3339(),
                    "data_dir": self.paths.root,
                    "llm_mode": cfg.llm.provider,
                    "queue_depth": 0,
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
                let cfg = load_config(&self.paths.config_path()).map_err(|e| {
                    RpcErrorObject::new(RpcCode::ConfigError, e.to_string())
                })?;
                let mut guard = self.config.lock().map_err(|_| internal("config lock"))?;
                *guard = cfg;
                Ok(json!({ "ok": true }))
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

fn parse_status_value(v: &Value) -> Result<TaskStatus, RpcErrorObject> {
    let s = v
        .as_str()
        .ok_or_else(|| RpcErrorObject::new(RpcCode::InvalidParams, "status must be a string"))?;
    TaskStatus::parse(s).ok_or_else(|| {
        RpcErrorObject::new(RpcCode::InvalidParams, format!("invalid status '{s}'"))
    })
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
