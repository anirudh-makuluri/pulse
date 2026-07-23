use pulse_core::{
    apply_checkin_answer, open_in_memory, NewCheckIn, NewTask, Store, TaskStatus, CheckInKind,
};
use serde_json::json;

#[test]
fn summary_roundtrip() {
    let store = Store::new(open_in_memory().unwrap());
    store
        .upsert_summary("2026-07-22", -420, "Did stuff", r#"["a"]"#, "[]")
        .unwrap();
    let s = store.get_summary("2026-07-22").unwrap().unwrap();
    assert!(s.text.contains("stuff"));
    store
        .upsert_summary("2026-07-22", -420, "Updated", r#"[]"#, "[]")
        .unwrap();
    let s2 = store.get_summary("2026-07-22").unwrap().unwrap();
    assert_eq!(s2.text, "Updated");
}

#[test]
fn checkin_answer_done() {
    let store = Store::new(open_in_memory().unwrap());
    let t = store.create_task(NewTask::manual("Finish the migration runner")).unwrap();
    let c = store
        .create_checkin(NewCheckIn {
            task_id: Some(t.id),
            question: "Is it done?".into(),
            kind: CheckInKind::IsDone,
        })
        .unwrap();
    let patch = apply_checkin_answer(CheckInKind::IsDone, &json!({"done": true})).unwrap();
    let t2 = store.update_task(t.id, patch).unwrap();
    assert_eq!(t2.status, TaskStatus::Done);
    store.answer_checkin(c.id, r#"{"done":true}"#).unwrap();
    let open = store.list_checkins(true).unwrap();
    assert!(open.is_empty());
}
