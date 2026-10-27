use super::*;

fn record(id: &str, body: &str) -> Arc<CaptureRecord> {
    Arc::new(
        serde_json::from_value(serde_json::json!({
            "entry": { "id": id, "capturedAt": 1, "serial": "device" },
            "response": {
                "captureId": id, "capturedAt": 1, "recordId": 42,
                "url": "https://example.test", "statusCode": 200,
                "byteLength": body.len(), "format": "raw", "body": body, "items": []
            }
        }))
        .unwrap(),
    )
}

#[test]
fn count_limit_evicts_least_recently_used_and_hits_share_the_same_allocation() {
    let mut cache = RecordCache::default();
    let first = record("1", "raw");
    cache.insert(first.clone());
    cache.insert(record("2", "raw"));
    cache.insert(record("3", "raw"));
    assert!(Arc::ptr_eq(&cache.get("1").unwrap(), &first));
    cache.insert(record("4", "raw"));
    assert!(cache.get("2").is_none());
    assert_eq!(cache.records.len(), MAX_RECORDS);
    assert!(cache.get("1").is_some());
}

#[test]
fn byte_budget_evicts_records_and_oversized_entries_do_not_flush_hot_records() {
    let first = record("1", "first");
    let second = record("2", "second");
    let budget = first.retained_bytes() + second.retained_bytes() - 1;
    let mut cache = RecordCache::new(3, budget);
    cache.insert(first);
    cache.insert(second.clone());
    assert!(cache.get("1").is_none());
    assert!(cache.get("2").is_some());
    cache.insert(record("large", &"x".repeat(budget)));
    assert!(cache.get("large").is_none());
    assert!(cache.get("2").is_some());
    assert_eq!(cache.bytes, second.retained_bytes());
    cache.insert(second);
    assert_eq!(cache.records.len(), 1);
    cache.remove("2");
    assert_eq!(cache.bytes, 0);
}

#[test]
fn allocated_capacity_counts_even_when_string_length_is_small() {
    let mut record = record("1", "raw");
    Arc::get_mut(&mut record)
        .unwrap()
        .response
        .body
        .reserve(2048);
    let mut cache = RecordCache::new(3, 1024);
    cache.insert(record);
    assert!(cache.records.is_empty());
    assert_eq!(cache.bytes, 0);
}
