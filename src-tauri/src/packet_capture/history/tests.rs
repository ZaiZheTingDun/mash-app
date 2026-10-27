use super::*;

fn capture(id: &str, time: u64, raw: &str) -> ResponseState {
    let response = serde_json::from_value(serde_json::json!({
        "captureId": id, "capturedAt": time, "recordId": 42,
        "url": "https://example.test/?_key=toplogin", "statusCode": 200,
        "byteLength": raw.len(), "format": "原始响应", "body": raw, "items": [],
    }))
    .unwrap();
    ResponseState {
        response: Some(response),
        serial: Some("device".into()),
        ..Default::default()
    }
}

#[test]
fn persists_raw_responses_orders_selects_and_removes_without_resurrecting_deleted_records() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("history");
    let store = HistoryStore::new(root.clone());
    assert!(store
        .sync(&ResponseState::default())
        .unwrap()
        .records
        .is_empty());
    let first_id = uuid::Uuid::new_v4().to_string();
    let second_id = uuid::Uuid::new_v4().to_string();
    let first = capture(&first_id, 1, "encoded%3D\r\n");
    let second = capture(&second_id, 2, "another-raw-response");
    store.sync(&first).unwrap();
    store.sync(&first).unwrap();
    let view = store.sync(&second).unwrap();
    assert_eq!(view.records.len(), 2);
    assert_eq!(view.records[0].id, second_id);
    store.select(&first_id).unwrap();
    assert_eq!(store.selected_body().unwrap(), "encoded%3D\r\n");
    let restarted = HistoryStore::new(root.clone());
    assert_eq!(
        restarted
            .sync(&ResponseState::default())
            .unwrap()
            .selected
            .unwrap()
            .entry
            .id,
        second_id
    );
    let deleted = store.delete(Some(&first_id)).unwrap();
    assert_eq!(deleted.selected.unwrap().entry.id, second_id);
    assert!(!store.path(&first_id).unwrap().exists());
    assert_eq!(store.sync(&first).unwrap().records.len(), 1);
    store.delete(None).unwrap();
    assert!(store.sync(&second).unwrap().records.is_empty());
    assert!(store.selected_body().is_err());
    assert_eq!(fs::read_dir(root).unwrap().count(), 0);
    assert!(store.select(&first_id).is_err());
    assert!(store.delete(Some("../outside")).is_err());
}

#[test]
fn corrupted_and_unwritable_history_is_reported_without_overwriting_data() {
    let temp = tempfile::tempdir().unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let file = temp.path().join(format!("{id}.json"));
    fs::write(&file, "broken").unwrap();
    let store = HistoryStore::new(temp.path().into());
    assert!(store.sync(&capture(&id, 1, "secret-response")).is_err());
    assert_eq!(fs::read_to_string(&file).unwrap(), "broken");
    let store = HistoryStore::new(file);
    assert!(store.sync(&capture(&id, 1, "secret-response")).is_err());
}
