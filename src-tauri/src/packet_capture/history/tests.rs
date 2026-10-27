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
    assert_eq!(store.record(&first_id).unwrap().entry.id, first_id);
    assert_eq!(store.body(&first_id).unwrap(), "encoded%3D\r\n");
    let restarted = HistoryStore::new(root.clone());
    assert_eq!(
        restarted
            .sync(&ResponseState::default())
            .unwrap()
            .default_record_id
            .unwrap(),
        second_id
    );
    let deleted = store.delete(Some(&first_id)).unwrap();
    assert_eq!(deleted.default_record_id.unwrap(), second_id);
    assert!(!store.path(&first_id).unwrap().exists());
    assert_eq!(store.sync(&first).unwrap().records.len(), 1);
    store.delete(None).unwrap();
    assert!(store.sync(&second).unwrap().records.is_empty());
    assert!(store.body(&first_id).is_err());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1); // Empty index remains.
    assert!(HistoryStore::new(root)
        .sync(&ResponseState::default())
        .unwrap()
        .records
        .is_empty());
    assert!(store.record(&first_id).is_err());
    assert!(store.delete(Some("../outside")).is_err());
}

#[test]
fn migrates_legacy_once_then_returns_index_before_reading_any_payload() {
    let temp = tempfile::tempdir().unwrap();
    let old_id = uuid::Uuid::new_v4().to_string();
    let latest_id = uuid::Uuid::new_v4().to_string();
    for (id, time, raw) in [(&old_id, 1, "old%3D"), (&latest_id, 2, "latest%3D")] {
        let capture = capture(id, time, raw);
        let record = CaptureRecord {
            entry: CaptureHistoryEntry {
                id: id.clone(),
                captured_at: time,
                serial: capture.serial,
            },
            response: capture.response.unwrap(),
        };
        write_json_atomic(&temp.path().join(format!("{id}.json")), &record, "test").unwrap();
    }
    let old_file = temp.path().join(format!("{old_id}.json"));
    let old_bytes = fs::read(&old_file).unwrap();
    let store = HistoryStore::new(temp.path().into());
    let view = store.sync(&ResponseState::default()).unwrap();
    assert_eq!(view.records.len(), 2);
    assert_eq!(view.default_record_id.unwrap(), latest_id);
    assert_eq!(fs::read(&old_file).unwrap(), old_bytes);
    let index = fs::read_to_string(temp.path().join("index.json")).unwrap();
    assert!(!index.contains("latest%3D"));
    assert!(!index.contains("response"));
    assert!(!index.contains("items"));

    // Even the default payload is not parsed until explicitly requested.
    fs::write(&old_file, "broken").unwrap();
    let latest_file = temp.path().join(format!("{latest_id}.json"));
    let latest_bytes = fs::read(&latest_file).unwrap();
    fs::write(&latest_file, "broken latest").unwrap();
    let restarted = HistoryStore::new(temp.path().into());
    let view = restarted.sync(&ResponseState::default()).unwrap();
    assert_eq!(view.records.len(), 2);
    assert_eq!(view.default_record_id.unwrap(), latest_id);
    assert!(restarted.record(&latest_id).is_err());
    assert!(restarted.record(&old_id).is_err());
    fs::write(latest_file, latest_bytes).unwrap();
    assert_eq!(restarted.body(&latest_id).unwrap(), "latest%3D");
}

#[test]
fn reuses_selected_payload_for_refresh_selection_and_copy_and_evicts_deleted_entries() {
    let temp = tempfile::tempdir().unwrap();
    let store = HistoryStore::new(temp.path().into());
    let id = uuid::Uuid::new_v4().to_string();
    let capture = capture(&id, 1, "unchanged%3D");
    store.sync(&capture).unwrap();
    let original = store.record(&id).unwrap();
    // Prove no disk read is required for cached operations, without timing assertions.
    fs::remove_file(store.path(&id).unwrap()).unwrap();
    store.sync(&capture).unwrap();
    let refreshed = store.record(&id).unwrap();
    let selected = store.record(&id).unwrap();
    assert!(Arc::ptr_eq(&original, &refreshed));
    assert!(Arc::ptr_eq(&original, &selected));
    assert_eq!(store.body(&id).unwrap(), "unchanged%3D");
    assert!(store.delete(Some(&id)).unwrap().records.is_empty());
    assert!(store.inner.lock().unwrap().cache.get(&id).is_none());
    assert!(store.sync(&capture).unwrap().records.is_empty());
    assert!(HistoryStore::new(temp.path().into())
        .sync(&ResponseState::default())
        .unwrap()
        .records
        .is_empty());
}

#[test]
fn oversized_records_are_readable_but_not_retained() {
    let temp = tempfile::tempdir().unwrap();
    let store = HistoryStore::new(temp.path().into());
    store.inner.lock().unwrap().cache = RecordCache::new(3, 1024);
    let id = uuid::Uuid::new_v4().to_string();
    let raw = "r".repeat(2048);
    let capture = capture(&id, 1, &raw);
    store.sync(&capture).unwrap();
    assert!(store.inner.lock().unwrap().cache.get(&id).is_none());
    assert_eq!(store.record(&id).unwrap().response.body, raw);
    assert_eq!(store.body(&id).unwrap(), raw);
    assert!(store.inner.lock().unwrap().cache.get(&id).is_none());
}

#[test]
fn corrupt_indexes_and_mismatched_payloads_are_reported_without_rebuilding() {
    let temp = tempfile::tempdir().unwrap();
    let store = HistoryStore::new(temp.path().into());
    let id = uuid::Uuid::new_v4().to_string();
    store.sync(&capture(&id, 1, "raw")).unwrap();
    let index_path = temp.path().join("index.json");
    let valid = fs::read(&index_path).unwrap();
    fs::write(&index_path, "broken index").unwrap();
    assert!(HistoryStore::new(temp.path().into())
        .sync(&ResponseState::default())
        .is_err());
    assert_eq!(fs::read_to_string(&index_path).unwrap(), "broken index");
    fs::write(&index_path, valid).unwrap();
    let mut record = read_record(&store.path(&id).unwrap()).unwrap();
    record.entry.captured_at += 1;
    record.response.captured_at += 1;
    write_json_atomic(&store.path(&id).unwrap(), &record, "test").unwrap();
    let restarted = HistoryStore::new(temp.path().into());
    assert!(restarted.sync(&ResponseState::default()).is_ok());
    assert!(restarted.record(&id).is_err());
}

#[test]
fn parsing_does_not_lock_the_index_or_restore_records_deleted_during_the_read() {
    let temp = tempfile::tempdir().unwrap();
    let store = HistoryStore::new(temp.path().into());
    let id = uuid::Uuid::new_v4().to_string();
    store.sync(&capture(&id, 1, "raw")).unwrap();
    store.inner.lock().unwrap().cache.remove(&id);
    let result = store.record_using(&id, |path| {
        let record = read_record(path)?;
        assert!(store.inner.try_lock().is_ok());
        // These operations would deadlock if decoding held the history lock.
        assert_eq!(
            store.sync(&ResponseState::default()).unwrap().records.len(),
            1
        );
        store.delete(Some(&id)).unwrap();
        Ok(record)
    });
    assert_eq!(result.err().unwrap(), "抓包记录已删除");
    assert!(store.inner.lock().unwrap().cache.get(&id).is_none());
    assert!(store
        .sync(&ResponseState::default())
        .unwrap()
        .records
        .is_empty());
}

#[test]
fn deletion_commits_before_cleanup_and_can_resume_after_restart() {
    let temp = tempfile::tempdir().unwrap();
    let store = HistoryStore::new(temp.path().into());
    let id = uuid::Uuid::new_v4().to_string();
    store.sync(&capture(&id, 1, "raw")).unwrap();
    let path = store.path(&id).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap(); // Deterministic unlink failure on every platform.
    assert!(store.delete(Some(&id)).is_err());
    assert!(store.inner.lock().unwrap().index.records.is_empty());
    let index: HistoryIndex =
        serde_json::from_slice(&fs::read(temp.path().join("index.json")).unwrap()).unwrap();
    assert_eq!(index.pending_deletions, [id]);
    fs::remove_dir(path).unwrap();
    let restarted = HistoryStore::new(temp.path().into());
    assert!(restarted
        .sync(&ResponseState::default())
        .unwrap()
        .records
        .is_empty());
    let index: HistoryIndex =
        serde_json::from_slice(&fs::read(temp.path().join("index.json")).unwrap()).unwrap();
    assert!(index.pending_deletions.is_empty());
}

#[test]
fn index_write_failure_does_not_commit_new_records_or_deletions_in_memory() {
    let temp = tempfile::tempdir().unwrap();
    let store = HistoryStore::new(temp.path().into());
    let first = uuid::Uuid::new_v4().to_string();
    store.sync(&capture(&first, 1, "first")).unwrap();
    let index_path = temp.path().join("index.json");
    fs::remove_file(&index_path).unwrap();
    fs::create_dir(&index_path).unwrap();
    let second = uuid::Uuid::new_v4().to_string();
    assert!(store.sync(&capture(&second, 2, "second")).is_err());
    assert!(store.delete(Some(&first)).is_err());
    assert_eq!(store.inner.lock().unwrap().index.records.len(), 1);
    assert!(store.path(&first).unwrap().is_file());
    fs::remove_dir(&index_path).unwrap();
    store
        .save_index(&store.inner.lock().unwrap().index)
        .unwrap();
    // An uncommitted payload is never imported while an index exists.
    let view = HistoryStore::new(temp.path().into())
        .sync(&ResponseState::default())
        .unwrap();
    assert_eq!(view.records.len(), 1);
    assert_eq!(view.records[0].id, first);
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
