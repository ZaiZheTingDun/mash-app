//! Persisted project, support, Grand, recognition, and catalog wire models.

use crate::runner::{ApRecoveryItem, ApRecoveryLimits};

/// One cell of the team-builder grid. The frontend stores six of these per
/// project (5 servant slots + 1 support slot) along with their order, so
/// drag-and-drop layouts and chosen servants survive across sessions.
///
/// `kind` is either `"servant"` or `"support"`. For support slots,
/// `servant_id` is ignored — the pinned servant lives on
/// `Project::support_servant_id` (kept separate because the runner reads
/// it through `RunConfig::support_servant_id` and we don't want two
/// sources of truth for the same value).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSlot {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub servant_id: Option<u32>,
    /// Optional UI variant key for servants with multiple gameplay
    /// variants. The runner still acts on the base servant id; this
    /// keeps the team builder showing the exact variant the user picked.
    #[serde(default)]
    pub servant_variant_key: Option<String>,
    /// Pinned craft-essence id for this slot. Persisted alongside the
    /// servant so each loadout can carry its own equipment plan; for
    /// support slots the runner uses this to verify candidate rows on
    /// the support-select screen, party slots store it for future use.
    #[serde(default)]
    pub craft_essence_id: Option<u32>,
    /// Ordered allow-list used by ordinary support selection. Legacy
    /// projects only carry `craft_essence_id`; normalization migrates that
    /// value into this list. The UI and runner cap it at ten unique ids.
    #[serde(default)]
    pub craft_essence_ids: Vec<u32>,
    #[serde(default)]
    pub craft_essence_multi_select: bool,
    #[serde(default = "default_true")]
    pub craft_essence_mlb_required: bool,
}

fn default_true() -> bool {
    true
}

/// Default 6-slot layout used both when creating a fresh project and when
/// deserializing a legacy `projects.json` that predates the `slots` field.
pub(crate) fn default_project_slots() -> Vec<ProjectSlot> {
    let new_slot = |id: &str, kind: &str| ProjectSlot {
        id: id.into(),
        kind: kind.into(),
        servant_id: None,
        servant_variant_key: None,
        craft_essence_id: None,
        craft_essence_ids: Vec::new(),
        craft_essence_multi_select: false,
        craft_essence_mlb_required: true,
    };
    vec![
        new_slot("slot-0", "servant"),
        new_slot("slot-1", "servant"),
        new_slot("slot-2", "support"),
        new_slot("slot-3", "servant"),
        new_slot("slot-4", "servant"),
        new_slot("slot-5", "servant"),
    ]
}

pub(crate) fn default_support_skill_level_mins() -> [Option<u32>; 3] {
    [None; 3]
}

pub(crate) fn default_support_append_skill_level_mins() -> [Option<u32>; 5] {
    [None; 5]
}

pub(crate) fn default_support_grand_craft_essence_ids() -> [Option<u32>; 3] {
    [None; 3]
}

pub(crate) fn default_support_grand_craft_essence_id_lists() -> [Vec<u32>; 3] {
    std::array::from_fn(|_| Vec::new())
}

pub(crate) fn default_support_grand_craft_essence_mlb_required() -> [bool; 3] {
    [true; 3]
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SupportGrandBondCeMode {
    Any,
    Bond,
    BondNp,
}

impl Default for SupportGrandBondCeMode {
    fn default() -> Self {
        Self::Any
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum GrandClass {
    Saber,
    Lancer,
    Rider,
    Berserker,
    Extra1Fire,
    Extra1Earth,
    Extra2Wind,
    Extra2Water,
}

impl GrandClass {
    pub const ALL: [Self; 8] = [
        Self::Saber,
        Self::Lancer,
        Self::Rider,
        Self::Berserker,
        Self::Extra1Fire,
        Self::Extra1Earth,
        Self::Extra2Wind,
        Self::Extra2Water,
    ];
}

impl Default for GrandClass {
    fn default() -> Self {
        Self::Saber
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum ProjectRepeatMode {
    Single,
    Infinite,
    Count,
}

impl Default for ProjectRepeatMode {
    fn default() -> Self {
        Self::Single
    }
}

pub(crate) fn default_grand_np_card() -> String {
    "auto".into()
}

pub(crate) fn default_grand_card_priority() -> String {
    "damage".into()
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GrandChainPriorityItem {
    MainBraveChain,
    MainReadyNp,
    DeputyBraveChain,
    MainColorChain,
    DeputyColorChain,
    Fallback,
}

pub(crate) fn default_grand_chain_priority() -> Vec<GrandChainPriorityItem> {
    vec![
        GrandChainPriorityItem::MainBraveChain,
        GrandChainPriorityItem::MainReadyNp,
        GrandChainPriorityItem::DeputyBraveChain,
        GrandChainPriorityItem::MainColorChain,
        GrandChainPriorityItem::DeputyColorChain,
        GrandChainPriorityItem::Fallback,
    ]
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrandCardStrategy {
    #[serde(default = "default_grand_chain_priority")]
    pub chain_priority: Vec<GrandChainPriorityItem>,
    #[serde(default)]
    pub custom_rules: Vec<GrandCardRuleConfig>,
}

impl Default for GrandCardStrategy {
    fn default() -> Self {
        Self {
            chain_priority: default_grand_chain_priority(),
            custom_rules: Vec::new(),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrandCardRuleSlotConfig {
    #[serde(default)]
    pub member_id: Option<String>,
    #[serde(default)]
    pub slot_index: Option<u32>,
    #[serde(default)]
    pub servant_id: Option<u32>,
    #[serde(default)]
    pub is_support: bool,
    #[serde(default)]
    pub grand_servant: bool,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub color: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrandCardRuleConfig {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slots: Vec<GrandCardRuleSlotConfig>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrandServantConfig {
    #[serde(default)]
    pub member_id: Option<String>,
    pub slot_index: u32,
    #[serde(default)]
    pub servant_id: Option<u32>,
    #[serde(default)]
    pub is_support: bool,
    #[serde(default = "default_grand_np_card")]
    pub np_card: String,
    #[serde(default = "default_grand_card_priority")]
    pub priority: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Legacy persisted field. Normalization migrates it into `role`, and
    /// serialization intentionally omits it from newly saved projects.
    #[serde(default, skip_serializing)]
    pub lancer_role: Option<LancerGrandRole>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LancerGrandRole {
    Single,
    Aoe,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GrandRoleDefinition {
    pub role: String,
    pub label: String,
    pub required: bool,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GrandClassDefinition {
    pub id: GrandClass,
    pub label: String,
    pub servant_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_group_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_option_label: Option<String>,
    pub roles: Vec<GrandRoleDefinition>,
    pub card_priority_enabled: bool,
    pub auto_order_change_roles: Vec<String>,
    pub validation_message: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecognitionSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_ce_threshold: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_ce_full_gate_threshold: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_mlb_icon_threshold: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_bond_icon_threshold: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_skill_activation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_extra_class_filter: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_on_five_star_ce_drop: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_star_ce_drop_target_count: Option<u32>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    /// Advanced teams use rule-based battle configuration stored separately
    /// from the legacy preparation/attack scene list.
    #[serde(default)]
    pub advanced_mode: bool,
    /// Pinned support-select servant id. The runner's `handle_support_select`
    /// reads this through `RunConfig::support_servant_id` to drive the OCR
    /// detector. `None` means the user hasn't pinned anyone yet, in which
    /// case the runner falls back to tapping the topmost visible support.
    /// `#[serde(default)]` so legacy `projects.json` rows without the field
    /// continue to deserialize.
    #[serde(default)]
    pub support_servant_id: Option<u32>,
    #[serde(default)]
    pub support_servant_variant_key: Option<String>,
    #[serde(default)]
    pub support_grand_mode: bool,
    #[serde(default = "default_support_grand_craft_essence_ids")]
    pub support_grand_craft_essence_ids: [Option<u32>; 3],
    #[serde(default = "default_support_grand_craft_essence_id_lists")]
    pub support_grand_craft_essence_id_lists: [Vec<u32>; 3],
    #[serde(default = "default_support_grand_craft_essence_mlb_required")]
    pub support_grand_craft_essence_mlb_required: [bool; 3],
    #[serde(default)]
    pub support_grand_bond_ce_mode: SupportGrandBondCeMode,
    #[serde(default)]
    pub grand_class: GrandClass,
    #[serde(default)]
    pub grand_servants: Vec<GrandServantConfig>,
    #[serde(default)]
    pub grand_card_strategy: GrandCardStrategy,
    /// Optional minimum servant level required for the chosen support row.
    #[serde(default)]
    pub support_servant_level_min: Option<u32>,
    /// Optional support-search NP minimum level. `None` means "任意".
    #[serde(default)]
    pub support_noble_phantasm_level_min: Option<u32>,
    /// Optional minimum ordinary star-map score (0-62).
    #[serde(default)]
    pub support_star_map_score_min: Option<u32>,
    /// Optional minimum Grand star-map score (0-16), used in Grand mode.
    #[serde(default)]
    pub support_grand_star_map_score_min: Option<u32>,
    /// Optional support-search owned skill minimum levels, one entry per
    /// skill slot. `None` means "任意".
    #[serde(default = "default_support_skill_level_mins")]
    pub support_skill_level_mins: [Option<u32>; 3],
    /// Optional support-search append skill minimum levels, one entry per
    /// append slot. `None` means "任意".
    #[serde(default = "default_support_append_skill_level_mins")]
    pub support_append_skill_level_mins: [Option<u32>; 5],
    /// Optional project-specific recognition thresholds. `None` means this
    /// project inherits the global recognition settings.
    #[serde(default)]
    pub recognition_settings: Option<ProjectRecognitionSettings>,
    #[serde(default)]
    pub disable_auto_skill_target_recognition: bool,
    #[serde(default)]
    pub mystic_code_id: Option<u32>,
    /// When two command cards belong to the same member and have the same
    /// color, prefer the one with the higher recognized critical chance.
    #[serde(default)]
    pub prefer_higher_critical_chance: bool,
    /// Team-builder grid layout (chosen servants + slot order). Persisted
    /// so the user's selections survive app restarts and project switches.
    /// Defaulted via `default_project_slots` for legacy rows.
    #[serde(default = "default_project_slots")]
    pub slots: Vec<ProjectSlot>,
    /// When `true`, the runner taps "Next" on the post-battle continue
    /// screen so the same quest is queued again; when `false`, it taps
    /// "Close" and the run terminates. `#[serde(default)]` keeps legacy
    /// rows (no field) defaulting to `false` = single-run behaviour.
    #[serde(default)]
    pub repeat_mission: bool,
    /// Three-way repeat selector persisted for the start page. `None`
    /// indicates a legacy row and is normalized from `repeat_mission`.
    #[serde(default)]
    pub repeat_mode: Option<ProjectRepeatMode>,
    /// Persisted run count used when `repeat_mode == Count`.
    #[serde(default)]
    pub repeat_count: Option<u32>,
    /// Persisted AP recovery items in UI priority order.
    #[serde(default)]
    pub ap_recovery_items: Vec<ApRecoveryItem>,
    /// Persisted per-run use caps. Missing/null item limits mean unlimited.
    #[serde(default)]
    pub ap_recovery_limits: ApRecoveryLimits,
}

/// User-defined, one-level grouping for projects. Project execution data stays
/// in `projects.json`; this catalog only owns organization and display order.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub project_ids: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCatalog {
    #[serde(default = "default_project_catalog_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub groups: Vec<ProjectGroup>,
    #[serde(default)]
    pub ungrouped_project_ids: Vec<String>,
}

fn default_project_catalog_schema_version() -> u32 {
    1
}

impl Default for ProjectCatalog {
    fn default() -> Self {
        Self {
            schema_version: default_project_catalog_schema_version(),
            groups: Vec::new(),
            ungrouped_project_ids: Vec::new(),
        }
    }
}
