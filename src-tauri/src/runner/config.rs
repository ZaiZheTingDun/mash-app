//! Runner configuration, event, and shared state DTOs.
//! These types are serialized to the frontend, so serde names are part of IPC API.

use super::*;
use crate::commands::settings::NoblePhantasmDetectionMode;

// ---------------------------------------------------------------------------
// Configuration (received from frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServantSlotConfig {
    #[serde(default)]
    pub member_id: Option<String>,
    pub slot_index: u32,
    pub servant_id: u32,
}

fn default_grand_np_card() -> String {
    "auto".into()
}

fn default_grand_card_priority() -> String {
    "damage".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GrandChainPriorityItem {
    MainBraveChain,
    MainReadyNp,
    DeputyBraveChain,
    MainColorChain,
    DeputyColorChain,
    Fallback,
}

pub(super) fn default_grand_chain_priority() -> Vec<GrandChainPriorityItem> {
    vec![
        GrandChainPriorityItem::MainBraveChain,
        GrandChainPriorityItem::MainReadyNp,
        GrandChainPriorityItem::DeputyBraveChain,
        GrandChainPriorityItem::MainColorChain,
        GrandChainPriorityItem::DeputyColorChain,
        GrandChainPriorityItem::Fallback,
    ]
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrandCardRuleConfig {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slots: Vec<GrandCardRuleSlotConfig>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GrandClass {
    Saber,
    Berserker,
}

impl Default for GrandClass {
    fn default() -> Self {
        Self::Saber
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GrandServantRuntimeConfig {
    pub(crate) slot_index: usize,
    pub(crate) servant_id: u32,
    pub(crate) is_support: bool,
    pub(crate) np_card: String,
    pub(crate) priority: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApRecoveryItem {
    Rainbow,
    Gold,
    Silver,
    Bronze,
    Copper,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunConfig {
    pub project_id: String,
    /// Desired party slot order, e.g. [2,0,1,3,4,5]. None = don't reorder.
    pub party_order: Option<Vec<u32>>,
    /// Class filter to tap in the support list (e.g. "Caster").
    pub support_class_filter: Option<String>,
    /// Legacy support servant template key. Only consulted by the no-pin
    /// fallback path in `handle_support_select`; the OCR detector reads
    /// `support_servant_id` instead.
    pub support_servant_name: Option<String>,
    /// Servant id pinned via the team-builder support slot. Drives the
    /// OCR-based `find_supports` lookup. `None` falls back to legacy
    /// behaviour (tap top of the list).
    #[serde(default)]
    pub support_servant_id: Option<u32>,
    /// Actual 0-based team-builder slot index of the pinned support.
    /// The support can sit in front or back line, and Order Change needs
    /// the full 1-6 position map to stay accurate after a swap.
    #[serde(default)]
    pub support_slot_index: Option<u32>,
    /// Stable team-builder slot id of the pinned support member.
    #[serde(default)]
    pub support_member_id: Option<String>,
    /// Craft-essence id pinned via the team-builder support CE slot.
    /// When `Some`, `handle_support_select` runs `verify_support_ce`
    /// against each OCR-detected row and picks the first row whose CE
    /// matches the template at `assets/ces/{id}/card_ce.png`. `None`
    /// (or a missing template) preserves legacy behaviour: pick the
    /// first OCR match.
    #[serde(default)]
    pub support_craft_essence_id: Option<u32>,
    /// Runtime CE artwork threshold injected from persisted recognition
    /// settings when automation starts.
    #[serde(default = "default_support_ce_threshold")]
    pub support_ce_threshold: f64,
    /// Minimum full-artwork score required when a cropped CE variant is selected.
    #[serde(default = "default_support_ce_full_gate_threshold")]
    pub support_ce_full_gate_threshold: f64,
    /// Runtime threshold for optional support CE MLB icon verification.
    #[serde(default = "default_support_icon_threshold")]
    pub support_mlb_icon_threshold: f64,
    /// Runtime threshold for optional support Grand bond icon verification.
    #[serde(default = "default_support_icon_threshold")]
    pub support_bond_icon_threshold: f64,
    /// NP readiness detector selected from global recognition settings.
    /// Defaults to the legacy upper NP-card detector so existing users keep
    /// the same battle behavior unless they explicitly opt into the gauge path.
    #[serde(default)]
    pub noble_phantasm_detection_mode: NoblePhantasmDetectionMode,
    /// Stop automation on any bond level-up result overlay.
    #[serde(default)]
    pub stop_on_bond_level_up: bool,
    /// Stop automation only when a bond level-up result reaches level 10+.
    #[serde(default)]
    pub stop_on_bond_max_level: bool,
    /// Optional extra confirmation after skill taps. Disabled by default
    /// because it adds CV polling between skill actions.
    #[serde(default)]
    pub verify_skill_activation: bool,
    /// Stop this automation run once cumulative five-star CE drops reach the target.
    #[serde(default)]
    pub stop_on_five_star_ce_drop: bool,
    /// Cumulative five-star CE drop target for this automation run.
    #[serde(default = "default_five_star_ce_drop_target_count")]
    pub five_star_ce_drop_target_count: u32,
    /// Save each newly handled loot result page to the app debug directory.
    #[serde(default)]
    pub auto_capture_battle_result_loot: bool,
    #[serde(default = "default_true")]
    pub support_craft_essence_mlb_required: bool,
    #[serde(default)]
    pub support_grand_mode: bool,
    #[serde(default = "default_support_grand_craft_essence_ids")]
    pub support_grand_craft_essence_ids: [Option<u32>; 3],
    #[serde(default = "default_support_grand_craft_essence_mlb_required")]
    pub support_grand_craft_essence_mlb_required: [bool; 3],
    #[serde(default)]
    pub support_grand_bond_ce_mode: SupportGrandBondCeMode,
    #[serde(default)]
    pub grand_servants: Vec<GrandServantConfig>,
    #[serde(default)]
    pub grand_class: GrandClass,
    #[serde(default)]
    pub grand_card_strategy: GrandCardStrategy,
    /// Minimum NP level required for the chosen support row. `None`
    /// disables the filter.
    #[serde(default)]
    pub support_noble_phantasm_level_min: Option<u32>,
    /// Minimum owned skill levels, one entry per skill slot. `None`
    /// means "任意".
    #[serde(default = "default_support_skill_level_mins")]
    pub support_skill_level_mins: [Option<u32>; 3],
    /// Minimum append skill levels, one entry per append slot. `None`
    /// means "任意".
    #[serde(default = "default_support_append_skill_level_mins")]
    pub support_append_skill_level_mins: [Option<u32>; 5],
    /// Servants to place into specific party slots.
    pub servant_selections: Vec<ServantSlotConfig>,
    /// Max scrolls before refreshing the support list.
    pub max_support_scrolls: u32,
    /// Drives the BattleResultContinue branch: when `true`, the runner
    /// taps "Next" on the continue page so FGO re-queues the same quest;
    /// when `false`, it taps "Close" and the run finishes.
    #[serde(default)]
    pub repeat_mission: bool,
    /// Optional run cap measured in completed quest rounds. When set, it
    /// takes precedence over `repeat_mission`: the runner keeps repeating
    /// until this many final continue pages have been reached, then stops.
    #[serde(default)]
    pub max_mission_runs: Option<u32>,
    /// Apple/AP recovery items allowed when the repeat tap opens the
    /// insufficient-AP dialog. Empty means stop on that dialog.
    #[serde(default)]
    pub ap_recovery_items: Vec<ApRecoveryItem>,
}

fn default_support_skill_level_mins() -> [Option<u32>; 3] {
    [None; 3]
}

fn default_support_ce_threshold() -> f64 {
    crate::commands::settings::SUPPORT_CE_THRESHOLD_DEFAULT
}

fn default_support_ce_full_gate_threshold() -> f64 {
    crate::commands::settings::SUPPORT_CE_FULL_GATE_THRESHOLD_DEFAULT
}

fn default_support_icon_threshold() -> f64 {
    crate::commands::settings::SUPPORT_ICON_THRESHOLD_DEFAULT
}

fn default_true() -> bool {
    true
}

fn default_five_star_ce_drop_target_count() -> u32 {
    1
}

fn default_support_append_skill_level_mins() -> [Option<u32>; 5] {
    [None; 5]
}

fn default_support_grand_craft_essence_ids() -> [Option<u32>; 3] {
    [None; 3]
}

fn default_support_grand_craft_essence_mlb_required() -> [bool; 3] {
    [true; 3]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

// ---------------------------------------------------------------------------
// Runner state (shared with Tauri commands)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum RunnerState {
    Idle,
    Starting,
    Running,
    Finished,
    Error { message: String },
}

/// Severity of an automation status log entry. `Info` is the normal,
/// user-facing channel — every action the runner takes, every screen it
/// transitions through. `Debug` is reserved for technical diagnostics
/// the operator usually doesn't need to see (e.g. raw CV anchor
/// coordinates, computed swipe distances) but that are valuable when
/// triaging a bug report. `LocalDebug` is for noisier diagnostics that
/// should only surface in local development builds. The frontend filters
/// out diagnostic entries by default and exposes a toggle for power users.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Info,
    Warn,
    Debug,
    LocalDebug,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationEvent {
    pub state: String,
    pub current_screen: String,
    pub message: String,
    pub level: LogLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack: Option<AttackLogMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<ActionLogMeta>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ActionLogMeta {
    ServantSkill {
        servant_id: Option<u32>,
        skill_index: u32,
        target_servant_id: Option<u32>,
    },
    EquipmentSkill {
        skill_index: u32,
        target_servant_id: Option<u32>,
    },
    CommandSpell {
        spell: String,
        target_servant_id: Option<u32>,
    },
    OrderChange {
        front_servant_id: Option<u32>,
        back_servant_id: Option<u32>,
    },
    SkippedAction {
        servant_id: Option<u32>,
    },
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttackLogMeta {
    pub front_servant_ids: [Option<u32>; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_servant_ids: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_cards: Option<Vec<AttackLogCommandCard>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready_np_slots: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_pick: Option<AttackLogSelectedPick>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttackLogCommandCard {
    pub slot: u32,
    pub suit: Option<String>,
    pub servant_id: Option<u32>,
    pub is_support: bool,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttackLogSelectedPick {
    pub step: usize,
    pub total: usize,
    pub from_priority: Option<String>,
    pub kind: AttackLogPickKind,
    pub slot: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servant_id: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AttackLogPickKind {
    Np,
    Card,
}

/// Handle stored in Tauri managed state to control / observe the runner.
pub struct RunnerHandle {
    pub state: Arc<Mutex<RunnerState>>,
    pub cancel: Arc<AtomicBool>,
    pub stop_after_current: Arc<AtomicBool>,
}

impl RunnerHandle {
    pub fn new_idle() -> Self {
        Self {
            state: Arc::new(Mutex::new(RunnerState::Idle)),
            cancel: Arc::new(AtomicBool::new(false)),
            stop_after_current: Arc::new(AtomicBool::new(false)),
        }
    }
}
