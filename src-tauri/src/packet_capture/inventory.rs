//! A small, read-only projection of common account items from login caches.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapturedItem {
    pub item_id: u64,
    pub name: String,
    pub quantity: u64,
}

fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn collect(value: &Value, items: &mut HashMap<u64, u64>, depth: usize) {
    if depth > 10 {
        return;
    }
    match value {
        Value::Object(object) => {
            // Read only live cache collections, never deleted rows or request data.
            if let Some(rows) = object.get("userItem").and_then(Value::as_array) {
                for row in rows {
                    if let (Some(id), Some(quantity)) = (
                        row.get("itemId").and_then(number),
                        row.get("num").and_then(number),
                    ) {
                        items.insert(id, quantity);
                    }
                }
            }
            for key in [
                "response", "success", "data", "cache", "replaced", "updated",
            ] {
                if let Some(child) = object.get(key) {
                    collect(child, items, depth + 1);
                }
            }
        }
        Value::Array(rows) => rows.iter().for_each(|row| collect(row, items, depth + 1)),
        _ => {}
    }
}

pub(super) fn preview(raw: &str) -> Vec<CapturedItem> {
    let Some(json) = super::response::decoded_json(raw) else {
        return Vec::new();
    };
    let mut items = HashMap::new();
    collect(&json, &mut items, 0);
    // Atlas CN item IDs. Only show quantities actually present in this snapshot.
    [
        (1, "QP"),
        (2, "圣晶石"),
        (100, "黄金果实"),
        (101, "白银果实"),
        (104, "青铜果实"),
        (102, "赤铜果实"),
    ]
    .into_iter()
    .filter_map(|(item_id, name)| {
        items.get(&item_id).map(|quantity| CapturedItem {
            item_id,
            name: name.into(),
            quantity: *quantity,
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn previews_encoded_live_items_and_updates_without_inventing_missing_counts() {
        let json = r#"{"response":[{"success":{"cache":{"replaced":{"userItem":[{"itemId":1,"num":1234},{"itemId":100,"num":10}]},"updated":{"userItem":[{"itemId":"100","num":"12"},{"itemId":101,"num":0}]},"deleted":{"userItem":[{"itemId":1,"num":0}]}}}}]}"#;
        let encoded = base64::engine::general_purpose::URL_SAFE
            .encode(json)
            .replace('=', "%3D");
        for raw in [json.to_string(), encoded] {
            let result = preview(&raw);
            assert_eq!(
                result
                    .iter()
                    .map(|item| (item.item_id, item.quantity))
                    .collect::<Vec<_>>(),
                [(1, 1234), (100, 12), (101, 0)]
            );
            assert_eq!(result[1].name, "黄金果实");
        }
        for raw in ["invalid", "{}", r#"{"userItem":[{"itemId":2,"num":-1}]}"#] {
            assert!(preview(raw).is_empty());
        }
    }
}
