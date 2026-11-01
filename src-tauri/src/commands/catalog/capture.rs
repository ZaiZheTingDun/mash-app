//! Compact, generation-aware identity lookup for captured account snapshots.
use super::data::{catalog_json, CatalogCache};
use super::{preferred_cn_name, string_field};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

pub(crate) struct CaptureIdentity {
    pub number: u64,
    pub name: String,
    pub rarity: Option<u64>,
    pub class_id: Option<u64>,
    pub class_name: Option<String>,
    pub icon: Option<String>,
}

pub(crate) struct CaptureCatalog {
    pub servants: HashMap<u64, CaptureIdentity>,
    pub ces: HashMap<u64, CaptureIdentity>,
    pub items: HashMap<u64, CaptureIdentity>,
}

pub(crate) fn capture_catalog() -> Result<Arc<CaptureCatalog>, String> {
    static CACHE: CatalogCache<CaptureCatalog> = CatalogCache::new();
    CACHE.get_or_try_init(|| {
        let servants: Vec<Value> = catalog_json("servants.json")?;
        let ces: Vec<Value> = catalog_json("craft_essences.json")?;
        #[derive(Deserialize)]
        struct Items {
            items: Vec<Value>,
        }
        let items: Items = catalog_json("event_items.json")?;
        Ok(CaptureCatalog {
            servants: identities(servants, true),
            ces: identities(ces, false),
            items: items
                .items
                .into_iter()
                .filter_map(|v| {
                    let id = v["id"].as_u64()?;
                    Some((
                        id,
                        CaptureIdentity {
                            number: id,
                            name: preferred_cn_name(&v)
                                .or_else(|| string_field(&v, &["name"]))
                                .unwrap_or_else(|| format!("物品 #{id}")),
                            rarity: None,
                            class_id: None,
                            class_name: None,
                            icon: string_field(&v, &["icon"]),
                        },
                    ))
                })
                .collect(),
        })
    })
}

fn identities(rows: Vec<Value>, servant: bool) -> HashMap<u64, CaptureIdentity> {
    rows.into_iter()
        .filter_map(|v| {
            let id = v["id"].as_u64()?;
            Some((
                id,
                CaptureIdentity {
                    number: v["collectionNo"].as_u64()?,
                    name: preferred_cn_name(&v)
                        .or_else(|| string_field(&v, &["nameJP", "name"]))
                        .unwrap_or_else(|| format!("#{id}")),
                    rarity: v["rarity"].as_u64(),
                    class_id: servant.then(|| v["classId"].as_u64()).flatten(),
                    class_name: servant.then(|| string_field(&v, &["className"])).flatten(),
                    icon: None,
                },
            ))
        })
        .collect()
}
