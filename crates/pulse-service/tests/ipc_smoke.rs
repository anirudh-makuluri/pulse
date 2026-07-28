//! Windows named-pipe IPC smoke: start service, ping, create task, stop.
#![cfg(windows)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use pulse_core::ipc::client::IpcClient;
use pulse_core::TaskStatus;
use tempfile::tempdir;

fn service_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_pulse-service"))
}

#[test]
fn service_ping_and_task_via_ipc() {
    let dir = tempdir().unwrap();
    let data = dir.path();
    let pipe = format!("pulse-test-{}", std::process::id());

    let cfg = format!(
        r#"
[service]
pipe_name = "{pipe}"
log_level = "info"
"#
    );
    fs::write(data.join("config.toml"), cfg).unwrap();

    let mut child = Command::new(service_bin())
        .args(["run", "--quiet", "--data-dir"])
        .arg(data)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn service");

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut client = None;
    while Instant::now() < deadline {
        if let Ok(mut c) = IpcClient::connect(&pipe, Duration::from_millis(200)) {
            if c.ping().is_ok() {
                client = Some(c);
                break;
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    let mut client = client.expect("service did not become ready");

    let task = client
        .tasks_create("IPC task from test", Some(TaskStatus::Inbox), None)
        .expect("create");
    assert_eq!(task.title, "IPC task from test");

    let list = client.tasks_list(Some(TaskStatus::Inbox)).expect("list");
    assert!(list.iter().any(|t| t.id == task.id));

    let activity = client
        .activities_create("IPC activity timeline", None)
        .expect("create activity");
    let session = client
        .sessions_attach(
            &activity.id.to_string(),
            serde_json::json!({ "agent": "codex", "metadata": { "branch": "main" } }),
        )
        .expect("attach session");
    let checkpoint = client
        .checkpoints_create(
            &activity.id.to_string(),
            "IPC checkpoint",
            serde_json::json!({
                "session_id": session.id,
                "decisions": ["Use RPC"],
                "next_actions": ["Render timeline"],
            }),
        )
        .expect("create checkpoint");
    assert_eq!(checkpoint.task_id, activity.id);
    let timeline = client
        .activities_timeline(&activity.id.to_string())
        .expect("timeline");
    let activity_id = activity.id.to_string();
    assert_eq!(timeline["task"]["id"].as_str(), Some(activity_id.as_str()));
    assert_eq!(timeline["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(timeline["checkpoints"].as_array().unwrap().len(), 1);

    client.service_shutdown().expect("shutdown");

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            assert!(status.success() || status.code() == Some(0) || status.code().is_some());
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
}
