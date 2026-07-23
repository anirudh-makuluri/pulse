use pulse_core::{
    load_config, open_db, try_connect, Evidence, NewTask, PulsePaths, Store, Task, TaskStatus,
    TaskUpdate,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct TaskDetail {
    task: Task,
    evidence: Vec<Evidence>,
}

fn open_store() -> Result<Store, String> {
    let paths = PulsePaths::default().map_err(|e| e.to_string())?;
    paths.ensure_layout().map_err(|e| e.to_string())?;
    let conn = open_db(&paths.db_path()).map_err(|e| e.to_string())?;
    Ok(Store::new(conn))
}

fn parse_status(s: &str) -> Result<TaskStatus, String> {
    TaskStatus::parse(s).ok_or_else(|| format!("invalid status: {s}"))
}

/// Prefer live service via IPC; fall back to direct SQLite (same as CLI).
fn with_backend_list(status: Option<TaskStatus>) -> Result<Vec<Task>, String> {
    let paths = PulsePaths::default().map_err(|e| e.to_string())?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        return c.tasks_list(status).map_err(|e| e.to_string());
    }
    let store = open_store()?;
    store.list_tasks(status).map_err(|e| e.to_string())
}

fn with_backend_get(id: &str) -> Result<TaskDetail, String> {
    let paths = PulsePaths::default().map_err(|e| e.to_string())?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        let (task, evidence) = c.tasks_get(id).map_err(|e| e.to_string())?;
        return Ok(TaskDetail { task, evidence });
    }
    let store = open_store()?;
    let task = store.resolve_task(id).map_err(|e| e.to_string())?;
    let evidence = store.list_evidence(task.id).map_err(|e| e.to_string())?;
    Ok(TaskDetail { task, evidence })
}

#[tauri::command]
fn list_tasks(status: Option<String>) -> Result<Vec<Task>, String> {
    let st = match status.as_deref() {
        None | Some("") => None,
        Some(s) => Some(parse_status(s)?),
    };
    with_backend_list(st)
}

#[tauri::command]
fn get_task(id: String) -> Result<TaskDetail, String> {
    with_backend_get(&id)
}

#[tauri::command]
fn create_task(title: String, today: bool) -> Result<Task, String> {
    let paths = PulsePaths::default().map_err(|e| e.to_string())?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    let status = if today {
        Some(TaskStatus::Today)
    } else {
        None
    };
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        return c
            .tasks_create(&title, status, None)
            .map_err(|e| e.to_string());
    }
    let store = open_store()?;
    let mut new = NewTask::manual(title);
    if today {
        new.status = TaskStatus::Today;
    }
    store.create_task(new).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_task_status(id: String, status: String) -> Result<Task, String> {
    let st = parse_status(&status)?;
    let paths = PulsePaths::default().map_err(|e| e.to_string())?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        return c
            .tasks_update(&id, None, Some(st), None)
            .map_err(|e| e.to_string());
    }
    let store = open_store()?;
    let task = store.resolve_task(&id).map_err(|e| e.to_string())?;
    store
        .update_task(
            task.id,
            TaskUpdate {
                status: Some(st),
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn mark_done(id: String) -> Result<Task, String> {
    let paths = PulsePaths::default().map_err(|e| e.to_string())?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        return c.tasks_done(&id).map_err(|e| e.to_string());
    }
    let store = open_store()?;
    let task = store.resolve_task(&id).map_err(|e| e.to_string())?;
    store.mark_done(task.id).map_err(|e| e.to_string())
}

#[tauri::command]
fn service_info() -> Result<String, String> {
    let paths = PulsePaths::default().map_err(|e| e.to_string())?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        let v = c.service_status().map_err(|e| e.to_string())?;
        let mode = v
            .get("llm_mode")
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        let pid = v.get("pid").and_then(|x| x.as_u64()).unwrap_or(0);
        return Ok(format!("service pid {pid} · llm {mode}"));
    }
    Ok("direct DB (service off)".into())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            list_tasks,
            get_task,
            create_task,
            set_task_status,
            mark_done,
            service_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running Pulse");
}
