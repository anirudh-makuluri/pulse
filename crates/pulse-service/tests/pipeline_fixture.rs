//! Fixture-driven heuristic inference into Inbox.

use std::io::Write;

use pulse_core::{open_in_memory, Config, Store, TaskSource, TaskStatus};
use pulse_service::pipeline;
use pulse_sources::{ClaudeSource, SourceAdapter};
use tempfile::tempdir;

#[test]
fn claude_fixture_creates_inbox_task_with_evidence() {
    let dir = tempdir().unwrap();
    let proj = dir.path().join("projects").join("demo-app");
    std::fs::create_dir_all(&proj).unwrap();
    let sess = proj.join("session-1.jsonl");
    let mut f = std::fs::File::create(&sess).unwrap();
    writeln!(
        f,
        r#"{{"type":"user","message":{{"role":"user","content":"Please implement dark mode toggle for settings page"}}}}"#
    )
    .unwrap();
    writeln!(
        f,
        r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"Sure, I will add a dark mode toggle."}}]}}}}"#
    )
    .unwrap();

    let mut store = Store::new(open_in_memory().unwrap());
    let cfg = Config::default();
    let adapter: Box<dyn SourceAdapter> =
        Box::new(ClaudeSource::with_root(dir.path(), 65_536));

    let created = pipeline::run_once_with_adapters(&mut store, &cfg, vec![adapter]).unwrap();
    assert!(created >= 1, "expected at least one inferred task");

    let inbox = store.list_tasks(Some(TaskStatus::Inbox)).unwrap();
    assert!(!inbox.is_empty());
    let task = &inbox[0];
    assert_eq!(task.source, TaskSource::Claude);
    assert_eq!(task.status, TaskStatus::Inbox);
    assert!(task.confidence.unwrap_or(1.0) <= 0.45);
    assert_eq!(task.project.as_deref(), Some("demo-app"));
    assert!(task.dedup_key.is_some());

    let ev = store.list_evidence(task.id).unwrap();
    assert!(!ev.is_empty());
    assert_eq!(ev[0].kind, "session_snippet");

    // Second pass should not duplicate (watermark + dedup)
    let adapter2: Box<dyn SourceAdapter> =
        Box::new(ClaudeSource::with_root(dir.path(), 65_536));
    let created2 = pipeline::run_once_with_adapters(&mut store, &cfg, vec![adapter2]).unwrap();
    assert_eq!(created2, 0);
    assert_eq!(store.list_tasks(Some(TaskStatus::Inbox)).unwrap().len(), inbox.len());
}
