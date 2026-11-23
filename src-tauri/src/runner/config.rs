//! Runner configuration, event, and shared state DTOs.
//! These types are serialized to the frontend, so serde names are part of IPC API.

use super::*;
use crate::commands::settings::NoblePhantasmDetectionMode;
use crate::{ProjectRepeatMode, SupportClassFilterMode};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RankUpQuestMode {
    Single,
    All,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankUpQuestStartRequest {
    pub mode: RankUpQuestMode,
    #[serde(default)]
    pub capture_id: Option<String>,
    #[serde(default)]
    pub candidate_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RankUpQuestTarget {
    pub reference_path: PathBuf,
    pub signature_regions: Vec<NormRect>,
}

#[derive(Debug, Clone)]
pub struct RankUpQuestWorkflowConfig {
    pub mode: RankUpQuestMode,
    pub target: Option<RankUpQuestTarget>,
}

impl RankUpQuestWorkflowConfig {
    pub fn all() -> Self {
        Self {
            mode: RankUpQuestMode::All,
            target: None,
        }
    }

    pub fn single(target: RankUpQuestTarget) -> Self {
        Self {
            mode: RankUpQuestMode::Single,
            target: Some(target),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum InterludeWorkflowConfig {
    All,
    Single(crate::screen::InterludeIdentity),
}

#[derive(Debug, Clone)]
pub(crate) struct GrandServantRuntimeConfig {
    pub(crate) slot_index: usize,
    pub(crate) servant_id: u32,
    pub(crate) is_support: bool,
    pub(crate) np_card: String,
    pub(crate) priority: String,
    pub(crate) role: String,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ApRecoveryLimits {
    pub rainbow: Option<u32>,
    pub gold: Option<u32>,
    pub silver: Option<u32>,
    pub bronze: Option<u32>,
    pub copper: Option<u32>,
}

impl ApRecoveryLimits {
    pub(crate) fn get(self, item: ApRecoveryItem) -> Option<u32> {
        match item {
            ApRecoveryItem::Rainbow => self.rainbow,
            ApRecoveryItem::Gold => self.gold,
            ApRecoveryItem::Silver => self.silver,
            ApRecoveryItem::Bronze => self.bronze,
            ApRecoveryItem::Copper => self.copper,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BattleRunApRecoveryUsage {
    pub rainbow: u32,
    pub gold: u32,
    pub silver: u32,
    pub bronze: u32,
    pub copper: u32,
}

impl BattleRunApRecoveryUsage {
    pub(crate) fn get(&self, item: ApRecoveryItem) -> u32 {
        match item {
            ApRecoveryItem::Rainbow => self.rainbow,
            ApRecoveryItem::Gold => self.gold,
            ApRecoveryItem::Silver => self.silver,
            ApRecoveryItem::Bronze => self.bronze,
            ApRecoveryItem::Copper => self.copper,
        }
    }

    pub(crate) fn increment(&mut self, item: ApRecoveryItem) {
        let count = match item {
            ApRecoveryItem::Rainbow => &mut self.rainbow,
            ApRecoveryItem::Gold => &mut self.gold,
            ApRecoveryItem::Silver => &mut self.silver,
            ApRecoveryItem::Bronze => &mut self.bronze,
            ApRecoveryItem::Copper => &mut self.copper,
        };
        *count += 1;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BattleRunProgressEvent {
    pub completed_runs: u32,
    pub max_runs: Option<u32>,
    pub ap_recovery_usage: BattleRunApRecoveryUsage,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunConfig {
    pub project_id: String,
    #[serde(skip)]
    pub rank_up_quest: Option<RankUpQuestWorkflowConfig>,
    #[serde(skip)]
    pub interlude: Option<InterludeWorkflowConfig>,
    /// Mystic Code selected for this team. When present, TeamConfirm checks
    /// the outfit icon in the lower-left corner before starting the quest.
    #[serde(default)]
    pub mystic_code_id: Option<u32>,
    /// Desired party slot order, e.g. [2,0,1,3,4,5]. None = don't reorder.
    pub party_order: Option<Vec<u32>>,
    /// Class filter to tap in the support list (e.g. "Caster").
    pub support_class_filter: Option<String>,
    /// Which class tabs to search before refreshing the support list.
    #[serde(default)]
    pub support_class_filter_mode: SupportClassFilterMode,
    /// Legacy support servant template key. Only consulted by the no-pin
    /// fallback path in `handle_support_select`; the OCR detector reads
    /// `support_servant_id` instead.
    pub support_servant_name: Option<String>,
    /// Servant id pinned via the team-builder support slot. Drives the
    /// OCR-based `find_supports` lookup. `None` falls back to legacy
    /// behaviour (tap top of the list).
    #[serde(default)]
    pub support_servant_id: Option<u32>,
    /// Exact servant variant selected in the team builder. Servants that
    /// share one collection id can still have different in-game names and
    /// skill sets, so support OCR must not search every alias for the id.
    #[serde(default)]
    pub support_servant_variant_key: Option<String>,
    /// Enables the CN-only second-level EXTRA class dialog. Defaults to true
    /// so projects and start payloads saved before this option retain the
    /// existing concrete-class filtering behavior.
    #[serde(default = "default_true")]
    pub enable_extra_class_filter: bool,
    /// Retry support OCR with the full-list detector when the fast
    /// anchor-row recognizer cannot resolve the configured target.
    #[serde(default)]
    pub support_full_list_ocr_fallback: bool,
    /// Actual 0-based team-builder slot index of the pinned support.
    /// The support can sit in front or back line, and Order Change needs
    /// the full 1-6 position map to stay accurate after a swap.
    #[serde(default)]
    pub support_slot_index: Option<u32>,
    /// Stable team-builder slot id of the pinned support member.
    #[serde(default)]
    pub support_member_id: Option<String>,
    /// Legacy single craft-essence id pinned via the team-builder support
    /// CE slot. New projects also populate `support_craft_essence_ids`;
    /// this field remains as a fallback for older start payloads.
    #[serde(default)]
    pub support_craft_essence_id: Option<u32>,
    /// Ordered ordinary-support CE allow-list. A candidate row passes when
    /// its card matches any configured entry. Empty keeps the legacy single
    /// `support_craft_essence_id` behavior.
    #[serde(default)]
    pub support_craft_essence_ids: Vec<u32>,
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
    /// Save every detected bond level-up result overlay.
    #[serde(default)]
    pub auto_capture_bond_level_up: bool,
    /// Optional extra confirmation after skill taps. Disabled by default
    /// because it adds CV polling between skill actions.
    #[serde(default)]
    pub verify_skill_activation: bool,
    /// Consecutive Unknown detections allowed during ordinary screen transitions.
    #[serde(default = "default_unknown_screen_timeout_count")]
    pub unknown_screen_timeout_count: u32,
    /// Delay after selecting an AP recovery item, before tapping confirmation.
    #[serde(default = "default_ap_recovery_confirm_wait_ms")]
    pub ap_recovery_confirm_wait_ms: u32,
    /// Stop this automation run once cumulative five-star CE drops reach the target.
    #[serde(default)]
    pub stop_on_five_star_ce_drop: bool,
    /// Cumulative five-star CE drop target for this automation run.
    #[serde(default = "default_five_star_ce_drop_target_count")]
    pub five_star_ce_drop_target_count: u32,
    /// Save each newly handled loot result page to the app debug directory.
    #[serde(default)]
    pub auto_capture_battle_result_loot: bool,
    /// Save a lossless Battle frame immediately before tapping Attack.
    #[serde(default)]
    pub auto_capture_battle_before_attack: bool,
    #[serde(default)]
    pub auto_friend_request: bool,
    /// Save the exact frame used by the skill-use dialog probe.
    #[serde(default)]
    pub auto_capture_skill_use_probe: bool,
    /// Save the current Attack frame when critical chance OCR is incomplete.
    #[serde(default)]
    pub auto_capture_unrecognized_critical_chance: bool,
    /// Prefer higher critical chance only between otherwise-equivalent
    /// command cards (same member and color).
    #[serde(default)]
    pub prefer_higher_critical_chance: bool,
    #[serde(default = "default_true")]
    pub support_craft_essence_mlb_required: bool,
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
    pub grand_servants: Vec<GrandServantConfig>,
    #[serde(default)]
    pub grand_class: GrandClass,
    #[serde(default)]
    pub grand_card_strategy: GrandCardStrategy,
    /// Minimum servant level required for the chosen support row.
    #[serde(default)]
    pub support_servant_level_min: Option<u32>,
    /// Minimum NP level required for the chosen support row. `None`
    /// disables the filter.
    #[serde(default)]
    pub support_noble_phantasm_level_min: Option<u32>,
    /// Minimum ordinary star-map score required for the chosen support row.
    #[serde(default)]
    pub support_star_map_score_min: Option<u32>,
    /// Minimum Grand star-map score required in Grand support mode.
    #[serde(default)]
    pub support_grand_star_map_score_min: Option<u32>,
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
    /// Maximum uses per recovery item for this run. `None` means unlimited.
    #[serde(default)]
    pub ap_recovery_limits: ApRecoveryLimits,
}

impl RunConfig {
    pub(crate) fn from_project(project: &crate::Project) -> Self {
        let support = project
            .slots
            .iter()
            .enumerate()
            .find(|(_, slot)| slot.kind == "support");
        let support_slot = support.map(|(_, slot)| slot);
        let recognition = project.recognition_settings.unwrap_or_default();
        let stop_on_five_star_ce_drop = recognition.stop_on_five_star_ce_drop == Some(true);
        let (repeat_mission, max_mission_runs) = match project.repeat_mode.as_ref() {
            Some(ProjectRepeatMode::Infinite) => (true, None),
            Some(ProjectRepeatMode::Count) => {
                (false, project.repeat_count.filter(|count| *count > 0))
            }
            _ => (false, None),
        };
        let limits = project.ap_recovery_limits;
        let positive = |limit: Option<u32>| limit.filter(|value| *value > 0);
        Self {
            project_id: project.id.clone(),
            rank_up_quest: None,
            interlude: None,
            mystic_code_id: project.mystic_code_id,
            party_order: None,
            support_class_filter: None,
            support_class_filter_mode: Default::default(),
            support_servant_name: None,
            support_servant_id: project.support_servant_id,
            support_servant_variant_key: project.support_servant_variant_key.clone(),
            support_slot_index: support.map(|(index, _)| index as u32),
            support_member_id: support_slot.map(|slot| slot.id.clone()),
            support_craft_essence_id: support_slot.and_then(|slot| slot.craft_essence_id),
            support_craft_essence_ids: support_slot
                .map(|slot| slot.craft_essence_ids.clone())
                .unwrap_or_default(),
            support_craft_essence_mlb_required: support_slot
                .is_none_or(|slot| slot.craft_essence_mlb_required),
            support_grand_mode: project.support_grand_mode,
            support_grand_craft_essence_ids: project.support_grand_craft_essence_ids,
            support_grand_craft_essence_id_lists: project
                .support_grand_craft_essence_id_lists
                .clone(),
            support_grand_craft_essence_mlb_required: project
                .support_grand_craft_essence_mlb_required,
            support_grand_bond_ce_mode: match project.support_grand_bond_ce_mode {
                crate::SupportGrandBondCeMode::Any => SupportGrandBondCeMode::Any,
                crate::SupportGrandBondCeMode::Bond => SupportGrandBondCeMode::Bond,
                crate::SupportGrandBondCeMode::BondNp => SupportGrandBondCeMode::BondNp,
            },
            grand_servants: project.grand_servants.clone(),
            grand_class: project.grand_class,
            grand_card_strategy: project.grand_card_strategy.clone(),
            support_servant_level_min: project.support_servant_level_min,
            support_noble_phantasm_level_min: project.support_noble_phantasm_level_min,
            support_star_map_score_min: project.support_star_map_score_min,
            support_grand_star_map_score_min: project.support_grand_star_map_score_min,
            support_skill_level_mins: project.support_skill_level_mins,
            support_append_skill_level_mins: project.support_append_skill_level_mins,
            servant_selections: project
                .slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.kind == "servant")
                .filter_map(|(index, slot)| {
                    Some(ServantSlotConfig {
                        member_id: Some(slot.id.clone()),
                        slot_index: index as u32,
                        servant_id: slot.servant_id?,
                    })
                })
                .collect(),
            max_support_scrolls: 3,
            repeat_mission,
            max_mission_runs,
            ap_recovery_items: project.ap_recovery_items.clone(),
            ap_recovery_limits: ApRecoveryLimits {
                rainbow: positive(limits.rainbow),
                gold: positive(limits.gold),
                silver: positive(limits.silver),
                bronze: positive(limits.bronze),
                copper: positive(limits.copper),
            },
            stop_on_five_star_ce_drop,
            five_star_ce_drop_target_count: if stop_on_five_star_ce_drop {
                recognition
                    .five_star_ce_drop_target_count
                    .unwrap_or(1)
                    .max(1)
            } else {
                1
            },
            prefer_higher_critical_chance: false,
            // App and project recognition settings are applied at runner startup.
            enable_extra_class_filter: true,
            support_full_list_ocr_fallback: false,
            support_ce_threshold: default_support_ce_threshold(),
            support_ce_full_gate_threshold: default_support_ce_full_gate_threshold(),
            support_mlb_icon_threshold: default_support_icon_threshold(),
            support_bond_icon_threshold: default_support_icon_threshold(),
            noble_phantasm_detection_mode: Default::default(),
            stop_on_bond_level_up: false,
            stop_on_bond_max_level: false,
            auto_capture_bond_level_up: false,
            verify_skill_activation: false,
            unknown_screen_timeout_count: default_unknown_screen_timeout_count(),
            ap_recovery_confirm_wait_ms: default_ap_recovery_confirm_wait_ms(),
            auto_capture_battle_result_loot: false,
            auto_capture_battle_before_attack: false,
            auto_friend_request: false,
            auto_capture_skill_use_probe: false,
            auto_capture_unrecognized_critical_chance: false,
        }
    }
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

fn default_ap_recovery_confirm_wait_ms() -> u32 {
    crate::commands::settings::AP_RECOVERY_CONFIRM_WAIT_MS_DEFAULT
}

fn default_unknown_screen_timeout_count() -> u32 {
    crate::commands::settings::UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT
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

fn default_support_grand_craft_essence_id_lists() -> [Vec<u32>; 3] {
    std::array::from_fn(|_| Vec::new())
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum RunnerState {
    Idle,
    Starting,
    Running,
    Paused,
    Finished,
    Error { message: String },
}

impl RunnerState {
    pub(crate) fn status(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Finished => "finished",
            Self::Error { .. } => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunnerLifecycleEvent {
    WorkerStarted,
    VerificationBlocked,
    VerificationResolved,
    StopRequested,
    Finished,
    Failed { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunnerLifecycleTransition {
    pub(crate) previous: RunnerState,
    pub(crate) event: RunnerLifecycleEvent,
    pub(crate) next: RunnerState,
    pub(crate) accepted: bool,
}

pub(crate) fn runner_lifecycle_transition(
    state: RunnerState,
    event: RunnerLifecycleEvent,
) -> RunnerLifecycleTransition {
    let next = match (&state, &event) {
        (RunnerState::Starting, RunnerLifecycleEvent::WorkerStarted) => Some(RunnerState::Running),
        (
            RunnerState::Starting | RunnerState::Running | RunnerState::Paused,
            RunnerLifecycleEvent::StopRequested,
        ) => Some(RunnerState::Idle),
        (RunnerState::Running, RunnerLifecycleEvent::VerificationBlocked) => {
            Some(RunnerState::Paused)
        }
        (RunnerState::Paused, RunnerLifecycleEvent::VerificationResolved) => {
            Some(RunnerState::Running)
        }
        (RunnerState::Running, RunnerLifecycleEvent::Finished) => Some(RunnerState::Finished),
        (_, RunnerLifecycleEvent::Failed { message }) => Some(RunnerState::Error {
            message: message.clone(),
        }),
        _ => None,
    };

    let accepted = next.is_some();
    RunnerLifecycleTransition {
        previous: state.clone(),
        event,
        next: next.unwrap_or(state),
        accepted,
    }
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
    pub status: &'static str,
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
    pub is_stunned: bool,
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

/// State and controls shared with a single battle worker.
pub(crate) struct BattleRunHandle {
    pub(crate) team_verification: Arc<Mutex<TeamVerificationControl>>,
    pub(crate) state: Arc<Mutex<RunnerState>>,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) stop_after_current: Arc<AtomicBool>,
    pub(crate) progress: Arc<Mutex<BattleRunProgressEvent>>,
}

impl BattleRunHandle {
    pub(crate) fn new(run_id: uuid::Uuid, state: RunnerState, max_runs: Option<u32>) -> Self {
        Self {
            team_verification: Arc::new(Mutex::new(TeamVerificationControl::new(run_id))),
            state: Arc::new(Mutex::new(state)),
            cancel: Arc::new(AtomicBool::new(false)),
            stop_after_current: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(BattleRunProgressEvent {
                max_runs,
                ..Default::default()
            })),
        }
    }
}

/// Battle handles retained in Tauri managed state for the app session.
pub struct RunnerHandle {
    pub(crate) current: Arc<BattleRunHandle>,
    pub(crate) latest_mcp: Option<Arc<BattleRunHandle>>,
}

impl RunnerHandle {
    pub fn new_idle() -> Self {
        Self {
            current: Arc::new(BattleRunHandle::new(
                uuid::Uuid::new_v4(),
                RunnerState::Idle,
                None,
            )),
            latest_mcp: None,
        }
    }

    pub(crate) fn replace_current(&mut self, run: Arc<BattleRunHandle>, started_by_mcp: bool) {
        if started_by_mcp {
            self.latest_mcp = Some(run.clone());
        }
        self.current = run;
    }
}
