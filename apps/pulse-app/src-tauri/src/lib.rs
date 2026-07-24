use pulse_core::{
    export_history, load_config, open_db, try_connect, write_config, ActivityEvent, Artifact,
    Checkpoint, Evidence, ExportFormat, Memory, NewTask, PulsePaths, Reminder, Session, Store,
    Task, TaskStatus, TaskUpdate,
};
use pulse_llm::llm_status;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct TaskDetail {
    task: Task,
    evidence: Vec<Evidence>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ActivityTimeline {
    task: Task,
    evidence: Vec<Evidence>,
    sessions: Vec<Session>,
    events: Vec<ActivityEvent>,
    checkpoints: Vec<Checkpoint>,
    reminders: Vec<Reminder>,
    memories: Vec<Memory>,
    artifacts: Vec<Artifact>,
}

#[derive(Debug, Serialize)]
struct SettingsSnapshot {
    claude_enabled: bool,
    codex_enabled: bool,
    privacy_ack: bool,
    llm_backend: String,
    llm_path: Option<String>,
    llm_reason: String,
    service_line: String,
    config_path: String,
    data_dir: String,
}

fn open_store() -> Result<Store, String> {
    let paths = PulsePaths::default().map_err(|e| e.to_string())?;
    paths.ensure_layout().map_err(|e| e.to_string())?;
    let conn = open_db(&paths.db_path()).map_err(|e| e.to_string())?;
    Ok(Store::new(conn))
}

fn paths() -> Result<PulsePaths, String> {
    let p = PulsePaths::default().map_err(|e| e.to_string())?;
    p.ensure_layout().map_err(|e| e.to_string())?;
    Ok(p)
}

fn parse_status(s: &str) -> Result<TaskStatus, String> {
    TaskStatus::parse(s).ok_or_else(|| format!("invalid status: {s}"))
}

fn with_backend_list(status: Option<TaskStatus>) -> Result<Vec<Task>, String> {
    let paths = paths()?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        return c.tasks_list(status).map_err(|e| e.to_string());
    }
    let store = open_store()?;
    store.list_tasks(status).map_err(|e| e.to_string())
}

fn with_backend_get(id: &str) -> Result<TaskDetail, String> {
    let paths = paths()?;
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

fn with_backend_timeline(id: &str) -> Result<ActivityTimeline, String> {
    let paths = paths()?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        let timeline = c.activities_timeline(id).map_err(|e| e.to_string())?;
        return serde_json::from_value(timeline).map_err(|e| e.to_string());
    }
    let store = open_store()?;
    let task = store.resolve_task(id).map_err(|e| e.to_string())?;
    Ok(ActivityTimeline {
        evidence: store.list_evidence(task.id).map_err(|e| e.to_string())?,
        sessions: store.list_sessions(task.id).map_err(|e| e.to_string())?,
        events: store.list_events(task.id).map_err(|e| e.to_string())?,
        checkpoints: store.list_checkpoints(task.id).map_err(|e| e.to_string())?,
        reminders: store.list_reminders(task.id).map_err(|e| e.to_string())?,
        memories: store.list_memories(task.id).map_err(|e| e.to_string())?,
        artifacts: store.list_artifacts(task.id).map_err(|e| e.to_string())?,
        task,
    })
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
fn get_activity_timeline(id: String) -> Result<ActivityTimeline, String> {
    with_backend_timeline(&id)
}

#[tauri::command]
fn create_task(title: String, today: bool) -> Result<Task, String> {
    let paths = paths()?;
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
    let paths = paths()?;
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
    let paths = paths()?;
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
    let paths = paths()?;
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

#[tauri::command]
fn get_settings() -> Result<SettingsSnapshot, String> {
    let paths = paths()?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    let st = llm_status(&cfg.llm, &cfg.privacy);
    let service_line = service_info().unwrap_or_else(|_| "backend unknown".into());
    Ok(SettingsSnapshot {
        claude_enabled: cfg.sources.claude.enabled,
        codex_enabled: cfg.sources.codex.enabled,
        privacy_ack: cfg.privacy.acknowledge_remote_llm,
        llm_backend: st.backend_id,
        llm_path: st.path,
        llm_reason: st.reason,
        service_line,
        config_path: paths.config_path().display().to_string(),
        data_dir: paths.root.display().to_string(),
    })
}

#[tauri::command]
fn set_source_enabled(id: String, enabled: bool) -> Result<(), String> {
    let paths = paths()?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        c.sources_set_enabled(&id, enabled)
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let mut cfg = cfg;
    match id.as_str() {
        "claude" => cfg.sources.claude.enabled = enabled,
        "codex" => cfg.sources.codex.enabled = enabled,
        other => return Err(format!("unknown source: {other}")),
    }
    write_config(&paths.config_path(), &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
fn privacy_acknowledge() -> Result<(), String> {
    let paths = paths()?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        c.call_raw("privacy.acknowledge", serde_json::json!({}))
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let mut cfg = cfg;
    cfg.privacy.acknowledge_remote_llm = true;
    write_config(&paths.config_path(), &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_summary(date: Option<String>) -> Result<String, String> {
    let day = date.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    let paths = paths()?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        let v = c
            .call_raw("summary.get", serde_json::json!({ "date": day }))
            .map_err(|e| e.to_string())?;
        if let Some(s) = v.pointer("/summary/text").and_then(|x| x.as_str()) {
            return Ok(s.to_string());
        }
        return Ok(String::new());
    }
    let store = open_store()?;
    Ok(store
        .get_summary(&day)
        .map_err(|e| e.to_string())?
        .map(|s| s.text)
        .unwrap_or_default())
}

#[tauri::command]
fn generate_summary(date: Option<String>) -> Result<String, String> {
    let day = date.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    let paths = paths()?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        let v = c
            .call_raw("summary.generate", serde_json::json!({ "date": day }))
            .map_err(|e| e.to_string())?;
        if let Some(t) = v.get("text").and_then(|x| x.as_str()) {
            return Ok(t.to_string());
        }
        return Ok(serde_json::to_string_pretty(&v).unwrap_or_default());
    }
    // Offline: heuristic/CLI via pulse-llm
    use pulse_llm::{resolve_llm_client, SummaryRequest};
    let store = open_store()?;
    let tasks = store.list_tasks(None).map_err(|e| e.to_string())?;
    let lines: Vec<String> = tasks
        .iter()
        .map(|t| format!("[{}] {}", t.status, t.title))
        .collect();
    let client = resolve_llm_client(&cfg.llm, &cfg.privacy);
    let out = client
        .summarize_day(&SummaryRequest {
            day: day.clone(),
            task_lines: lines,
            activity_notes: None,
        })
        .map_err(|e| e.to_string())?;
    let offset = chrono::Local::now().offset().local_minus_utc() / 60;
    let highlights = serde_json::to_string(&out.highlights).unwrap_or_else(|_| "[]".into());
    store
        .upsert_summary(&day, offset, &out.text, &highlights, "[]")
        .map_err(|e| e.to_string())?;
    Ok(out.text)
}

#[tauri::command]
fn export_history_cmd(format: String) -> Result<String, String> {
    let fmt = ExportFormat::parse(&format).ok_or_else(|| "format must be json or md".to_string())?;
    let paths = paths()?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        let v = c
            .call_raw(
                "export.history",
                serde_json::json!({ "format": format }),
            )
            .map_err(|e| e.to_string())?;
        return Ok(v
            .get("path")
            .and_then(|p| p.as_str())
            .unwrap_or_default()
            .to_string());
    }
    let store = open_store()?;
    let path = export_history(&store, &paths, fmt, None, None, None).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            list_tasks,
            get_task,
            get_activity_timeline,
            create_task,
            set_task_status,
            mark_done,
            service_info,
            get_settings,
            set_source_enabled,
            privacy_acknowledge,
            get_summary,
            generate_summary,
            export_history_cmd
        ])
        .run(tauri::generate_context!())
        .expect("error while running Pulse");
}
