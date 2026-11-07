//! Read-only account projections. Raw responses and persisted history stay untouched.
use crate::commands::catalog::CaptureCatalog;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

mod snapshot;
pub(super) use snapshot::Snapshot;
#[cfg(test)]
mod tests;
mod view;

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PreviewTab {
    #[default]
    Overview,
    Servants,
    CraftEssences,
    Items,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum StorageFilter {
    #[default]
    All,
    Inventory,
    Storage,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PreviewSort {
    #[default]
    Number,
    Level,
    Bond,
    Quantity,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct PreviewQuery {
    pub tab: PreviewTab,
    pub search: String,
    pub class_id: Option<u64>,
    pub rarity: Option<u64>,
    pub storage: StorageFilter,
    pub sort: PreviewSort,
    pub item_category: Option<String>,
    pub limit_broken: bool,
    pub show_zero: bool,
    pub page: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewField {
    pub label: String,
    pub value: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewItem {
    pub id: String,
    pub item_id: u64,
    pub name: String,
    pub quantity: u64,
    pub category: String,
    pub icon: Option<String>,
    pub note: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewServant {
    pub instance_id: String,
    pub svt_id: u64,
    pub number: Option<u64>,
    pub name: String,
    pub rarity: Option<u64>,
    pub class_id: Option<u64>,
    pub class_name: Option<String>,
    pub icon: Option<String>,
    pub level: Option<u64>,
    pub np_level: Option<u64>,
    pub skills: [Option<u64>; 3],
    pub bond: Option<u64>,
    pub in_storage: bool,
    pub details: Vec<PreviewField>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewCeInstance {
    pub instance_id: String,
    pub level: Option<u64>,
    pub limit_count: Option<u64>,
    pub limit_broken: Option<bool>,
    pub in_storage: bool,
    pub locked: Option<bool>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewCe {
    pub svt_id: u64,
    pub number: u64,
    pub name: String,
    pub rarity: Option<u64>,
    pub icon: Option<String>,
    pub quantity: usize,
    pub summary: String,
    pub instances: Vec<PreviewCeInstance>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewSummary {
    pub servant_instances: Option<usize>,
    pub servant_types: Option<usize>,
    pub ce_instances: Option<usize>,
    pub ce_types: Option<usize>,
    pub storage_instances: Option<usize>,
    pub presents: Option<usize>,
    pub unclassified_instances: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewClass {
    pub id: u64,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapturePreview {
    pub record_id: String,
    pub tab: PreviewTab,
    pub profile: Vec<PreviewField>,
    pub currencies: Vec<PreviewItem>,
    pub fruits: Vec<PreviewItem>,
    pub summary: PreviewSummary,
    pub classes: Vec<PreviewClass>,
    pub servants: Vec<PreviewServant>,
    pub craft_essences: Vec<PreviewCe>,
    pub items: Vec<PreviewItem>,
    pub total: usize,
    pub page: usize,
    pub page_count: usize,
}

impl Snapshot {
    pub(crate) fn view(
        &self,
        record_id: &str,
        query: PreviewQuery,
        catalog: &CaptureCatalog,
        assets: &Path,
    ) -> CapturePreview {
        view::build(self, record_id, query, catalog, assets)
    }
}

fn field(label: &str, value: Option<impl ToString>) -> PreviewField {
    PreviewField {
        label: label.into(),
        value: value.map(|v| v.to_string()),
    }
}

fn storage_matches(filter: StorageFilter, in_storage: bool) -> bool {
    match filter {
        StorageFilter::All => true,
        StorageFilter::Inventory => !in_storage,
        StorageFilter::Storage => in_storage,
    }
}

fn icon_at(assets: &Path, relative: &str) -> Option<String> {
    // Catalog icons are relative paths; never permit a resource entry to escape its root.
    if Path::new(relative)
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return None;
    }
    let path = assets.join(relative);
    path.is_file().then(|| path.to_string_lossy().into_owned())
}
