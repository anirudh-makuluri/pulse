use pulse_core::{
    export_history, load_config, open_db, try_connect, write_config, ActivityEvent, Artifact,
    Checkpoint, Evidence, ExportFormat, Memory, NewActivityEvent, NewReminder, NewTask,
    OmniboxIntent, ParsedOmniboxIntent, PulsePaths, Reminder, ReminderStatus, Session, Store,
    Task, TaskStatus, TaskUpdate, parse_omnibox,
};
use pulse_llm::llm_status;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_shell::{process::CommandChild, ShellExt};

#[derive(Default)]
struct ManagedService(Mutex<Option<CommandChild>>);

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

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ContextEnvelope {
    active_app: Option<String>,
    window_title: Option<String>,
    selected_text: Option<String>,
    captured_at: String,
}

#[derive(Debug, Serialize)]
struct OmniboxPreview {
    parsed: ParsedOmniboxIntent,
    context: ContextEnvelope,
    needs_context_confirmation: bool,
}

#[derive(Debug, Serialize)]
struct OmniboxResult {
    message: String,
    task: Option<Task>,
    reminder: Option<Reminder>,
    tasks: Vec<Task>,
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

fn start_bundled_service(app: &tauri::App) -> Result<(), String> {
    let paths = paths()?;
    let cfg = load_config(&paths.config_path()).map_err(|e| e.to_string())?;
    // Respect a daemon launched by the CLI or another Pulse window.
    if try_connect(&cfg.service.pipe_name).is_ok() {
        return Ok(());
    }
    let command = app
        .shell()
        .sidecar("pulse-service")
        .map_err(|e| format!("could not locate bundled pulse-service: {e}"))?
        .args([
            "run",
            "--quiet",
            "--data-dir",
            &paths.root.display().to_string(),
        ]);
    let (mut events, child) = command
        .spawn()
        .map_err(|e| format!("could not start pulse-service: {e}"))?;
    // Drain service output so a long-running daemon can never block on a full pipe.
    tauri::async_runtime::spawn(async move { while events.recv().await.is_some() {} });
    *app.state::<ManagedService>().0.lock().map_err(|_| "service state lock failed")? = Some(child);
    Ok(())
}

fn stop_bundled_service(app: &tauri::AppHandle) {
    let state = app.state::<ManagedService>();
    let Ok(mut guard) = state.0.lock() else { return };
    if let Some(child) = guard.take() {
        let _ = child.kill();
    }
}

fn parse_status(s: &str) -> Result<TaskStatus, String> {
    TaskStatus::parse(s).ok_or_else(|| format!("invalid status: {s}"))
}

fn capture_context_envelope(include_selected_text: bool) -> ContextEnvelope {
    // Clipboard access is only performed after an explicit UI action. In the MVP it
    // represents selected text copied by the user; it is never persisted until the
    // preview is confirmed.
    let selected_text = include_selected_text.then(capture_clipboard_text).flatten();
    let (active_app, window_title) = active_window_metadata();
    ContextEnvelope {
        active_app,
        window_title,
        selected_text,
        captured_at: chrono::Utc::now().to_rfc3339(),
    }
}

#[cfg(windows)]
fn active_window_metadata() -> (Option<String>, Option<String>) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() { return (None, None); }
        let mut title = [0u16; 512];
        let len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        ((process_id != 0).then(|| format!("process-{process_id}")), (len > 0).then(|| String::from_utf16_lossy(&title[..len as usize])))
    }
}

#[cfg(not(windows))]
fn active_window_metadata() -> (Option<String>, Option<String>) { (None, None) }

#[cfg(windows)]
fn capture_clipboard_text() -> Option<String> {
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 { return None; }
        let handle = GetClipboardData(13); // CF_UNICODETEXT
        if handle.is_null() { CloseClipboard(); return None; }
        let ptr = GlobalLock(handle) as *const u16;
        if ptr.is_null() { CloseClipboard(); return None; }
        let mut len = 0usize;
        while len < 32_768 && *ptr.add(len) != 0 { len += 1; }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)).trim().to_string();
        GlobalUnlock(handle);
        CloseClipboard();
        (!text.is_empty()).then_some(text)
    }
}

#[cfg(not(windows))]
fn capture_clipboard_text() -> Option<String> { None }

fn find_task(store: &Store, selected_id: Option<&str>, subject: &str) -> Result<Task, String> {
    if let Some(id) = selected_id.filter(|id| !id.is_empty()) { return store.resolve_task(id).map_err(|e| e.to_string()); }
    let needle = subject.trim().to_ascii_lowercase();
    let matches: Vec<_> = store.list_tasks(None).map_err(|e| e.to_string())?.into_iter()
        .filter(|task| task.status.is_open() && (needle.is_empty() || task.title.to_ascii_lowercase().contains(&needle)))
        .collect();
    match matches.len() { 1 => Ok(matches.into_iter().next().unwrap()), 0 => Err("Choose a task or include a task title.".into()), _ => Err("More than one task matches; select one first.".into()) }
}

#[tauri::command]
fn preview_omnibox(input: String, include_selected_text: bool) -> OmniboxPreview {
    let context = capture_context_envelope(include_selected_text);
    OmniboxPreview {
        parsed: parse_omnibox(&input, chrono::Local::now()),
        needs_context_confirmation: context.selected_text.is_some(),
        context,
    }
}

#[tauri::command]
fn execute_omnibox(input: String, selected_task_id: Option<String>, context: ContextEnvelope) -> Result<OmniboxResult, String> {
    let parsed = parse_omnibox(&input, chrono::Local::now());
    let store = open_store()?;
    let context_json = serde_json::to_string(&context).map_err(|e| e.to_string())?;
    let none = || OmniboxResult { message: String::new(), task: None, reminder: None, tasks: vec![] };
    match parsed.intent {
        OmniboxIntent::CreateTask => {
            let title = if parsed.subject.is_empty() { return Err("Add a task title.".into()) } else { parsed.subject };
            let task = store.create_task(NewTask::manual(title)).map_err(|e| e.to_string())?;
            Ok(OmniboxResult { message: "Task created.".into(), task: Some(task), reminder: None, tasks: vec![] })
        }
        OmniboxIntent::SearchActivity => {
            let needle = parsed.subject.to_ascii_lowercase();
            let tasks = store.list_tasks(None).map_err(|e| e.to_string())?.into_iter().filter(|t| t.title.to_ascii_lowercase().contains(&needle) || t.notes.as_deref().unwrap_or("").to_ascii_lowercase().contains(&needle)).collect();
            Ok(OmniboxResult { message: "Search results.".into(), task: None, reminder: None, tasks })
        }
        OmniboxIntent::CompleteTask => {
            let task = find_task(&store, selected_task_id.as_deref(), &parsed.subject)?;
            let task = store.mark_done(task.id).map_err(|e| e.to_string())?;
            Ok(OmniboxResult { message: "Task marked done.".into(), task: Some(task), reminder: None, tasks: vec![] })
        }
        OmniboxIntent::DeleteTask => {
            let task = find_task(&store, selected_task_id.as_deref(), &parsed.subject)?;
            store.delete_task(task.id).map_err(|e| e.to_string())?;
            Ok(OmniboxResult { message: "Task deleted.".into(), ..none() })
        }
        OmniboxIntent::CreateReminder => {
            if parsed.subject.is_empty() { return Err("Say what to be reminded about.".into()); }
            let due_at = parsed.due_at.ok_or_else(|| "Use a supported time such as 'in 30 minutes' or 'tomorrow morning'.".to_string())?;
            let task = match find_task(&store, selected_task_id.as_deref(), "") { Ok(task) => task, Err(_) => store.create_task(NewTask::manual(parsed.subject.clone())).map_err(|e| e.to_string())? };
            let reminder = store.create_reminder(NewReminder { task_id: task.id, title: parsed.subject, due_at, context_json }).map_err(|e| e.to_string())?;
            Ok(OmniboxResult { message: "Reminder scheduled locally.".into(), task: Some(task), reminder: Some(reminder), tasks: vec![] })
        }
        OmniboxIntent::SnoozeReminder => Err("Use Snooze on a due reminder so Pulse knows which reminder to move.".into()),
        OmniboxIntent::ResumeTask | OmniboxIntent::OpenContext => {
            let task = find_task(&store, selected_task_id.as_deref(), &parsed.subject)?;
            Ok(OmniboxResult { message: "Context opened.".into(), task: Some(task), reminder: None, tasks: vec![] })
        }
        OmniboxIntent::TransferTask => {
            let task = find_task(&store, selected_task_id.as_deref(), &parsed.subject)?;
            store.record_event(NewActivityEvent { task_id: task.id, session_id: None, kind: "handoff_requested".into(), summary: "Continue this activity in Codex".into(), payload_json: Some(context_json), source_ref: Some("omnibox".into()), occurred_at: chrono::Utc::now() }).map_err(|e| e.to_string())?;
            Ok(OmniboxResult { message: "Handoff recorded. Select the activity to continue in Codex.".into(), task: Some(task), reminder: None, tasks: vec![] })
        }
        OmniboxIntent::Unknown => Err("I couldn't determine that action.".into()),
    }
}

#[tauri::command]
fn due_reminders() -> Result<Vec<Reminder>, String> { open_store()?.list_due_reminders(chrono::Utc::now()).map_err(|e| e.to_string()) }

#[tauri::command]
fn reminder_action(id: String, action: String) -> Result<Reminder, String> {
    let reminder_id = uuid::Uuid::parse_str(&id).map_err(|_| "invalid reminder id".to_string())?;
    let store = open_store()?;
    match action.as_str() {
        "done" => store.set_reminder_status(reminder_id, ReminderStatus::Done).map_err(|e| e.to_string()),
        "snooze" => store.snooze_reminder(reminder_id, chrono::Utc::now() + chrono::Duration::minutes(30)).map_err(|e| e.to_string()),
        "open_context" | "continue_coding" => store.get_reminder(reminder_id).map_err(|e| e.to_string())?.ok_or_else(|| "reminder not found".into()),
        _ => Err("unknown reminder action".into()),
    }
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
        .manage(ManagedService::default())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            if let Err(error) = start_bundled_service(app) {
                // The app retains direct-DB fallback; surface startup trouble in
                // the console without preventing users from opening their data.
                eprintln!("Pulse service did not auto-start: {error}");
            }
            Ok(())
        })
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
            ,preview_omnibox,
            execute_omnibox,
            due_reminders,
            reminder_action
        ])
        .build(tauri::generate_context!())
        .expect("error while building Pulse")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                stop_bundled_service(app);
            }
        });
}
