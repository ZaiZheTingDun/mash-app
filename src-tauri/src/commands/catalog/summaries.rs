//! Catalog summaries from the installed resource pack only.
//! Individual asset JSON is only for details absent from these compact records.
use super::data::{catalog_json, CatalogCache};
use crate::server::Server;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CraftEssenceSummary {
    pub collection_no: u32,
    pub name: String,
    #[serde(rename = "name_cn")]
    pub name_cn: Option<String>,
    #[serde(rename = "name_link")]
    pub name_link: Option<String>,
    pub rarity: u8,
    #[serde(default)]
    pub flag: String,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub regions: HashMap<Server, Option<RegionBonuses>>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EventSchedule {
    pub started_at: u64,
    pub ended_at: u64,
}

#[derive(Deserialize)]
pub(super) struct EventSummary {
    pub id: u32,
    #[serde(rename = "JP")]
    pub jp: Option<EventSchedule>,
    #[serde(rename = "CN")]
    pub cn: Option<EventSchedule>,
}

#[derive(Deserialize)]
pub(super) struct EventCatalog {
    pub events: Vec<EventSummary>,
}

#[derive(Clone, Deserialize)]
pub(super) struct Item {
    pub name: String,
    #[serde(rename = "nameCN")]
    pub name_cn: Option<String>,
    pub icon: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ItemCatalog {
    pub items: Vec<Item>,
}

pub(super) struct CatalogSummaries {
    pub ces: Vec<CraftEssenceSummary>,
    pub events: EventCatalog,
    pub items: ItemCatalog,
}

pub(super) fn catalog_summaries() -> Result<Arc<CatalogSummaries>, String> {
    static SUMMARIES: CatalogCache<CatalogSummaries> = CatalogCache::new();
    SUMMARIES.get_or_try_init(|| {
        Ok(CatalogSummaries {
            ces: catalog_json("craft_essences.json")?,
            events: catalog_json("events.json")?,
            items: catalog_json("event_items.json")?,
        })
    })
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct Bonus {
    pub(super) event_id: u32,
    pub(super) limit_break: u8,
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) function_id: u32,
    pub(super) skill_id: u32,
    pub(super) function_type: String,
    pub(super) buff_type: Option<String>,
    pub(super) target_id: Option<u32>,
    pub(super) flat: Option<f64>,
    pub(super) percent: Option<f64>,
    pub(super) dynamic: bool,
    pub(super) group: serde_json::Value,
    pub(super) conditions: serde_json::Value,
    pub(super) values: Vec<RewardBonusValue>,
    pub(super) support_values: Option<Vec<RewardBonusValue>>,
}

#[derive(Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct RewardBonusValue {
    pub(super) percent: Option<f64>,
    pub(super) flat: Option<f64>,
    pub(super) conditions: RewardBonusConditions,
}

#[derive(Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct RewardBonusConditions {
    #[serde(rename = "EventId")]
    pub(super) event_id: u32,
    #[serde(flatten)]
    pub(super) extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RegionBonuses {
    #[serde(default)]
    pub(super) bond_equip_owner: Option<u32>,
    #[serde(default)]
    pub(super) bond_bonuses: Vec<Bonus>,
    #[serde(default)]
    pub(super) qp_bonuses: Vec<Bonus>,
    #[serde(default)]
    pub(super) master_exp_bonuses: Vec<Bonus>,
    #[serde(default)]
    pub(super) mystic_code_exp_bonuses: Vec<Bonus>,
    #[serde(default)]
    pub(super) friend_point_bonuses: Vec<Bonus>,
    #[serde(default)]
    pub(super) event_bonuses: Vec<Bonus>,
}
