use std::process::Command;

use tempfile::tempdir;

fn pulse() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pulse"))
}

#[test]
fn version_prints() {
    let out = pulse().arg("version").output().expect("run pulse version");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("pulse"), "{stdout}");
}

#[test]
fn add_list_done_roundtrip() {
    let dir = tempdir().unwrap();
    let data = dir.path();

    let add = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args(["tasks", "add", "Write PR2 CLI"])
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let add_out = String::from_utf8_lossy(&add.stdout);
    let id_prefix = add_out.split_whitespace().next().expect("id in add output");

    let list = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args(["tasks", "list", "--status", "inbox"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let list_out = String::from_utf8_lossy(&list.stdout);
    assert!(list_out.contains("Write PR2 CLI"), "{list_out}");

    let done = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args(["tasks", "done", id_prefix])
        .output()
        .unwrap();
    assert!(
        done.status.success(),
        "done failed: {}",
        String::from_utf8_lossy(&done.stderr)
    );

    let done_list = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args(["tasks", "list", "--status", "done"])
        .output()
        .unwrap();
    assert!(done_list.status.success());
    let done_out = String::from_utf8_lossy(&done_list.stdout);
    assert!(done_out.contains("Write PR2 CLI"), "{done_out}");
}

#[test]
fn json_list_and_move() {
    let dir = tempdir().unwrap();
    let data = dir.path();

    let add = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args(["tasks", "add", "Focus item", "--today"])
        .output()
        .unwrap();
    assert!(add.status.success());
    let id = String::from_utf8_lossy(&add.stdout)
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();

    let list = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args(["tasks", "list", "--json"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let v: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert!(v.as_array().unwrap().len() >= 1);

    let moved = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args(["tasks", "move", &id, "next"])
        .output()
        .unwrap();
    assert!(
        moved.status.success(),
        "{}",
        String::from_utf8_lossy(&moved.stderr)
    );
}

#[test]
fn config_path_and_show() {
    let dir = tempdir().unwrap();
    let data = dir.path();

    let path = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args(["config", "path"])
        .output()
        .unwrap();
    assert!(path.status.success());
    let p = String::from_utf8_lossy(&path.stdout);
    assert!(p.contains("config.toml"), "{p}");

    // force create config via show
    let show = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args(["config", "show"])
        .output()
        .unwrap();
    assert!(
        show.status.success(),
        "{}",
        String::from_utf8_lossy(&show.stderr)
    );
    let toml = String::from_utf8_lossy(&show.stdout);
    assert!(toml.contains("provider"), "{toml}");
    assert!(toml.contains("grok"), "{toml}");
}

#[test]
fn empty_title_fails() {
    let dir = tempdir().unwrap();
    let data = dir.path();
    let add = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args(["tasks", "add", "   "])
        .output()
        .unwrap();
    assert!(!add.status.success());
}

#[test]
fn activity_timeline_roundtrip() {
    let dir = tempdir().unwrap();
    let data = dir.path();

    let create = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args(["activities", "create", "Implement local timeline"])
        .output()
        .unwrap();
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );
    let activity_id = String::from_utf8_lossy(&create.stdout)
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();

    let attach = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args([
            "activities",
            "attach-session",
            &activity_id,
            "--agent",
            "codex",
            "--metadata",
            r#"{"branch":"main"}"#,
        ])
        .output()
        .unwrap();
    assert!(
        attach.status.success(),
        "{}",
        String::from_utf8_lossy(&attach.stderr)
    );
    let session_id = String::from_utf8_lossy(&attach.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned();

    let checkpoint = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args([
            "activities",
            "checkpoint",
            &activity_id,
            "Core store is ready",
            "--session-id",
            &session_id,
            "--decision",
            "Keep tasks as roots",
            "--next-action",
            "Add the Tauri timeline",
        ])
        .output()
        .unwrap();
    assert!(
        checkpoint.status.success(),
        "{}",
        String::from_utf8_lossy(&checkpoint.stderr)
    );

    let timeline = pulse()
        .args(["--data-dir"])
        .arg(data)
        .args(["activities", "timeline", &activity_id, "--json"])
        .output()
        .unwrap();
    assert!(
        timeline.status.success(),
        "{}",
        String::from_utf8_lossy(&timeline.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&timeline.stdout).unwrap();
    assert_eq!(value["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(value["checkpoints"].as_array().unwrap().len(), 1);
}
