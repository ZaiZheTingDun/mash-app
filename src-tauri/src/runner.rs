use crate::adb::Adb;
use crate::screen::{
    CommandCardMatch, NoblePhantasmMatch, NormRect, Point, Screen, SidecarClient,
    SupportCeArtworkCheck, SupportCeVerificationOptions, SupportRowMatch,
};
use crate::touch::{self, TouchBackend};
use crate::{
    load_servant_metadata, servant_np_card, Action, AdvancedBattleScene,
    AdvancedCommandCardCondition, AdvancedOutputType, AdvancedRule, AttackCard, BattleScene,
    ServantMetadata, Server,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use tauri::Emitter;

// ---------------------------------------------------------------------------
// Configuration (received from frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServantSlotConfig {
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

fn default_grand_chain_priority() -> Vec<GrandChainPriorityItem> {
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
}

impl Default for GrandCardStrategy {
    fn default() -> Self {
        Self {
            chain_priority: default_grand_chain_priority(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrandServantConfig {
    pub slot_index: u32,
    #[serde(default = "default_grand_np_card")]
    pub np_card: String,
    #[serde(default = "default_grand_card_priority")]
    pub priority: String,
}

#[derive(Debug, Clone)]
struct GrandServantRuntimeConfig {
    slot_index: usize,
    servant_id: u32,
    np_card: String,
    priority: String,
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
    /// Craft-essence id pinned via the team-builder support CE slot.
    /// When `Some`, `handle_support_select` runs `verify_support_ce`
    /// against each OCR-detected row and picks the first row whose CE
    /// matches the template at `assets/ces/{id}/card_ce.png`. `None`
    /// (or a missing template) preserves legacy behaviour: pick the
    /// first OCR match.
    #[serde(default)]
    pub support_craft_essence_id: Option<u32>,
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

fn default_true() -> bool {
    true
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
    Running,
    Finished,
    Error { message: String },
}

/// Severity of an automation status log entry. `Info` is the normal,
/// user-facing channel — every action the runner takes, every screen it
/// transitions through. `Debug` is reserved for technical diagnostics
/// the operator usually doesn't need to see (e.g. raw CV anchor
/// coordinates, computed swipe distances) but that are valuable when
/// triaging a bug report. The frontend filters out `Debug` entries by
/// default and exposes a toggle for power users.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Info,
    Debug,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationEvent {
    pub state: String,
    pub current_screen: String,
    pub message: String,
    pub level: LogLevel,
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

// ---------------------------------------------------------------------------
// Placeholder positions (normalized 0..1, to be configured later)
// ---------------------------------------------------------------------------

/// Servant skill buttons: [servant_index][skill_index]
/// servant_1 = index 0, servant_2 = index 1, servant_3 = index 2
/// skill_1 = index 0, skill_2 = index 1, skill_3 = index 2
const SERVANT_SKILLS: [[Point; 3]; 3] = [
    [
        Point::new(0.058, 0.807),
        Point::new(0.127, 0.807),
        Point::new(0.196, 0.807),
    ],
    [
        Point::new(0.305, 0.807),
        Point::new(0.374, 0.807),
        Point::new(0.443, 0.807),
    ],
    [
        Point::new(0.553, 0.807),
        Point::new(0.622, 0.807),
        Point::new(0.691, 0.807),
    ],
];

const EQUIPMENT_BUTTON: Point = Point::new(0.933, 0.434);

/// Master / equipment skill buttons
const EQUIPMENT_SKILLS: [Point; 3] = [
    Point::new(0.708, 0.436),
    Point::new(0.777, 0.436),
    Point::new(0.848, 0.436),
];

/// Attack button position on the battle screen.
pub const ATTACK_BUTTON: Point = Point::new(0.887, 0.844);

/// Return button on the attack-card screen, used after advanced-mode card
/// inspection when the runner needs to go back to Battle and run skills.
const ATTACK_SCREEN_RETURN: Point = Point::new(0.938, 0.947);

/// Tap target that, when pressed during a skill / NP animation, makes the
/// game skip ahead to the next actionable frame. Same physical button
/// works after every skill on the battle screen.
const SKIP_ANIMATION_BUTTON: Point = Point::new(0.685, 0.095);

const BATTLE_SCREEN: &str = "Battle";
const SUPPORT_SELECT_SCREEN: &str = "SupportSelect";
pub const ATTACK_BUTTON_ELEMENT: &str = "attack_button";
const SUPPORT_SCROLL_END_ELEMENT: &str = "support_scroll_end";
/// Party servant auto-placement is reserved for a later implementation.
/// Keep the config shape intact, but do not enter ServantSelect from
/// TeamConfirm yet.
const ENABLE_PARTY_SERVANT_AUTO_PLACEMENT: bool = false;

/// Region of the top-right `BATTLE m/n` HUD strip. The CV sidecar
/// anchors on the gold `BATTLE` label inside this region and reads
/// the `(m, n)` digit pair to its right; `m` drives which configured
/// `BattleScene` block runs this iteration.
pub const BATTLE_SCENE_REGION: NormRect = NormRect {
    x: 0.587,
    y: 0.000,
    w: 0.160,
    h: 0.062,
};

/// Ally target positions for skill targeting (servant_1, servant_2, servant_3)
const SKILL_TARGETS: [Point; 3] = [
    Point::new(0.254, 0.474),
    Point::new(0.499, 0.474),
    Point::new(0.744, 0.474),
];

/// Enemy target positions for attack targeting (enemy_1..enemy_6).
/// Coordinates are normalized from 2560x1440 screenshots.
const ENEMY_TARGETS: [Point; 6] = [
    Point::new(0.11015625, 0.04583333333333333),
    Point::new(0.26640625, 0.04583333333333333),
    Point::new(0.42265625, 0.04583333333333333),
    Point::new(0.033203125, 0.18263888888888888),
    Point::new(0.189453125, 0.18263888888888888),
    Point::new(0.345703125, 0.18263888888888888),
];

/// Command card positions on the attack screen (5 cards left to right)
const COMMAND_CARDS: [Point; 5] = [
    Point::new(0.097, 0.678),
    Point::new(0.303, 0.678),
    Point::new(0.504, 0.678),
    Point::new(0.705, 0.678),
    Point::new(0.907, 0.678),
];

const NOBLE_PHANTASMS: [Point; 3] = [
    Point::new(0.319, 0.242),
    Point::new(0.497, 0.242),
    Point::new(0.680, 0.242),
];

/// Command Spell (令咒) entry button on the battle screen — opens the
/// modal listing the available spells.
const COMMAND_SPELL_BUTTON: Point = Point::new(0.829, 0.113);

/// Spell-option tap targets inside the Command Spell modal
/// (`CommandSpell_open.png`). Indices align with `command_spell_index`:
/// 0 = "宝具解放" (np_release), 1 = "灵基修复" (restore).
const COMMAND_SPELL_OPTIONS: [Point; 2] = [Point::new(0.500, 0.460), Point::new(0.500, 0.690)];

/// "决定" confirm button on the Command Spell confirmation dialog
/// (`command_spell_confirmation.png`). Its mirror "取消" button at
/// roughly (0.340, 0.600) is intentionally not exposed — the runner
/// always confirms.
const COMMAND_SPELL_CONFIRM: Point = Point::new(0.660, 0.600);

/// In-battle Order Change servant slots, left to right on the change screen.
/// Front-line slots are indices 0-2, back-line slots are 3-5.
const ORDER_CHANGE_SLOTS: [Point; 6] = [
    Point::new(0.107, 0.486),
    Point::new(0.264, 0.486),
    Point::new(0.420, 0.486),
    Point::new(0.576, 0.486),
    Point::new(0.732, 0.486),
    Point::new(0.888, 0.486),
];

const ORDER_CHANGE_CONFIRM: Point = Point::new(0.500, 0.872);

/// Settle time between taps in the Command Spell dialog stack. Each tap
/// pops or pushes a full-screen modal (open dialog → confirmation →
/// target picker), so we wait noticeably longer than `ACTION_DELAY`
/// (which is sized for in-place taps on the battle screen).
const COMMAND_SPELL_DIALOG_SETTLE: Duration = Duration::from_millis(600);

// ---------------------------------------------------------------------------
// Battle-result tap targets. Each post-battle page has a single forward
// button; constants are kept here so the debug page (and future overlay)
// can introspect them without crawling the match arm.
// ---------------------------------------------------------------------------

/// "Next" arrow on the bond-points result page.
const BATTLE_RESULT_BOND_NEXT: Point = Point::new(0.041, 0.945);
/// "Next" arrow on the EXP-gain result page (same physical button as bond).
const BATTLE_RESULT_EXP_NEXT: Point = Point::new(0.041, 0.945);
/// "Next" button on the loot/drops summary page.
const BATTLE_RESULT_LOOT_NEXT: Point = Point::new(0.874, 0.890);
/// "Skip / Close" on the optional friend-request prompt that appears
/// after using a non-friend support.
const BATTLE_RESULT_FRIEND_SKIP: Point = Point::new(0.254, 0.854);
/// "Continue / Repeat" button on the final continue page — taps this when
/// `RunConfig::repeat_mission` is true.
const BATTLE_RESULT_CONTINUE_REPEAT: Point = Point::new(0.657, 0.809);
/// "Close / Stop" button on the final continue page — taps this when
/// `RunConfig::repeat_mission` is false. The runner finishes after.
const BATTLE_RESULT_CONTINUE_STOP: Point = Point::new(0.348, 0.809);

const AP_RECOVERY_ITEMS_REGION: NormRect = NormRect {
    x: 0.244,
    y: 0.142,
    w: 0.095,
    h: 0.659,
};
const AP_RECOVERY_SCROLL_FROM: Point = Point::new(0.780, 0.166);
const AP_RECOVERY_SCROLL_TO: Point = Point::new(0.780, 0.426);
const AP_RECOVERY_CONFIRM_BUTTON: Point = Point::new(0.663, 0.795);
const AP_RECOVERY_LIST_LABEL_TEMPLATE: &str = "items/label_item";
const AP_RECOVERY_ITEM_THRESHOLD: f64 = 0.82;

/// Cadence used by `tap_until_screen_changes` when dismissing post-battle
/// result pages. Slow enough for the device to register each tap and for
/// `detect()` to read a fresh frame, fast enough that a 3–5 s bond /
/// EXP animation only absorbs a few wasted taps before the page actually
/// transitions.
const BATTLE_RESULT_TAP_INTERVAL: Duration = Duration::from_millis(300);
/// Hard ceiling for how long any single result page is allowed to absorb
/// taps before the runner emits a timeout warning. Comfortably above the
/// longest measured bond / EXP animation (~5 s for a multi-servant
/// level-up cascade).
const BATTLE_RESULT_TAP_TIMEOUT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// Debug: expose coordinate constants for visualization
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabeledPoint {
    pub label: String,
    pub point: Point,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabeledRegion {
    pub label: String,
    pub region: NormRect,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordGroup {
    pub id: String,
    pub label: String,
    pub points: Vec<LabeledPoint>,
    pub regions: Vec<LabeledRegion>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DebugCoordinates {
    pub groups: Vec<CoordGroup>,
}

/// Snapshot of every `Point` / `NormRect` constant the runner uses, grouped
/// for display in the debug UI.
pub fn debug_coordinates() -> DebugCoordinates {
    let mut servant_skill_points = Vec::with_capacity(9);
    for (si, row) in SERVANT_SKILLS.iter().enumerate() {
        for (ki, p) in row.iter().enumerate() {
            servant_skill_points.push(LabeledPoint {
                label: format!("S{}.{}", si + 1, ki + 1),
                point: *p,
            });
        }
    }

    let mut equipment_points = Vec::with_capacity(4);
    equipment_points.push(LabeledPoint {
        label: "Menu".into(),
        point: EQUIPMENT_BUTTON,
    });
    for (i, p) in EQUIPMENT_SKILLS.iter().enumerate() {
        equipment_points.push(LabeledPoint {
            label: format!("E{}", i + 1),
            point: *p,
        });
    }

    let groups = vec![
        CoordGroup {
            id: "servantSkills".into(),
            label: "从者技能".into(),
            points: servant_skill_points,
            regions: Vec::new(),
        },
        CoordGroup {
            id: "equipment".into(),
            label: "御主技能 / 装备".into(),
            points: equipment_points,
            regions: Vec::new(),
        },
        CoordGroup {
            id: "attack".into(),
            label: "攻击".into(),
            points: vec![LabeledPoint {
                label: "Attack".into(),
                point: ATTACK_BUTTON,
            }],
            regions: Vec::new(),
        },
        CoordGroup {
            id: "battleScene".into(),
            label: "战斗场景".into(),
            points: Vec::new(),
            regions: vec![LabeledRegion {
                label: "BattleSceneRegion".into(),
                region: BATTLE_SCENE_REGION,
            }],
        },
        CoordGroup {
            id: "skillTargets".into(),
            label: "技能目标".into(),
            points: SKILL_TARGETS
                .iter()
                .enumerate()
                .map(|(i, p)| LabeledPoint {
                    label: format!("Ally{}", i + 1),
                    point: *p,
                })
                .collect(),
            regions: Vec::new(),
        },
        CoordGroup {
            id: "enemyTargets".into(),
            label: "敌人目标".into(),
            points: ENEMY_TARGETS
                .iter()
                .enumerate()
                .map(|(i, p)| LabeledPoint {
                    label: format!("Enemy{}", i + 1),
                    point: *p,
                })
                .collect(),
            regions: Vec::new(),
        },
        CoordGroup {
            id: "commandCards".into(),
            label: "指令卡".into(),
            points: COMMAND_CARDS
                .iter()
                .enumerate()
                .map(|(i, p)| LabeledPoint {
                    label: format!("C{}", i + 1),
                    point: *p,
                })
                .collect(),
            regions: Vec::new(),
        },
        CoordGroup {
            id: "noblePhantasms".into(),
            label: "宝具".into(),
            points: NOBLE_PHANTASMS
                .iter()
                .enumerate()
                .map(|(i, p)| LabeledPoint {
                    label: format!("NP{}", i + 1),
                    point: *p,
                })
                .collect(),
            regions: Vec::new(),
        },
        CoordGroup {
            id: "commandSpell".into(),
            label: "令咒".into(),
            points: vec![
                LabeledPoint {
                    label: "Open".into(),
                    point: COMMAND_SPELL_BUTTON,
                },
                LabeledPoint {
                    label: "宝具解放".into(),
                    point: COMMAND_SPELL_OPTIONS[0],
                },
                LabeledPoint {
                    label: "灵基修复".into(),
                    point: COMMAND_SPELL_OPTIONS[1],
                },
                LabeledPoint {
                    label: "决定".into(),
                    point: COMMAND_SPELL_CONFIRM,
                },
            ],
            regions: Vec::new(),
        },
        CoordGroup {
            id: "supportSelect".into(),
            label: "助战选择".into(),
            points: vec![
                LabeledPoint {
                    label: "Saber".into(),
                    point: SUPPORT_TAB_SABER,
                },
                LabeledPoint {
                    label: "Archer".into(),
                    point: SUPPORT_TAB_ARCHER,
                },
                LabeledPoint {
                    label: "Lancer".into(),
                    point: SUPPORT_TAB_LANCER,
                },
                LabeledPoint {
                    label: "Rider".into(),
                    point: SUPPORT_TAB_RIDER,
                },
                LabeledPoint {
                    label: "Caster".into(),
                    point: SUPPORT_TAB_CASTER,
                },
                LabeledPoint {
                    label: "Assassin".into(),
                    point: SUPPORT_TAB_ASSASSIN,
                },
                LabeledPoint {
                    label: "Berserker".into(),
                    point: SUPPORT_TAB_BERSERKER,
                },
                LabeledPoint {
                    label: "Extra".into(),
                    point: SUPPORT_TAB_EXTRA,
                },
                LabeledPoint {
                    label: "Refresh".into(),
                    point: SUPPORT_REFRESH_BUTTON,
                },
                LabeledPoint {
                    label: "Skill Panel Toggle".into(),
                    point: SUPPORT_SKILL_PANEL_TOGGLE_BUTTON,
                },
            ],
            regions: Vec::new(),
        },
    ];

    DebugCoordinates { groups }
}

// ---------------------------------------------------------------------------
// Battle state
// ---------------------------------------------------------------------------

struct BattleState {
    /// Which battle-scene config index we're executing (0-based into the
    /// `scenes` vec). One config block = one battle scene as labeled in
    /// the HUD's `BATTLE m/n`.
    current_scene_index: usize,
    /// `m` value last successfully detected from the BATTLE m/n HUD strip.
    /// Stays `Some(prev)` across transient failed reads (e.g. NP overlay
    /// briefly covers the strip) so we don't double-trigger skill
    /// execution when the strip reappears.
    last_screen_scene: Option<u32>,
    /// `current_scene_index` value we last executed skills for. We
    /// re-run the configured skills exactly once per index value, so
    /// transient failed reads (`scene_m == None`) never cause duplicate
    /// execution — only an actual `Some(prev) → Some(curr != prev)`
    /// transition advances the index and triggers a re-execution.
    executed_scene_index: Option<usize>,
    /// Whether we used the scene config (vs fallback) — drives card selection
    scene_config_used: bool,
    /// Set after clicking start on TeamConfirm; tolerates longer Unknown streaks
    waiting_for_battle: bool,
    /// Set after tapping the selected command cards. The attack-card screen
    /// can remain detectable for a short moment before the animation takes
    /// over; this prevents submitting another set of picks in that window.
    attack_submitted: bool,
    advanced_startup_done: HashSet<usize>,
    advanced_control_indices: HashMap<usize, usize>,
    advanced_startup_control_indices: HashMap<usize, usize>,
    advanced_auto_order_changes: HashMap<usize, Action>,
}

impl BattleState {
    fn new() -> Self {
        Self {
            current_scene_index: 0,
            last_screen_scene: None,
            executed_scene_index: None,
            scene_config_used: false,
            waiting_for_battle: false,
            attack_submitted: false,
            advanced_startup_done: HashSet::new(),
            advanced_control_indices: HashMap::new(),
            advanced_startup_control_indices: HashMap::new(),
            advanced_auto_order_changes: HashMap::new(),
        }
    }
}

/// Outcome of merging a fresh `BATTLE m/n` reading into the prior scene
/// state. Returned by [`tick_scene_state`] so the decision logic
/// (advance? lock in? re-execute?) is testable independently of the
/// runner's I/O side effects (taps, sidecar IPC, emitted events).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct SceneTick {
    /// New value for `BattleState::last_screen_scene`. Preserves the
    /// prior `Some(prev)` across transient `None` reads.
    last_screen_scene: Option<u32>,
    /// New value for `BattleState::current_scene_index`. Increments
    /// only on an actual `Some(prev) → Some(curr != prev)` transition.
    current_scene_index: usize,
    /// True iff the configured skills for `current_scene_index` should
    /// be executed this iteration. False on every iteration where the
    /// caller has already executed for the same index value, including
    /// when the latest CV read failed and we're sitting on a previously
    /// locked-in scene.
    needs_exec: bool,
}

fn tick_scene_state(
    last_screen_scene: Option<u32>,
    current_scene_index: usize,
    executed_scene_index: Option<usize>,
    scene_m: Option<u32>,
) -> SceneTick {
    // Three update paths for `current_scene_index`:
    //
    // 1. First successful read (no prior `last_screen_scene`): snap the
    //    index to `scene_m - 1` so the runner aligns with whatever
    //    scene the screen is actually on. This handles the user
    //    starting the runner mid-quest (e.g. screen already shows 2/3
    //    on the first poll) — without the snap we would execute
    //    config block 0 for the actual scene 2 and only advance on the
    //    *next* observed transition.
    // 2. Subsequent transition (`prev → curr` with both Some and
    //    different): increment the index by 1, mirroring the on-screen
    //    advance.
    // 3. Anything else (failed read, same `m` re-read, no read yet):
    //    leave the index alone.
    let next_index = match (last_screen_scene, scene_m) {
        (None, Some(curr)) => curr.saturating_sub(1) as usize,
        (Some(prev), Some(curr)) if prev != curr => current_scene_index + 1,
        _ => current_scene_index,
    };
    let next_last = if scene_m.is_some() {
        scene_m
    } else {
        last_screen_scene
    };
    SceneTick {
        last_screen_scene: next_last,
        current_scene_index: next_index,
        needs_exec: executed_scene_index != Some(next_index),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApRecoveryPage {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ApRecoveryTemplate {
    item: ApRecoveryItem,
    page: ApRecoveryPage,
    label: &'static str,
    template_key: &'static str,
}

fn ap_recovery_template(item: ApRecoveryItem) -> ApRecoveryTemplate {
    match item {
        // Frontend `Rainbow` maps to the premium Saint Quartz recovery option.
        ApRecoveryItem::Rainbow => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Top,
            label: "圣晶石",
            template_key: "items/item_saint_quartz",
        },
        ApRecoveryItem::Gold => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Top,
            label: "黄金苹果",
            template_key: "items/item_apple_gold",
        },
        ApRecoveryItem::Silver => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Top,
            label: "白银苹果",
            template_key: "items/item_apple_silver",
        },
        ApRecoveryItem::Bronze => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Bottom,
            label: "青铜苹果",
            template_key: "items/item_apple_bronzed_cobalt",
        },
        ApRecoveryItem::Copper => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Bottom,
            label: "赤铜苹果",
            template_key: "items/item_apple_bronze",
        },
    }
}

fn ap_recovery_candidates_for_page(
    configured: &[ApRecoveryItem],
    page: ApRecoveryPage,
) -> Vec<ApRecoveryTemplate> {
    [
        ApRecoveryItem::Gold,
        ApRecoveryItem::Silver,
        ApRecoveryItem::Bronze,
        ApRecoveryItem::Copper,
        ApRecoveryItem::Rainbow,
    ]
    .into_iter()
    .filter(|item| configured.contains(item))
    .map(ap_recovery_template)
    .filter(|template| template.page == page)
    .collect()
}

fn is_unknown_element_error(err: &str, screen: &str, element: &str) -> bool {
    err.contains(&format!("unknown element: {screen}.{element}"))
}

/// Compute the y-delta a support-list scroll swipe should travel so
/// the *lowest visible row* on the current page ends up at the same
/// Compute the support-list scroll distance from the last visible
/// confirm-button anchor only. The goal is simple and observable:
/// move the bottom-most detected button to the first-row button y
/// (`SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y`). We deliberately do not
/// extrapolate hidden/partial rows from the visible row pitch because
/// that can skip a servant that is only partially visible at the bottom.
fn scroll_support_list_delta(confirm_button_anchors: &[NormRect]) -> f64 {
    let Some(bottom_y) = confirm_button_anchors
        .iter()
        .map(|anchor| anchor.y)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    else {
        return SUPPORT_SCROLL_FALLBACK_DELTA;
    };
    (bottom_y - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y)
        .clamp(SUPPORT_SCROLL_MIN_DELTA, SUPPORT_SCROLL_MAX_DELTA)
}

/// Pick the active-motion (MOVE-phase) duration in ms for a settle
/// swipe. The actual lift-off velocity is governed by the trailing
/// `SUPPORT_SCROLL_SETTLE_MS` hold inside `Adb::swipe_with_settle`, so
/// here we only need to pick a duration that produces visually smooth
/// motion (not too jumpy on long deltas, not too long on tiny ones).
/// `SUPPORT_SCROLL_VELOCITY` is interpreted as normalized screen units
/// per second of *active* motion; with no fling to worry about, we can
/// run this much faster than the old all-linear swipe needed to. The
/// total realized swipe time is roughly
/// `scroll_support_list_duration_ms(delta) + SUPPORT_SCROLL_SETTLE_MS`
/// plus per-event ADB overhead.
fn scroll_support_list_duration_ms(delta: f64) -> u32 {
    let raw_ms = (delta.abs() / SUPPORT_SCROLL_VELOCITY * 1000.0).round();
    let raw_ms = raw_ms.clamp(0.0, u32::MAX as f64) as u32;
    raw_ms.clamp(SUPPORT_SCROLL_MIN_DURATION_MS, SUPPORT_SCROLL_MAX_DURATION_MS)
}

/// Render a one-line, human-readable summary of a support-list scroll
/// decision for the debug log. Kept compact (single line, three digits
/// of precision) so the operation-log panel stays readable when many
/// scrolls scroll past in a row.
fn format_scroll_debug(
    confirm_button_anchors: &[NormRect],
    delta: f64,
    from_y: f64,
    to_y: f64,
    swipe_ms: u32,
    settle_ms: u32,
) -> String {
    let mut anchor_ys: Vec<f64> = confirm_button_anchors.iter().map(|a| a.y).collect();
    anchor_ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let anchor_list = if anchor_ys.is_empty() {
        "无".to_string()
    } else {
        anchor_ys
            .iter()
            .map(|y| format!("{y:.3}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "滚动助战列表: 按钮 y=[{anchor_list}] (n={}) Δ={delta:.3} swipe={from_y:.2}→{to_y:.2} ({swipe_ms}ms+{settle_ms}ms settle)",
        confirm_button_anchors.len(),
    )
}

fn support_grand_section_exhausted_after_probe(
    visible: Option<bool>,
    seen: &mut bool,
    consecutive_misses: &mut u8,
) -> bool {
    match visible {
        Some(true) => {
            *seen = true;
            *consecutive_misses = 0;
            false
        }
        Some(false) if *seen => {
            *consecutive_misses = consecutive_misses.saturating_add(1);
            *consecutive_misses >= 2
        }
        Some(false) => false,
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const ACTION_DELAY: Duration = Duration::from_millis(300);

/// Hard cap on friend-list refreshes inside `handle_support_select`. After
/// this many refreshes (each preceded by a full scroll cycle) without
/// finding the pinned servant, the runner aborts with an error so the user
/// isn't stuck looping forever on a servant that simply isn't available.
const SUPPORT_MAX_REFRESHES: u32 = 99;
/// Settle time after the support-list scroll swipe completes, before
/// the next OCR pass. The settle hold *inside* `swipe_with_settle`
/// already lifts the finger at near-zero velocity (see
/// `SUPPORT_SCROLL_SETTLE_MS`), so the list isn't bouncing or flinging
/// when we wake up — this is just the buffer for the row cards to
/// re-render at their new positions. Empirically the redraw completes
/// in well under 300 ms on real devices; 450 ms keeps comfortable
/// headroom for slower emulators / a low-end CPU without pinning the
/// runner at the old "wait nearly a second between every scroll"
/// cadence (was 900 ms before the fling-avoiding settle gesture made
/// the longer wait redundant).
const SUPPORT_SCROLL_SETTLE: Duration = Duration::from_millis(450);
/// The y from which the scroll swipe starts (finger-down point). Sits in
/// the lower half of the list so the symmetric `to` point can always
/// stay above it for an "up" swipe even at `SUPPORT_SCROLL_MAX_DELTA`.
const SUPPORT_SCROLL_FROM_Y: f64 = 0.78;
/// Target y for the bottom-most detected support confirm button after
/// a scroll. This is the first-row button position on the support list.
const SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y: f64 = 0.30;
/// Minimum scroll delta. Guarantees forward progress when the bottom-most
/// detected confirm button is already near the first-row target.
const SUPPORT_SCROLL_MIN_DELTA: f64 = 0.10;
/// Maximum scroll delta. Caps the swipe so a bogus anchor near the very
/// bottom of the screen can't fling the list past several pages in one go.
const SUPPORT_SCROLL_MAX_DELTA: f64 = 0.65;
/// Legacy fixed scroll delta — used only when no confirm-button anchors
/// were detected so we still make progress on devices / resolutions
/// where the template / shape detector misses the button column.
const SUPPORT_SCROLL_FALLBACK_DELTA: f64 = 0.40;
/// Normalized screen units (fraction of screen height) per second
/// for the *active* motion phase of the settle swipe. The
/// fling-protection settle hold (see `SUPPORT_SCROLL_SETTLE_MS`)
/// decouples lift-off velocity from this constant, so we can pick a
/// value purely for visual / cycle-time reasons — it does NOT need
/// to stay under Android's per-device fling threshold the way a
/// plain `input swipe` would.
///
/// 2.0 norm/s ≈ two full screen heights per second; for the
/// canonical Δ≈0.555 support-list scroll that's ~278 ms of motion +
/// the ~250 ms settle hold ≈ ~530 ms total active gesture. The 50 Hz
/// MOVE cadence is preserved at this velocity because
/// `settle_swipe_move_steps` scales the step count with `swipe_ms`
/// (≈14 MOVE events at 278 ms), so the active phase stays visibly
/// smooth instead of degenerating into a few jumpy steps.
const SUPPORT_SCROLL_VELOCITY: f64 = 2.0;
/// Lower bound on the swipe duration so a tiny min-delta scroll still
/// reads as a deliberate gesture to the touch dispatcher.
const SUPPORT_SCROLL_MIN_DURATION_MS: u32 = 250;
/// Upper bound on the swipe duration so a pathologically large delta
/// can't stall the runner with a multi-second swipe.
const SUPPORT_SCROLL_MAX_DURATION_MS: u32 = 2000;

/// How long the finger holds at the destination before lifting off in a
/// settle-style support-list scroll. Must exceed Android's velocity
/// tracker sliding window (~100 ms on most devices) so the tracker
/// sees a stretch of "no motion" right before UP and reports ~0 px/s.
/// 250 ms gives ~2.5× the velocity-tracker window on modern Android
/// (the window shortened from ~100 ms on older versions to ~80 ms on
/// 12+), comfortably above the fling threshold while shaving 150 ms
/// off each scroll cycle vs. the original 400 ms — empirically the
/// list still stops dead at the lift point on the devices we've
/// tested. Bump back up if a future device leaks a fling.
const SUPPORT_SCROLL_SETTLE_MS: u32 = 250;
/// Settle time after tapping the "refresh friend list" button. The friend
/// list refetch and re-render takes ~2.5s on slow devices; one extra second
/// of buffer keeps us from OCRing a half-loaded list.
const SUPPORT_REFRESH_SETTLE: Duration = Duration::from_secs(3);
/// Small timeout for the refresh-confirm dialog to animate in after tapping
/// the support refresh button.
const SUPPORT_REFRESH_DIALOG_APPEAR_TIMEOUT: Duration = Duration::from_secs(2);
/// Poll cadence while waiting for the refresh-confirm dialog to appear or
/// disappear.
const SUPPORT_REFRESH_DIALOG_POLL: Duration = Duration::from_millis(300);
/// Maximum wait for the refresh-confirm dialog to close after tapping OK.
const SUPPORT_REFRESH_DIALOG_DISMISS_TIMEOUT: Duration = Duration::from_secs(8);
/// Extra pause after the refresh-confirm dialog is first seen, before we tap
/// the confirm button. The detect template can match while the modal is
/// still mid-fade-in and the button hit-area isn't fully interactive yet;
/// this short settle keeps taps from being eaten by the animation.
const SUPPORT_REFRESH_DIALOG_CONFIRM_SETTLE: Duration = Duration::from_millis(500);

/// Refresh-friend-list button on the support-select screen, captured from
/// a 2560x1440 landscape device. Calibrated alongside the class-tab strip
/// (same row, x further right).
const SUPPORT_REFRESH_BUTTON: Point = Point::new(0.726, 0.178);
/// Confirm button inside the support refresh dialog.
const SUPPORT_REFRESH_CONFIRM_BUTTON: Point = Point::new(0.650, 0.779);
const SUPPORT_REFRESH_DIALOG_ELEMENT: &str = "dialog_refresh_support";

/// "技能显示切换" toggle on the support-select screen — the button that
/// cycles which skill panel (owned vs append) is shown for every support
/// row. Three-state cycle: 固定持有 → 固定追加 → 间隔切换 → … The runner
/// can't tell which mode the user has the game in (the icon variants
/// don't ship as templates), but we don't need to: each tap advances
/// the cycle, so worst case 3 taps will surface every panel layout.
/// Sits immediately to the left of `SUPPORT_REFRESH_BUTTON` in the same
/// settings row, so it shares y with the class-tab strip.
const SUPPORT_SKILL_PANEL_TOGGLE_BUTTON: Point = Point::new(0.658, 0.178);
/// Settle time after tapping the panel toggle: long enough for the
/// support-list rows to redraw their skill icons before the next OCR
/// pass. Shorter than `SUPPORT_SCROLL_SETTLE` because no list reflow
/// happens — only the per-row icon swap.
const SUPPORT_SKILL_PANEL_TOGGLE_SETTLE: Duration = Duration::from_millis(800);
/// Cap on how many times we'll tap the toggle for a single candidate
/// row before giving up on it. The cycle is length 3 plus we may need
/// to ride out the auto-switching mode's flip, so 3 attempts is the
/// minimum that's guaranteed to expose every panel layout in every
/// starting state.
const SUPPORT_SKILL_PANEL_MAX_TOGGLE_TAPS: u32 = 3;

/// Class-filter tab bar across the top of the support-select screen. All
/// tabs share the same y. Order mirrors the FGO UI: all → saber → ...
/// → berserker → extra → mix. Lookup happens via `class_tab_for` which
/// maps Atlas Academy `className` strings into one of these tabs.
const SUPPORT_CLASS_TAB_Y: f64 = 0.178;
const SUPPORT_TAB_SABER: Point = Point::new(0.1246, SUPPORT_CLASS_TAB_Y);
const SUPPORT_TAB_ARCHER: Point = Point::new(0.1773, SUPPORT_CLASS_TAB_Y);
const SUPPORT_TAB_LANCER: Point = Point::new(0.2301, SUPPORT_CLASS_TAB_Y);
const SUPPORT_TAB_RIDER: Point = Point::new(0.2828, SUPPORT_CLASS_TAB_Y);
const SUPPORT_TAB_CASTER: Point = Point::new(0.3355, SUPPORT_CLASS_TAB_Y);
const SUPPORT_TAB_ASSASSIN: Point = Point::new(0.3883, SUPPORT_CLASS_TAB_Y);
const SUPPORT_TAB_BERSERKER: Point = Point::new(0.4410, SUPPORT_CLASS_TAB_Y);
const SUPPORT_TAB_EXTRA: Point = Point::new(0.4938, SUPPORT_CLASS_TAB_Y);

/// Settle time after tapping a class tab. The list animates a quick fade
/// when filtering; ~600ms is enough for the new rows to render before we
/// kick off the OCR pass.
const SUPPORT_CLASS_TAB_SETTLE: Duration = Duration::from_millis(600);

/// Search window for the support row's craft-essence icon, expressed as
/// fractions of the row bbox. The OCR-derived `row_region` only covers
/// the name + NP text strip (anchored to `SUPPORT_LIST_REGION.x`/`.w`);
/// the CE icon overlay actually sits on the **face card to the left of
/// the row**, so `x` is negative on purpose to push the search window
/// outside the row's left edge. `h > 1.0` lets the window span the
/// face vertically (the face is taller than the text strip).
///
/// Reference screenshot: `tests/test_data/screenshots/support_select.png`
/// (1024×576). row_region for row 1: `x≈0.177, w≈0.466, y≈0.30, h≈0.13`.
/// With the values below the search window resolves to roughly
/// `x≈0.05, w≈0.10, y≈0.39, h≈0.13` — bottom of the face card. Retune
/// via the debug page (`助战识别` panel) against real captures.
pub const SUPPORT_CE_OFFSET_IN_ROW: NormRect = NormRect {
    x: -0.33,
    y: -1.60,
    w: 0.35,
    h: 3.45,
};
/// Grand support rows show three CE strips in a fixed left-side column.
/// Their vertical position tracks the right-side "助战编队确认" panel in
/// Grand support rows. The panel's top edge is cleaner than the score
/// badge because it has no overflowing numeric text.
pub const SUPPORT_GRAND_CE_X: f64 = 0.172;
pub const SUPPORT_GRAND_CE_W: f64 = 0.124;
pub const SUPPORT_GRAND_CE_H: f64 = 0.064;
pub const SUPPORT_GRAND_CE_GAP: f64 = 0.000;
pub const SUPPORT_GRAND_CE_THIRD_CENTER_FROM_BUTTON_TOP_Y: f64 = 0.180;
pub const SUPPORT_GRAND_CE_THIRD_CENTER_FROM_PANEL_TOP_Y: f64 = 0.220;
/// Minimum `TM_CCOEFF_NORMED` score to accept a row's CE icon as the
/// pinned CE. Conservative on purpose — the CE icons share a lot of dark
/// background pixels so even mismatched CEs score ~0.4-0.5; the matched
/// CE typically scores 0.75+.
pub const SUPPORT_CE_THRESHOLD: f64 = 0.70;

fn format_ce_artwork_checks(checks: &[SupportCeArtworkCheck]) -> String {
    checks
        .iter()
        .map(|check| {
            let marker = if check.selected {
                "*"
            } else if check.passed {
                "✓"
            } else {
                "✗"
            };
            format!(
                "{}:{} {:.3}/{:.2}{}",
                check.region_kind, check.variant, check.score, check.threshold, marker
            )
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SupportLevelFilter {
    Pass,
    /// The visible panel produced an OCR'd level that's below the
    /// configured minimum. The payload is a short human-readable reason
    /// (e.g. `"持有技能 2 ≥ 10（实际 8）"`) so the runner can surface
    /// *which* requirement failed in the UI log instead of just a
    /// generic "等级不匹配".
    Fail(String),
    WaitingForPanel,
}

#[derive(Debug, Default, Clone)]
struct SupportLevelPanelProgress {
    candidate_key: Option<String>,
    owned_met: bool,
    append_met: bool,
    /// How many times we've tapped the "技能显示切换" button while still
    /// trying to verify this candidate's skill panels. Bounded by
    /// [`SUPPORT_SKILL_PANEL_MAX_TOGGLE_TAPS`]; exceeding it makes the
    /// runner give up on this row and continue scrolling. Reset
    /// implicitly whenever `candidate_key` rolls over.
    panel_toggle_taps: u32,
}

/// Pure helper: apply [`SUPPORT_CE_OFFSET_IN_ROW`] (a row-local rect) to
/// `row` (an absolute row bbox) and return the absolute search window for
/// the row's CE icon. Extracted from `Runner::support_ce_search_region`
/// so unit tests can exercise the math directly without needing to build
/// a full `SupportRowMatch`.
pub fn ce_search_region(row: NormRect) -> NormRect {
    NormRect {
        x: row.x + SUPPORT_CE_OFFSET_IN_ROW.x * row.w,
        y: row.y + SUPPORT_CE_OFFSET_IN_ROW.y * row.h,
        w: SUPPORT_CE_OFFSET_IN_ROW.w * row.w,
        h: SUPPORT_CE_OFFSET_IN_ROW.h * row.h,
    }
}

fn grand_ce_search_region_from_third_center(third_center_y: f64, slot: usize) -> Option<NormRect> {
    if slot >= 3 {
        return None;
    }
    let pitch = SUPPORT_GRAND_CE_H + SUPPORT_GRAND_CE_GAP;
    let center_y = third_center_y - (2 - slot) as f64 * pitch;
    Some(NormRect {
        x: SUPPORT_GRAND_CE_X,
        y: center_y - SUPPORT_GRAND_CE_H / 2.0,
        w: SUPPORT_GRAND_CE_W,
        h: SUPPORT_GRAND_CE_H,
    })
}

pub fn grand_ce_search_region(row: &SupportRowMatch, slot: usize) -> Option<NormRect> {
    let third_center_y = row
        .score_anchor
        .as_ref()
        .map(|anchor| {
            let offset = if anchor.h <= 0.075 {
                SUPPORT_GRAND_CE_THIRD_CENTER_FROM_BUTTON_TOP_Y
            } else {
                SUPPORT_GRAND_CE_THIRD_CENTER_FROM_PANEL_TOP_Y
            };
            anchor.y + offset
        })
        .unwrap_or_else(|| {
            let r = row.row_region;
            r.y + r.h + 0.13
        });
    grand_ce_search_region_from_third_center(third_center_y, slot)
}

fn support_level_meets(actual: Option<u32>, required_min: Option<u32>) -> bool {
    match required_min {
        None => true,
        Some(required) => actual.is_some_and(|level| level >= required),
    }
}

/// Find the first slot whose configured minimum isn't satisfied by the
/// OCR'd value, returning `(slot_index, actual_level, required_min)`
/// for the caller to format. Slots whose `required` is `None` are
/// skipped entirely. Returns `None` when every required slot meets its
/// minimum.
fn support_first_level_mismatch(
    actual: &[Option<u32>],
    required: &[Option<u32>],
) -> Option<(usize, Option<u32>, u32)> {
    required
        .iter()
        .enumerate()
        .find_map(|(index, required_min)| {
            let min = (*required_min)?;
            let actual_level = actual.get(index).copied().flatten();
            if actual_level.is_some_and(|level| level >= min) {
                None
            } else {
                Some((index, actual_level, min))
            }
        })
}

/// Format an OCR'd level for log output. `None` becomes a plain `-` so
/// "the value wasn't read" reads distinctly from "the value was zero".
fn format_actual_level(actual: Option<u32>) -> String {
    actual.map(|v| v.to_string()).unwrap_or_else(|| "-".into())
}

fn support_level_candidate_key(row: &SupportRowMatch) -> String {
    format!(
        "{}|{}|{:.3}",
        row.name_text, row.np_matched_name, row.row_region.y
    )
}

fn support_row_matches_level_requirements_with_progress(
    server: Server,
    config: &RunConfig,
    row: &SupportRowMatch,
    progress: &mut SupportLevelPanelProgress,
) -> SupportLevelFilter {
    let needs_np = config.support_noble_phantasm_level_min.is_some();
    let needs_owned = config.support_skill_level_mins.iter().any(Option::is_some);
    let needs_append = config
        .support_append_skill_level_mins
        .iter()
        .any(Option::is_some);
    if server != Server::Cn || (!needs_np && !needs_owned && !needs_append) {
        return SupportLevelFilter::Pass;
    }
    if let Some(min) = config.support_noble_phantasm_level_min {
        if !row.np_level.is_some_and(|level| level >= min) {
            return SupportLevelFilter::Fail(format!(
                "宝具 ≥ {}（实际 {}）",
                min,
                format_actual_level(row.np_level),
            ));
        }
    }
    if !needs_owned && !needs_append {
        return SupportLevelFilter::Pass;
    }

    let key = support_level_candidate_key(row);
    if progress.candidate_key.as_deref() != Some(key.as_str()) {
        progress.candidate_key = Some(key);
        progress.owned_met = false;
        progress.append_met = false;
        progress.panel_toggle_taps = 0;
    }

    match row.skill_panel.as_deref() {
        Some("owned") if needs_owned => {
            if let Some((index, actual, min)) =
                support_first_level_mismatch(&row.skill_levels, &config.support_skill_level_mins)
            {
                progress.owned_met = false;
                return SupportLevelFilter::Fail(format!(
                    "持有技能 {} ≥ {}（实际 {}）",
                    index + 1,
                    min,
                    format_actual_level(actual),
                ));
            }
            progress.owned_met = true;
        }
        Some("append") if needs_append => {
            if let Some((index, actual, min)) = support_first_level_mismatch(
                &row.append_skill_levels,
                &config.support_append_skill_level_mins,
            ) {
                progress.append_met = false;
                return SupportLevelFilter::Fail(format!(
                    "追加技能 {} ≥ {}（实际 {}）",
                    index + 1,
                    min,
                    format_actual_level(actual),
                ));
            }
            progress.append_met = true;
        }
        Some(_) | None => {}
    }

    if (!needs_owned || progress.owned_met) && (!needs_append || progress.append_met) {
        SupportLevelFilter::Pass
    } else {
        SupportLevelFilter::WaitingForPanel
    }
}

fn support_skill_diag_message(row: &SupportRowMatch) -> String {
    if row.skill_level_diagnostics.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = row
        .skill_level_diagnostics
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let level = item
                .get("level")
                .and_then(|v| v.as_u64())
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into());
            let score = item.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let source = item.get("source").and_then(|v| v.as_str()).unwrap_or("");
            format!("{}:{}@{:.2}/{}", index + 1, level, score, source)
        })
        .collect();
    format!(" skill [{}]", parts.join(" "))
}

/// Map an Atlas Academy `className` (already lowercased by
/// `load_servant_metadata`) to the support-select class-filter tab.
/// Returns `None` for unknown / boss-only classes (beasts, etc.) so the
/// caller can skip the tap and log it instead of guessing wrong.
fn class_tab_for(class_name: &str) -> Option<Point> {
    match class_name {
        "saber" => Some(SUPPORT_TAB_SABER),
        "archer" => Some(SUPPORT_TAB_ARCHER),
        "lancer" => Some(SUPPORT_TAB_LANCER),
        "rider" => Some(SUPPORT_TAB_RIDER),
        "caster" => Some(SUPPORT_TAB_CASTER),
        "assassin" => Some(SUPPORT_TAB_ASSASSIN),
        "berserker" => Some(SUPPORT_TAB_BERSERKER),
        // "Extra" tab covers every non-knight / non-cavalry class:
        // shielder, ruler, avenger, alterego, mooncancer, foreigner,
        // pretender. Atlas mixes camelCase and lowercase forms so
        // `load_servant_metadata` lowercases before we land here.
        "shielder" | "ruler" | "avenger" | "alterego" | "mooncancer" | "foreigner"
        | "pretender" => Some(SUPPORT_TAB_EXTRA),
        _ => None,
    }
}
const UNKNOWN_TIMEOUT: u32 = 10;
/// Tolerated streak of `Unknown` screens while a long animation / loading
/// transition is playing -- raised from the default so a stacked NP chain
/// (which can run 30s+ of cut-ins before the battle screen reappears)
/// doesn't trip the "无法识别当前画面" error. 75 * 800ms = 60s.
const UNKNOWN_TIMEOUT_LOADING: u32 = 75;

/// Poll cadence for `wait_for_attack_button` while a skill animation
/// (cut-in, NP charge effect, etc.) is hiding the attack button.
const SKILL_POLL_INTERVAL: Duration = Duration::from_millis(300);
/// Hard cap on how long we'll wait for the attack button to come back
/// after a skill. Some skills trigger long buff cut-ins or animations
/// (and a few skills push an NP-charge cut-in on top), so this needs
/// to cover NP-length animations without hanging forever if something
/// genuinely went wrong.
const SKILL_WAIT_TIMEOUT: Duration = Duration::from_secs(45);
const COMMAND_CARD_COUNT: usize = 5;
/// Order Change opens as a semi-transparent overlay over Battle. The
/// classifier often keeps returning `Battle`, so the runner waits for the
/// overlay animation to settle and then taps the known panel coordinates.
const ORDER_CHANGE_PANEL_SETTLE: Duration = Duration::from_millis(900);

/// Maximum per-axis jitter (in physical pixels) added to every tap so
/// repeated runs don't land on identical coordinates. Small enough to
/// stay well inside button hit-boxes; large enough that the noise is
/// distinguishable from a deterministic script.
const TAP_JITTER_PX: i32 = 6;

const DEFAULT_W: u32 = 1080;
const DEFAULT_H: u32 = 1920;

pub struct Runner {
    adb: Adb,
    sidecar: Option<SidecarClient>,
    sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
    config: RunConfig,
    scenes: Vec<BattleScene>,
    advanced_mode: bool,
    advanced_scenes: Vec<AdvancedBattleScene>,
    state: Arc<Mutex<RunnerState>>,
    cancel: Arc<AtomicBool>,
    stop_after_current: Arc<AtomicBool>,
    app_handle: tauri::AppHandle,
    screen_w: u32,
    screen_h: u32,
    /// Per-servant face assets (`{id}/card_servant_*.png`). When ``None`` the
    /// sidecar can still report suit + slot but cannot identify which
    /// servant owns each command card, which means priority entries can't
    /// be matched and we fall through to the leftmost-fill path.
    assets_dir: Option<PathBuf>,
    /// Per-CE icon assets (`{ce_id}/card_ce.png`). Used by
    /// `handle_support_select` to verify a row's equipped CE matches the
    /// pinned support CE. `None` means we couldn't locate the dir on
    /// disk; CE verification is silently skipped in that case so the
    /// existing flow (pick first OCR match) still works.
    ce_assets_dir: Option<PathBuf>,
    /// Game server this run targets (JP/CN). Forwarded to
    /// `load_servant_metadata` so the cached `support_meta.name` /
    /// `np_names` come out in the right language for the OCR model the
    /// sidecar is using.
    server: Server,
    // Pre-battle progress tracking
    team_changed: bool,
    support_selected: bool,
    support_scroll_count: u32,
    /// How many times we've tapped the friend-list refresh button this run.
    /// Reset alongside `support_scroll_count` once a match is selected.
    support_refresh_count: u32,
    /// True once we've tapped the class-filter tab corresponding to the
    /// pinned servant's class. Reset on refresh (the refresh sometimes
    /// snaps the UI back to "all") so we always re-confirm the filter
    /// after a friend-list reload.
    support_class_tab_done: bool,
    /// Cached `(name, np_names, class_name)` for the pinned support
    /// servant. Loaded lazily on the first `handle_support_select` poll so
    /// we don't do disk I/O at 500ms cadence (and cleared between runs
    /// because each run owns its own `Runner`).
    support_meta: Option<ServantMetadata>,
    /// Cached resolved path to the pinned support CE template, computed
    /// once on the first poll where `support_craft_essence_id` is set.
    /// Outer `Option` is "have we tried to resolve yet"; inner `Option`
    /// is "did it succeed" (`None` = template missing / no CE pinned →
    /// skip verification).
    support_ce_template: Option<Option<PathBuf>>,
    support_grand_ce_templates: Option<[Option<PathBuf>; 3]>,
    /// Whether the current refreshed support list has ever shown the Grand
    /// avatar-frame probe. A missing probe before this flips true is not
    /// enough to conclude the Grand section is exhausted, because first-page
    /// template probes can be transiently stale while the list settles.
    support_grand_section_seen: bool,
    /// Consecutive Grand avatar-frame misses after the section was seen.
    support_grand_section_misses: u8,
    /// Tracks visible skill-panel validation for the current support row.
    /// Owned and append skills are shown on alternating panels, so a row can
    /// only satisfy both groups across multiple OCR polls.
    support_level_progress: SupportLevelPanelProgress,
    servants_placed: Vec<u32>,
    // Battle progress tracking
    battle: BattleState,
    completed_mission_runs: u32,
    battle_result_continue_handled: bool,
    /// Pluggable touch-injection backend (see `touch::TouchBackend`).
    /// Currently always `adb-input`; the trait indirection is kept so
    /// faster transports can be added later without changing the
    /// runner's call sites. The runner just calls `tap` / `swipe` /
    /// `swipe_with_settle` against the trait.
    touch: Box<dyn TouchBackend>,
}

impl Runner {
    pub fn new(
        adb: Adb,
        sidecar: SidecarClient,
        config: RunConfig,
        scenes: Vec<BattleScene>,
        advanced_mode: bool,
        advanced_scenes: Vec<AdvancedBattleScene>,
        app_handle: tauri::AppHandle,
        state: Arc<Mutex<RunnerState>>,
        cancel: Arc<AtomicBool>,
        stop_after_current: Arc<AtomicBool>,
        screen_size: Option<(u32, u32)>,
        assets_dir: Option<PathBuf>,
        ce_assets_dir: Option<PathBuf>,
        server: Server,
        sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
    ) -> Self {
        let (screen_w, screen_h) = screen_size.unwrap_or((DEFAULT_W, DEFAULT_H));
        let touch = build_touch_backend(&adb);
        Self {
            adb,
            touch,
            sidecar: Some(sidecar),
            sidecar_cache,
            config,
            scenes,
            advanced_mode,
            advanced_scenes,
            state,
            cancel,
            stop_after_current,
            app_handle,
            screen_w,
            screen_h,
            assets_dir,
            ce_assets_dir,
            server,
            team_changed: false,
            support_selected: false,
            support_scroll_count: 0,
            support_refresh_count: 0,
            support_class_tab_done: false,
            support_meta: None,
            support_ce_template: None,
            support_grand_ce_templates: None,
            support_grand_section_seen: false,
            support_grand_section_misses: 0,
            support_level_progress: SupportLevelPanelProgress::default(),
            servants_placed: Vec::new(),
            battle: BattleState::new(),
            completed_mission_runs: 0,
            battle_result_continue_handled: false,
        }
    }

    // -- helpers -------------------------------------------------------------

    fn set_state(&self, s: RunnerState) {
        *self.state.lock().unwrap() = s;
    }

    fn emit(&self, screen: &str, message: &str) {
        self.emit_with_level(screen, message, LogLevel::Info);
    }

    /// Like [`emit`] but at `LogLevel::Debug`. Use for technical
    /// diagnostics that the user doesn't normally want to see — they
    /// stay hidden behind the operation-log "显示调试" toggle in the
    /// status bar. Keep these messages compact (a single line) since
    /// the panel doesn't wrap long entries gracefully.
    fn emit_debug(&self, screen: &str, message: &str) {
        self.emit_with_level(screen, message, LogLevel::Debug);
    }

    fn emit_with_level(&self, screen: &str, message: &str, level: LogLevel) {
        let state_str = {
            let s = self.state.lock().unwrap();
            format!("{:?}", *s)
        };
        let _ = self.app_handle.emit(
            "automation-status",
            AutomationEvent {
                state: state_str,
                current_screen: screen.into(),
                message: message.into(),
                level,
            },
        );
    }

    fn sidecar(&mut self) -> &mut SidecarClient {
        self.sidecar.as_mut().expect("runner sidecar missing")
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn should_stop_after_current(&self) -> bool {
        self.stop_after_current.load(Ordering::Relaxed)
    }

    fn fail_action(&self, screen: &str, action: &str, err: String) {
        let message = format!("{action}失败: {err}");
        self.set_state(RunnerState::Error {
            message: message.clone(),
        });
        self.emit(screen, &message);
    }

    fn tap_at(&mut self, screen: &str, point: Point) -> bool {
        let (px, py) = point.to_physical(self.screen_w, self.screen_h);
        let (jx, jy) = jitter_offset();
        // Saturate at the screen edges so a near-edge button still
        // registers even if the jitter would push it off-screen.
        let tap_x = (px as i32 + jx).clamp(0, self.screen_w.saturating_sub(1) as i32) as u32;
        let tap_y = (py as i32 + jy).clamp(0, self.screen_h.saturating_sub(1) as i32) as u32;
        match self.touch.tap(tap_x, tap_y) {
            Ok(()) => true,
            Err(err) => {
                self.fail_action(screen, "点击", err);
                false
            }
        }
    }

    fn swipe_at(&mut self, screen: &str, from: Point, to: Point, duration_ms: u32) -> bool {
        let from_px = from.to_physical(self.screen_w, self.screen_h);
        let to_px = to.to_physical(self.screen_w, self.screen_h);
        match self.touch.swipe(from_px, to_px, duration_ms) {
            Ok(()) => true,
            Err(err) => {
                self.fail_action(screen, "滑动", err);
                false
            }
        }
    }

    /// "Press, drag, hold, release" swipe via the active `TouchBackend`.
    /// Use this for any swipe whose precise stopping position matters
    /// — the settle window prevents Android's fling momentum from
    /// continuing the scroll past the lift-off coordinate.
    fn swipe_with_settle_at(
        &mut self,
        screen: &str,
        from: Point,
        to: Point,
        swipe_ms: u32,
        settle_ms: u32,
    ) -> bool {
        let from_px = from.to_physical(self.screen_w, self.screen_h);
        let to_px = to.to_physical(self.screen_w, self.screen_h);
        match self.touch.swipe_with_settle(from_px, to_px, swipe_ms, settle_ms) {
            Ok(()) => true,
            Err(err) => {
                self.fail_action(screen, "滑动", err);
                false
            }
        }
    }

    /// Tap `point` repeatedly, every `interval`, until either
    /// `sidecar.detect()` returns a screen different from `from_screen`,
    /// `timeout` elapses, or the user cancels the run.
    ///
    /// The bond and EXP result pages in particular have a ~3–5 s
    /// per-servant level-up / bond animation that absorbs the very first
    /// tap silently — the page stays mounted until the animation
    /// completes, so the legacy "one tap per main-loop poll" cadence
    /// (~one tap every 800 ms) wastes most of those taps and leaves the
    /// runner sitting on the result page much longer than necessary.
    /// Tapping inside the handler at a tighter cadence and bailing the
    /// instant the screen actually changes drops the average bond-screen
    /// dwell from ~6 s to under 2 s on common quests.
    ///
    /// Returns ``true`` when the screen changed away from `from_screen`,
    /// ``false`` on timeout, cancellation, or ADB tap failure (which is
    /// already reported via `fail_action` from `tap_at`).
    fn tap_until_screen_changes(
        &mut self,
        screen_label: &str,
        from_screen: Screen,
        point: Point,
        interval: Duration,
        timeout: Duration,
    ) -> bool {
        let start = std::time::Instant::now();
        let mut taps: u32 = 0;
        loop {
            if self.is_cancelled() {
                return false;
            }
            if !self.tap_at(screen_label, point) {
                return false;
            }
            taps += 1;
            thread::sleep(interval);
            // detect() failures are best-effort here — treat them as
            // "screen unchanged" so we keep tapping rather than bailing.
            // A persistent CV error will surface on the main loop's
            // next iteration via the same call path.
            let detected = self.sidecar().detect(None).unwrap_or(from_screen);
            if detected != from_screen {
                return true;
            }
            if start.elapsed() >= timeout {
                self.emit(
                    screen_label,
                    &format!("等待画面切换超时 (已点击 {taps} 次)"),
                );
                return false;
            }
        }
    }

    /// Block until the attack-button probe in `cv.json` matches, polling
    /// every `SKILL_POLL_INTERVAL`. Used after firing a skill so
    /// the next tap doesn't land during the cut-in / animation while the
    /// button is hidden.
    ///
    /// Returns ``true`` when the button is detected, ``false`` on timeout
    /// or cancellation. Emits status updates so the user can see the wait.
    fn wait_for_attack_button(&mut self, screen: &str, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        let mut tick: u32 = 0;
        loop {
            if self.is_cancelled() {
                return false;
            }
            let found = self
                .sidecar()
                .find_element_by_name(None, BATTLE_SCREEN, ATTACK_BUTTON_ELEMENT)
                .map(|m| m.found)
                .unwrap_or(false);
            if found {
                return true;
            }
            if start.elapsed() >= timeout {
                self.emit(screen, "等待攻击按钮超时");
                return false;
            }
            tick += 1;
            if tick % 4 == 1 {
                self.emit(screen, "等待技能动画结束…");
            }
            thread::sleep(SKILL_POLL_INTERVAL);
        }
    }

    // -- main loop -----------------------------------------------------------

    pub fn run(mut self) {
        self.set_state(RunnerState::Running);
        self.emit("", "自动化已启动");

        let mut unknown_count: u32 = 0;

        loop {
            if self.is_cancelled() {
                self.set_state(RunnerState::Idle);
                self.emit("", "自动化已停止");
                return;
            }

            let screen = match self.sidecar().detect(None) {
                Ok(s) => s,
                Err(e) => {
                    self.set_state(RunnerState::Error { message: e.clone() });
                    self.emit("", &format!("画面识别失败: {e}"));
                    return;
                }
            };

            if screen != Screen::BattleResultContinue {
                self.battle_result_continue_handled = false;
            }

            match screen {
                Screen::TeamConfirm => {
                    unknown_count = 0;
                    self.handle_team_confirm();
                }
                Screen::TeamChange => {
                    unknown_count = 0;
                    self.handle_team_change();
                }
                Screen::SupportSelect => {
                    unknown_count = 0;
                    self.handle_support_select();
                }
                Screen::ServantSelect => {
                    unknown_count = 0;
                    self.handle_servant_select();
                }
                Screen::Battle => {
                    unknown_count = 0;
                    // NOTE: do NOT clear `waiting_for_battle` here. The
                    // screen classifier briefly returns Battle between
                    // taps and the NP cinematic, and clearing the flag
                    // too early would shrink the Unknown tolerance back
                    // to UNKNOWN_TIMEOUT mid-animation. handle_battle
                    // clears it itself once the attack button is
                    // actually visible (= we can really act).
                    self.handle_battle();
                }
                Screen::Attack => {
                    unknown_count = 0;
                    if self.battle.attack_submitted {
                        self.emit("Attack", "已提交本轮选卡，等待攻击动画");
                    } else {
                        self.handle_attack();
                    }
                }
                Screen::BattleResultBond => {
                    unknown_count = 0;
                    self.handle_battle_result_bond();
                }
                Screen::BattleResultExp => {
                    unknown_count = 0;
                    self.handle_battle_result_exp();
                }
                Screen::BattleResultLoot => {
                    unknown_count = 0;
                    self.handle_battle_result_loot();
                }
                Screen::BattleResultFriendRequest => {
                    unknown_count = 0;
                    self.handle_battle_result_friend_request();
                }
                Screen::BattleResultContinue => {
                    unknown_count = 0;
                    self.handle_battle_result_continue();
                }
                Screen::APRecovery => {
                    unknown_count = 0;
                    self.handle_ap_recovery();
                }
                Screen::Unknown => {
                    unknown_count += 1;
                    let timeout = if self.battle.waiting_for_battle {
                        UNKNOWN_TIMEOUT_LOADING
                    } else {
                        UNKNOWN_TIMEOUT
                    };
                    if unknown_count >= timeout {
                        self.set_state(RunnerState::Error {
                            message: "无法识别当前画面".into(),
                        });
                        self.emit("Unknown", "无法识别当前画面，已超时停止");
                        return;
                    }
                    self.emit(
                        "Unknown",
                        &format!("等待识别画面… ({unknown_count}/{timeout})"),
                    );
                }
            }

            if matches!(*self.state.lock().unwrap(), RunnerState::Error { .. }) {
                return;
            }

            if matches!(*self.state.lock().unwrap(), RunnerState::Finished) {
                self.emit("", "自动化已完成");
                return;
            }

            thread::sleep(POLL_INTERVAL);
        }
    }

    // -- pre-battle screen handlers ------------------------------------------

    fn handle_team_confirm(&mut self) {
        if self.config.party_order.is_some() && !self.team_changed {
            self.emit("TeamConfirm", "需要调整队伍顺序，进入编成变更");
            if !self.tap_at("TeamConfirm", Point::new(0.83, 0.90)) {
                return;
            }
            thread::sleep(ACTION_DELAY);
            return;
        }

        if let Some(slot_cfg) = self.next_unfilled_slot() {
            self.emit(
                "TeamConfirm",
                &format!("选择从者到槽位 {}", slot_cfg.slot_index),
            );
            let slot_x = slot_x_position(slot_cfg.slot_index);
            if !self.tap_at("TeamConfirm", Point::new(slot_x, 0.45)) {
                return;
            }
            thread::sleep(ACTION_DELAY);
            return;
        }

        self.emit("TeamConfirm", "队伍就绪，点击开始任务");
        if !self.tap_at("TeamConfirm", Point::new(0.90, 0.93)) {
            return;
        }
        self.battle.waiting_for_battle = true;
        thread::sleep(ACTION_DELAY);
    }

    fn handle_team_change(&mut self) {
        if self.config.party_order.is_some() {
            // TODO: implement swap logic based on current vs desired order.
            self.emit("TeamChange", "完成顺序调整，确认返回");
            if !self.tap_at("TeamChange", Point::new(0.90, 0.93)) {
                return;
            }
            self.team_changed = true;
            thread::sleep(ACTION_DELAY);
        } else {
            if !self.tap_at("TeamChange", Point::new(0.05, 0.05)) {
                return;
            }
            thread::sleep(ACTION_DELAY);
        }
    }

    fn handle_support_select(&mut self) {
        // No servant pinned → fall back to "tap the top of the list" so
        // existing setups that never picked a support still work.
        let Some(servant_id) = self.config.support_servant_id else {
            self.legacy_pick_first_support();
            return;
        };

        // Lazy-load (name, np_names, class_name) once per run. The shared
        // static cache in `lib.rs` makes this cheap, but caching on the
        // runner avoids even hashing it at every poll.
        if self.support_meta.is_none() {
            match load_servant_metadata(&self.app_handle, servant_id, self.server) {
                Ok(meta) => self.support_meta = Some(meta),
                Err(e) => {
                    self.fail_action(
                        "SupportSelect",
                        "加载助战元数据",
                        format!("servant {servant_id}: {e}"),
                    );
                    return;
                }
            }
        }
        let meta = self.support_meta.clone().unwrap();

        // Filter the list to the servant's class before scanning. We avoid
        // searching from "all" + "mix" because they interleave duplicates
        // and lengthen every OCR pass; the class tab restricts the list to
        // exactly the rows we care about.
        if !self.support_class_tab_done {
            match class_tab_for(&meta.class_name) {
                Some(tab) => {
                    self.emit(
                        "SupportSelect",
                        &format!("切换职阶筛选 -> {}", meta.class_name),
                    );
                    if !self.tap_at("SupportSelect", tab) {
                        return;
                    }
                    self.support_class_tab_done = true;
                    thread::sleep(SUPPORT_CLASS_TAB_SETTLE);
                    return;
                }
                None => {
                    // Unknown class → mark done so we don't loop, and let
                    // the OCR pass run against whatever tab is active.
                    eprintln!(
                        "[runner] no class-tab mapping for className='{}', skipping filter",
                        meta.class_name,
                    );
                    self.support_class_tab_done = true;
                }
            }
        }

        // OCR the current (class-filtered) screen and look for a row
        // whose name + NP both fuzzy-match the pinned servant.
        let include_support_details =
            self.server == crate::Server::Cn && self.has_support_level_requirements();
        let result = match self.sidecar().find_supports(
            None,
            &meta.name,
            &meta.np_names,
            include_support_details,
        ) {
            Ok(r) => r,
            Err(e) => {
                self.fail_action("SupportSelect", "OCR 助战识别", e);
                return;
            }
        };

        // `supports` is already sorted top-down by the sidecar. Default
        // pick = first match, but optional CE / level filters can reject
        // rows before we tap.
        let ce_filter_enabled = self.support_ce_filter_enabled();
        let mut waiting_for_skill_panel = false;
        let mut ce_filter_reasons: Vec<String> = Vec::new();
        // Distinct mismatch reasons across the visible candidates, in
        // first-seen order. Same servant from multiple friends often
        // means the same gap (e.g. "持有技能 2 ≥ 10（实际 8）"); dedup
        // keeps the log readable.
        let mut level_filter_reasons: Vec<String> = Vec::new();
        let mut chosen_index: Option<usize> = None;
        for (index, row) in result.supports.iter().enumerate() {
            if let Some(reason) = self.support_row_ce_mismatch(row) {
                if !ce_filter_reasons.contains(&reason) {
                    ce_filter_reasons.push(reason);
                }
                continue;
            }
            match support_row_matches_level_requirements_with_progress(
                self.server,
                &self.config,
                row,
                &mut self.support_level_progress,
            ) {
                SupportLevelFilter::Pass => {
                    chosen_index = Some(index);
                    break;
                }
                SupportLevelFilter::WaitingForPanel => {
                    waiting_for_skill_panel = true;
                }
                SupportLevelFilter::Fail(reason) => {
                    if !level_filter_reasons.contains(&reason) {
                        level_filter_reasons.push(reason);
                    }
                }
            }
        }
        let chosen = chosen_index.and_then(|index| result.supports.get(index));

        if let Some(row) = chosen {
            self.emit(
                "SupportSelect",
                &format!(
                    "找到助战 {} (name {:.2}, np {:.2}){}",
                    meta.name,
                    row.name_score,
                    row.np_score,
                    support_skill_diag_message(row),
                ),
            );
            if !self.tap_at("SupportSelect", row.tap) {
                return;
            }
            self.support_selected = true;
            self.support_scroll_count = 0;
            self.support_refresh_count = 0;
            self.support_grand_section_seen = false;
            self.support_grand_section_misses = 0;
            self.support_class_tab_done = false;
            self.support_level_progress = SupportLevelPanelProgress::default();
            thread::sleep(ACTION_DELAY);
            return;
        }

        // OCR found rows but the pinned CE didn't match any of them —
        // emit a distinct message before falling through to the
        // scroll/refresh branch so the user knows it's a CE filter miss
        // (vs a name miss).
        if ce_filter_enabled && !result.supports.is_empty() {
            let reason = if ce_filter_reasons.is_empty() {
                "礼装不匹配".to_string()
            } else {
                ce_filter_reasons.join("；")
            };
            self.emit("SupportSelect", &format!("找到从者但{reason}，继续滚动…"));
        }
        if waiting_for_skill_panel {
            // The candidate's name + NP match but we still need the
            // *other* skill panel to confirm its level requirements.
            // We can't rely on the game auto-flipping panels (the user
            // may have the toggle locked on 固定持有 or 固定追加), so
            // actively tap "技能显示切换" until both panels have been
            // observed. Cap at MAX_TOGGLE_TAPS so a row that genuinely
            // can't be verified (e.g. icon rendering bug) eventually
            // releases us back to the scroll branch.
            if self.support_level_progress.panel_toggle_taps < SUPPORT_SKILL_PANEL_MAX_TOGGLE_TAPS {
                self.support_level_progress.panel_toggle_taps += 1;
                self.emit(
                    "SupportSelect",
                    &format!(
                        "找到从者，主动点击技能显示切换 ({}/{})",
                        self.support_level_progress.panel_toggle_taps,
                        SUPPORT_SKILL_PANEL_MAX_TOGGLE_TAPS,
                    ),
                );
                if !self.tap_at("SupportSelect", SUPPORT_SKILL_PANEL_TOGGLE_BUTTON) {
                    return;
                }
                thread::sleep(SUPPORT_SKILL_PANEL_TOGGLE_SETTLE);
                return;
            }
            self.emit(
                "SupportSelect",
                "切换面板已达上限，跳过该助战，继续滚动列表…",
            );
            // Drop the per-candidate accumulator so the next visible
            // candidate (after scroll/refresh) starts fresh, then fall
            // through to the scroll/refresh branch below.
            self.support_level_progress = SupportLevelPanelProgress::default();
        }
        if !level_filter_reasons.is_empty() {
            self.emit(
                "SupportSelect",
                &format!(
                    "找到从者但等级不匹配（{}），继续滚动…",
                    level_filter_reasons.join("；"),
                ),
            );
        }

        // No match in the visible viewport. Normal support scans use the
        // scroll-bar tail indicator as the source of truth. Grand support
        // scans can stop earlier: Grand rows are listed before ordinary
        // rows, and the per-anchor "冠位从者" ribbon probe (run by the
        // sidecar inside `find_supports` and surfaced via
        // `diagnostics.is_grand_section_visible`) goes false once the
        // visible page has scrolled past the Grand section.
        let grand_section_exhausted = self.config.support_grand_mode
            && self.update_support_grand_section_exhausted(
                result.diagnostics.is_grand_section_visible,
            );
        if !grand_section_exhausted && !self.support_scroll_bar_at_end() {
            // Surface the Grand-section signal at info level when it
            // fired this poll, so the operator sees in the log that we
            // chose to keep scrolling because a 冠位从者 row was still
            // visible (vs. just hitting the generic "未找到, 滚动" path
            // on an ordinary list).
            let grand_visible_in_grand_mode = self.config.support_grand_mode
                && result.diagnostics.is_grand_section_visible == Some(true);
            let scroll_reason = if grand_visible_in_grand_mode {
                "识别到冠位从者，继续滚动"
            } else {
                "滚动列表"
            };
            self.emit(
                "SupportSelect",
                &format!(
                    "未找到 {}，{} (第 {} 次)",
                    meta.name,
                    scroll_reason,
                    self.support_scroll_count + 1,
                ),
            );
            if !self.scroll_support_list(&result.diagnostics.confirm_button_anchors) {
                return;
            }
            self.support_scroll_count += 1;
            self.support_level_progress = SupportLevelPanelProgress::default();
            thread::sleep(SUPPORT_SCROLL_SETTLE);
        } else if self.support_refresh_count < SUPPORT_MAX_REFRESHES {
            let reason = if grand_section_exhausted {
                "冠位助战已扫完"
            } else {
                "已到底部"
            };
            self.emit(
                "SupportSelect",
                &format!(
                    "{reason}，刷新助战列表 ({}/{})",
                    self.support_refresh_count + 1,
                    SUPPORT_MAX_REFRESHES,
                ),
            );
            if !self.tap_at("SupportSelect", SUPPORT_REFRESH_BUTTON) {
                return;
            }
            self.support_scroll_count = 0;
            self.support_refresh_count += 1;
            self.support_grand_section_seen = false;
            self.support_grand_section_misses = 0;
            self.support_level_progress = SupportLevelPanelProgress::default();
            // Refresh occasionally snaps the class filter back to "all";
            // re-tap the class tab on the next poll to be safe.
            self.support_class_tab_done = false;
            if !self.confirm_support_refresh_dialog_if_needed() {
                return;
            }
        } else {
            self.fail_action(
                "SupportSelect",
                "查找助战",
                format!("刷新 {} 次仍未找到 {}", SUPPORT_MAX_REFRESHES, meta.name),
            );
        }
    }

    /// Resolve the on-disk path for the pinned support CE template, or
    /// `None` if no CE is pinned, no CE assets directory was located, or
    /// the template file is missing. Cached on first call so repeated
    /// `handle_support_select` polls don't restat the filesystem.
    fn resolve_support_ce_template(&mut self) -> Option<PathBuf> {
        if let Some(cached) = &self.support_ce_template {
            return cached.clone();
        }
        let resolved = if self.config.support_grand_mode {
            None
        } else {
            self.config
                .support_craft_essence_id
                .and_then(|ce_id| self.resolve_ce_template_path(ce_id))
        };
        self.support_ce_template = Some(resolved.clone());
        resolved
    }

    fn resolve_support_grand_ce_templates(&mut self) -> [Option<PathBuf>; 3] {
        if let Some(cached) = &self.support_grand_ce_templates {
            return cached.clone();
        }
        let resolved = std::array::from_fn(|index| {
            self.config.support_grand_craft_essence_ids[index]
                .and_then(|ce_id| self.resolve_ce_template_path(ce_id))
        });
        self.support_grand_ce_templates = Some(resolved.clone());
        resolved
    }

    fn resolve_ce_template_path(&self, ce_id: u32) -> Option<PathBuf> {
        let dir = self.ce_assets_dir.as_ref()?;
        let path = dir.join(ce_id.to_string()).join("card_ce.png");
        if path.is_file() {
            Some(path)
        } else {
            eprintln!(
                "[runner] support CE template missing: {} (skipping CE filter)",
                path.display()
            );
            None
        }
    }

    fn support_ce_filter_enabled(&self) -> bool {
        if self.config.support_grand_mode {
            self.config
                .support_grand_craft_essence_ids
                .iter()
                .any(Option::is_some)
        } else {
            self.config.support_craft_essence_id.is_some()
        }
    }

    /// Compute the absolute search window for a row's CE icon by
    /// applying `SUPPORT_CE_OFFSET_IN_ROW` (a row-local rect) to the
    /// row's full bbox. Thin method wrapper around the pure free helper
    /// [`ce_search_region`] (kept free so unit tests can exercise the
    /// math without constructing a full `SupportRowMatch`).
    fn support_ce_search_region(row: &SupportRowMatch) -> NormRect {
        ce_search_region(row.row_region)
    }

    fn support_grand_ce_search_region(row: &SupportRowMatch, slot: usize) -> Option<NormRect> {
        grand_ce_search_region(row, slot)
    }

    /// Return true when a row's CE icon scores at or above
    /// `SUPPORT_CE_THRESHOLD` against `template_path`.
    fn support_row_matches_ce(&mut self, row: &SupportRowMatch, template_path: &Path) -> bool {
        let region = Self::support_ce_search_region(row);
        self.support_row_region_ce_mismatch(
            region,
            template_path,
            "礼装",
            SupportCeVerificationOptions {
                mlb_required: self.config.support_craft_essence_mlb_required,
                grand_bond_ce_mode: None,
            },
        )
        .is_none()
    }

    fn support_row_region_ce_mismatch(
        &mut self,
        region: NormRect,
        template_path: &Path,
        label: &str,
        options: SupportCeVerificationOptions,
    ) -> Option<String> {
        match self.sidecar().verify_support_ce(
            None,
            region,
            template_path,
            SUPPORT_CE_THRESHOLD,
            options,
        ) {
            Ok(result) => {
                let effective_threshold = if result.threshold > 0.0 {
                    result.threshold
                } else {
                    SUPPORT_CE_THRESHOLD
                };
                eprintln!(
                    "[runner] {label} verify: score={:.3} threshold={:.2} -> {}",
                    result.score,
                    effective_threshold,
                    if result.passed { "PASS" } else { "skip" },
                );
                if !result.artwork_checks.is_empty() {
                    eprintln!(
                        "[runner] {label} variants: {}",
                        format_ce_artwork_checks(&result.artwork_checks),
                    );
                }
                for check in &result.icon_checks {
                    eprintln!(
                        "[runner] {label} {} icon: score={:.3} threshold={:.2} -> {}",
                        check.kind,
                        check.score,
                        check.threshold,
                        if check.passed { "PASS" } else { "skip" },
                    );
                }
                if result.passed {
                    None
                } else if let Some(check) = result.icon_checks.iter().find(|check| !check.passed) {
                    // Surface decoration-icon failures with a more actionable
                    // message when present; if the artwork also failed but
                    // the icon failed too, the icon miss is the cleaner
                    // root cause to surface to the operator.
                    let kind = match check.kind.as_str() {
                        "mlb" => "满破图标",
                        "grandBond" => "原始牵绊图标",
                        "grandBondNp" => "冠位连接牵绊图标",
                        other => other,
                    };
                    Some(format!("{label} {kind}不匹配"))
                } else {
                    Some(format!("{label} 不匹配"))
                }
            }
            Err(e) => {
                eprintln!("[runner] support CE verify failed (treating as skip): {e}");
                Some(format!("{label} 校验失败"))
            }
        }
    }

    fn support_row_ce_mismatch(&mut self, row: &SupportRowMatch) -> Option<String> {
        if self.config.support_grand_mode {
            let templates = self.resolve_support_grand_ce_templates();
            if templates.iter().any(Option::is_some) && row.score_anchor.is_none() {
                return Some("确认按钮未完整显示".into());
            }
            for (index, template) in templates.iter().enumerate() {
                let Some(template) = template.as_deref() else {
                    continue;
                };
                let Some(region) = Self::support_grand_ce_search_region(row, index) else {
                    return Some(format!("冠位礼装 {} 区域无效", index + 1));
                };
                if let Some(reason) = self.support_row_region_ce_mismatch(
                    region,
                    template,
                    &format!("冠位礼装 {}", index + 1),
                    SupportCeVerificationOptions {
                        mlb_required: self.config.support_grand_craft_essence_mlb_required[index],
                        grand_bond_ce_mode: if index == 1 {
                            match self.config.support_grand_bond_ce_mode {
                                SupportGrandBondCeMode::Any => None,
                                SupportGrandBondCeMode::Bond => Some("bond".to_string()),
                                SupportGrandBondCeMode::BondNp => Some("bondNp".to_string()),
                            }
                        } else {
                            None
                        },
                    },
                ) {
                    return Some(reason);
                }
            }
            None
        } else {
            let template = self.resolve_support_ce_template()?;
            if self.support_row_matches_ce(row, &template) {
                None
            } else {
                Some("礼装不匹配".into())
            }
        }
    }

    fn has_support_level_requirements(&self) -> bool {
        self.config.support_noble_phantasm_level_min.is_some()
            || self
                .config
                .support_skill_level_mins
                .iter()
                .any(Option::is_some)
            || self
                .config
                .support_append_skill_level_mins
                .iter()
                .any(Option::is_some)
    }

    /// Scroll the support list by an adaptive distance so the last
    /// visible confirm-button anchor lands near the first-row confirm
    /// position. See `scroll_support_list_delta` for the math; the short
    /// version is `delta = last_button.y - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y`.
    /// This avoids guessing about clipped rows below the viewport.
    ///
    /// When no anchors are visible (rare — usually means the template
    /// detector glitched on this frame) we fall back to a single fixed
    /// delta so the runner still makes progress.
    ///
    /// Emits a debug-level log entry with the detected anchor positions
    /// and the chosen delta so the operator can inspect what the runner
    /// "saw" when triaging a "scrolled past my servant" bug report.
    fn scroll_support_list(&mut self, confirm_button_anchors: &[NormRect]) -> bool {
        let delta = scroll_support_list_delta(confirm_button_anchors);
        let from_y = SUPPORT_SCROLL_FROM_Y;
        let to_y = (from_y - delta).max(0.05);
        let swipe_ms = scroll_support_list_duration_ms(delta);
        let scroll_msg = format_scroll_debug(
            confirm_button_anchors,
            delta,
            from_y,
            to_y,
            swipe_ms,
            SUPPORT_SCROLL_SETTLE_MS,
        );
        // Tag the debug line with the active touch backend's name so
        // an operator can tell at a glance which backend produced the
        // gesture they're triaging — useful when we add more
        // backends behind the `TouchBackend` trait again.
        self.emit_debug(
            "SupportSelect",
            &format!("{scroll_msg} [{}]", self.touch.name()),
        );
        self.swipe_with_settle_at(
            "SupportSelect",
            Point::new(0.50, from_y),
            Point::new(0.50, to_y),
            swipe_ms,
            SUPPORT_SCROLL_SETTLE_MS,
        )
    }

    /// Return true when the scroll-bar-end indicator is visible in the
    /// bottom-right corner of the support list, meaning the user has
    /// scrolled all the way down. Logs the actual match score every poll
    /// so the threshold can be tuned from real numbers; transient sidecar
    /// errors degrade to `false` so a CV blip just means "keep scrolling"
    /// instead of triggering a refresh loop.
    fn support_scroll_bar_at_end(&mut self) -> bool {
        match self.sidecar().find_element_by_name(
            None,
            SUPPORT_SELECT_SCREEN,
            SUPPORT_SCROLL_END_ELEMENT,
        ) {
            Ok(m) => {
                eprintln!(
                    "[runner] scroll-bar-end score={:.3} -> {}",
                    m.score,
                    if m.found { "AT-BOTTOM" } else { "scrolling" },
                );
                m.found
            }
            Err(e) => {
                eprintln!("[runner] scroll-bar-end check failed (treating as not-at-bottom): {e}");
                false
            }
        }
    }

    /// In Grand support mode, given the sidecar's per-frame "is at
    /// least one Grand row visible?" probe (from
    /// `SupportDiagnostics::is_grand_section_visible`), update the
    /// rolling miss counter and return whether the Grand section has
    /// been exhausted. `None` from the sidecar means the active
    /// server bundle doesn't ship the ribbon template, so callers
    /// fall back to scroll-bar-end via the helper's existing logic.
    fn update_support_grand_section_exhausted(&mut self, visible: Option<bool>) -> bool {
        eprintln!(
            "[runner] grand-servant ribbon visible={:?} (seen={}, misses={})",
            visible, self.support_grand_section_seen, self.support_grand_section_misses,
        );
        support_grand_section_exhausted_after_probe(
            visible,
            &mut self.support_grand_section_seen,
            &mut self.support_grand_section_misses,
        )
    }

    /// After tapping the support-list refresh button, confirm the modal when
    /// the server's cv.json defines one. Servers without a modal template
    /// fall back to the legacy fixed settle delay.
    fn confirm_support_refresh_dialog_if_needed(&mut self) -> bool {
        let appear_deadline = std::time::Instant::now() + SUPPORT_REFRESH_DIALOG_APPEAR_TIMEOUT;
        loop {
            if self.is_cancelled() {
                return false;
            }
            match self.sidecar().find_element_by_name(
                None,
                SUPPORT_SELECT_SCREEN,
                SUPPORT_REFRESH_DIALOG_ELEMENT,
            ) {
                Ok(m) if m.found => break,
                Ok(_) => {}
                Err(e)
                    if is_unknown_element_error(
                        &e,
                        SUPPORT_SELECT_SCREEN,
                        SUPPORT_REFRESH_DIALOG_ELEMENT,
                    ) =>
                {
                    thread::sleep(SUPPORT_REFRESH_SETTLE);
                    return true;
                }
                Err(e) => {
                    eprintln!("[runner] refresh dialog probe failed (treating as absent): {e}");
                    break;
                }
            }
            if std::time::Instant::now() >= appear_deadline {
                thread::sleep(SUPPORT_REFRESH_SETTLE);
                return true;
            }
            thread::sleep(SUPPORT_REFRESH_DIALOG_POLL);
        }

        thread::sleep(SUPPORT_REFRESH_DIALOG_CONFIRM_SETTLE);
        if self.is_cancelled() {
            return false;
        }

        self.emit("SupportSelect", "助战刷新需要确认，点击确定");
        if !self.tap_at("SupportSelect", SUPPORT_REFRESH_CONFIRM_BUTTON) {
            return false;
        }

        let dismiss_deadline = std::time::Instant::now() + SUPPORT_REFRESH_DIALOG_DISMISS_TIMEOUT;
        loop {
            if self.is_cancelled() {
                return false;
            }
            match self.sidecar().find_element_by_name(
                None,
                SUPPORT_SELECT_SCREEN,
                SUPPORT_REFRESH_DIALOG_ELEMENT,
            ) {
                Ok(m) if !m.found => {
                    thread::sleep(SUPPORT_REFRESH_DIALOG_POLL);
                    return true;
                }
                Ok(_) => {}
                Err(e)
                    if is_unknown_element_error(
                        &e,
                        SUPPORT_SELECT_SCREEN,
                        SUPPORT_REFRESH_DIALOG_ELEMENT,
                    ) =>
                {
                    thread::sleep(SUPPORT_REFRESH_SETTLE);
                    return true;
                }
                Err(e) => {
                    eprintln!("[runner] refresh dialog disappearance probe failed: {e}");
                    thread::sleep(SUPPORT_REFRESH_SETTLE);
                    return true;
                }
            }
            if std::time::Instant::now() >= dismiss_deadline {
                self.emit("SupportSelect", "等待助战刷新确认关闭超时");
                return false;
            }
            thread::sleep(SUPPORT_REFRESH_DIALOG_POLL);
        }
    }

    /// Legacy "tap the top of the list" path used when the project hasn't
    /// pinned a support servant. Mirrors the pre-OCR behaviour so existing
    /// projects don't regress; new setups should pin a servant via the
    /// team-builder support slot to engage the OCR-based flow above.
    fn legacy_pick_first_support(&mut self) {
        if let Some(name) = self.config.support_servant_name.clone() {
            let region = NormRect {
                x: 0.0,
                y: 0.15,
                w: 1.0,
                h: 0.75,
            };
            if let Ok(Some(pos)) = self.sidecar().find_element(None, &name, region, 0.8) {
                self.emit("SupportSelect", &format!("找到助战从者: {name}"));
                if !self.tap_at("SupportSelect", pos) {
                    return;
                }
                self.support_selected = true;
                self.support_scroll_count = 0;
                thread::sleep(ACTION_DELAY);
                return;
            }
            // Template miss → fall through to scroll/refresh below.
            if self.support_scroll_count < self.config.max_support_scrolls {
                if !self.swipe_at(
                    "SupportSelect",
                    Point::new(0.50, 0.70),
                    Point::new(0.50, 0.30),
                    300,
                ) {
                    return;
                }
                self.support_scroll_count += 1;
                thread::sleep(ACTION_DELAY);
            } else {
                if !self.tap_at("SupportSelect", Point::new(0.92, 0.08)) {
                    return;
                }
                self.support_scroll_count = 0;
                thread::sleep(Duration::from_secs(2));
            }
            return;
        }

        self.emit("SupportSelect", "选择第一个助战从者");
        if !self.tap_at("SupportSelect", Point::new(0.50, 0.35)) {
            return;
        }
        self.support_selected = true;
        thread::sleep(ACTION_DELAY);
    }

    fn handle_servant_select(&mut self) {
        if let Some(slot_cfg) = self.next_unfilled_slot() {
            let servant_key = format!("servant_{}", slot_cfg.servant_id);
            let region = NormRect {
                x: 0.0,
                y: 0.10,
                w: 1.0,
                h: 0.85,
            };

            if let Ok(Some(pos)) = self.sidecar().find_element(None, &servant_key, region, 0.8) {
                self.emit(
                    "ServantSelect",
                    &format!("找到从者 {}，点击选择", slot_cfg.servant_id),
                );
                if !self.tap_at("ServantSelect", pos) {
                    return;
                }
                self.servants_placed.push(slot_cfg.slot_index);
                thread::sleep(ACTION_DELAY);
            } else {
                self.emit(
                    "ServantSelect",
                    &format!("搜索从者 {}…", slot_cfg.servant_id),
                );
                if !self.swipe_at(
                    "ServantSelect",
                    Point::new(0.50, 0.70),
                    Point::new(0.50, 0.30),
                    300,
                ) {
                    return;
                }
                thread::sleep(ACTION_DELAY);
            }
        } else {
            self.emit("ServantSelect", "所有从者已选择，返回");
            if !self.tap_at("ServantSelect", Point::new(0.05, 0.05)) {
                return;
            }
            thread::sleep(ACTION_DELAY);
        }
    }

    // -- battle screen handlers ----------------------------------------------

    fn handle_battle(&mut self) {
        // Check if the attack button is present (our turn to act)
        let attack_present = self
            .sidecar()
            .find_element_by_name(None, BATTLE_SCREEN, ATTACK_BUTTON_ELEMENT)
            .map(|m| m.found)
            .unwrap_or(false);

        if !attack_present {
            self.emit("Battle", "等待战斗动作…");
            return;
        }

        // Attack button is back -- the prior NP / attack cinematic (if
        // any) has finished. Drop back to the short Unknown tolerance.
        self.battle.waiting_for_battle = false;
        self.battle.attack_submitted = false;

        // Read the current battle scene (m of n) from the BATTLE label HUD.
        let screen_scene = self
            .sidecar()
            .read_battle_scene(None, BATTLE_SCENE_REGION)
            .unwrap_or(None);
        let scene_m = screen_scene.map(|(m, _)| m);

        // Decide what to do based on (prior scene, fresh read). See
        // `tick_scene_state` for the full state-transition rules; the
        // key invariant is that a failed CV read (`scene_m == None`)
        // never advances the index, never re-locks `last_screen_scene`,
        // and never re-fires skills for an already-executed scene.
        let tick = tick_scene_state(
            self.battle.last_screen_scene,
            self.battle.current_scene_index,
            self.battle.executed_scene_index,
            scene_m,
        );
        self.battle.current_scene_index = tick.current_scene_index;
        self.battle.last_screen_scene = tick.last_screen_scene;

        if tick.needs_exec {
            self.battle.scene_config_used = false;
            let scene_str = match screen_scene {
                Some((m, n)) => format!("{m}/{n}"),
                None => "?".into(),
            };
            self.emit(
                "Battle",
                &format!(
                    "执行第 {} 组指令 (画面场景: {})",
                    self.battle.current_scene_index + 1,
                    scene_str,
                ),
            );
            if self.advanced_mode {
                self.battle.scene_config_used = self
                    .advanced_scenes
                    .get(self.battle.current_scene_index)
                    .is_some();
            } else if let Some(scene_cfg) =
                self.scenes.get(self.battle.current_scene_index).cloned()
            {
                self.execute_scene_skills(&scene_cfg);
                self.battle.scene_config_used = true;
            } else {
                self.emit(
                    "Battle",
                    &format!(
                        "无第 {} 组指令配置，直接攻击",
                        self.battle.current_scene_index + 1
                    ),
                );
            }
            self.battle.executed_scene_index = Some(self.battle.current_scene_index);
        } else {
            self.emit("Battle", "场景未变更，直接攻击");
        }

        if !self.advanced_mode {
            if let Some(scene_cfg) = self.scenes.get(self.battle.current_scene_index).cloned() {
                self.select_enemy_target(scene_cfg.enemy_target.as_deref());
            }
        }

        // Click the attack button
        self.emit("Battle", "点击攻击按钮");
        if !self.tap_at("Battle", ATTACK_BUTTON) {
            return;
        }
        thread::sleep(ACTION_DELAY);
    }

    /// Build the front-line ``[party_slot_0, party_slot_1, party_slot_2]``
    /// id map. Player-configured slots take precedence; any remaining
    /// front-line slot is assumed to be the support (its id parsed out of
    /// ``support_servant_name`` when the user pinned a specific servant).
    fn build_party_ids(&self) -> [Option<u32>; 3] {
        let full = self.build_full_party_ids();
        [full[0], full[1], full[2]]
    }

    fn build_full_party_ids(&self) -> [Option<u32>; 6] {
        let mut full: [Option<u32>; 6] = [None, None, None, None, None, None];
        for sel in &self.config.servant_selections {
            let slot = sel.slot_index as usize;
            if slot < 6 {
                full[slot] = Some(sel.servant_id);
            }
        }

        let support_id = self.config.support_servant_id.or_else(|| {
            self.config
                .support_servant_name
                .as_deref()
                .and_then(parse_servant_name_id)
        });
        if let Some(support_id) = support_id {
            if let Some(slot) = self
                .config
                .support_slot_index
                .and_then(|slot| usize::try_from(slot).ok())
                .filter(|slot| *slot < full.len())
            {
                full[slot] = Some(support_id);
            } else {
                for slot in full.iter_mut().take(3) {
                    if slot.is_none() {
                        *slot = Some(support_id);
                        break;
                    }
                }
            }
        }
        full
    }

    fn normal_current_party_ids(&self) -> [Option<u32>; 3] {
        normal_current_party_ids_from(
            self.build_full_party_ids(),
            &self.scenes,
            self.battle.current_scene_index,
            self.battle.executed_scene_index,
        )
    }

    fn grand_servant_runtime_configs(&self) -> Vec<GrandServantRuntimeConfig> {
        let full = self.build_full_party_ids();
        let mut seen = HashSet::new();
        self.config
            .grand_servants
            .iter()
            .filter_map(|config| {
                let slot = usize::try_from(config.slot_index).ok()?;
                let servant_id = full.get(slot).copied().flatten()?;
                if !seen.insert(servant_id) {
                    return None;
                }
                Some(GrandServantRuntimeConfig {
                    slot_index: slot,
                    servant_id,
                    np_card: config.np_card.clone(),
                    priority: config.priority.clone(),
                })
            })
            .take(2)
            .collect()
    }

    fn advanced_party_ids_after_control(
        &self,
        scene: &AdvancedBattleScene,
        control_count: usize,
    ) -> [Option<u32>; 3] {
        self.advanced_party_ids_after_actions(scene.control_actions.iter().take(control_count))
    }

    fn advanced_party_ids_after_startup_flow(
        &self,
        scene: &AdvancedBattleScene,
        control_count: usize,
        startup_control_count: usize,
    ) -> [Option<u32>; 3] {
        let actions = advanced_startup_flow_actions(
            scene,
            control_count,
            startup_control_count,
            self.battle
                .advanced_auto_order_changes
                .get(&self.battle.current_scene_index),
        );
        self.advanced_party_ids_after_actions(actions.iter())
    }

    fn advanced_party_ids_after_actions<'a>(
        &self,
        actions: impl Iterator<Item = &'a Action>,
    ) -> [Option<u32>; 3] {
        let mut ids = self.build_full_party_ids();
        for action in actions {
            if action_frontline_available(&ids, action) {
                apply_party_lineup_change(&mut ids, action);
            }
        }
        [ids[0], ids[1], ids[2]]
    }

    fn advanced_actions_and_party_after(
        &self,
        already_executed: impl Iterator<Item = Action>,
        pending: impl Iterator<Item = Action>,
    ) -> (Vec<Action>, [Option<u32>; 3]) {
        let mut ids = self.build_full_party_ids();
        for action in already_executed {
            if action_frontline_available(&ids, &action) {
                apply_party_lineup_change(&mut ids, &action);
            }
        }

        let mut actions = Vec::new();
        for action in pending {
            if !action_frontline_available(&ids, &action) {
                self.emit(
                    "Battle",
                    &format!("跳过行动：{} 不在前排", action_frontline_label(&action)),
                );
                continue;
            }
            apply_party_lineup_change(&mut ids, &action);
            actions.push(action);
        }
        (actions, [ids[0], ids[1], ids[2]])
    }

    fn handle_attack(&mut self) {
        if self.advanced_mode {
            let party_ids = self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .filter(|scene| scene.rules.is_empty() || uses_advanced_strategy_flow(scene))
                .map(|scene| self.advanced_current_party_ids(scene))
                .unwrap_or_else(|| self.build_party_ids());
            let Some((cards, nps)) = self.read_attack_state(&party_ids) else {
                return;
            };
            self.handle_advanced_attack(cards, nps, party_ids);
            return;
        }

        let party_ids = self.normal_current_party_ids();
        let Some((cards, nps)) = self.read_attack_state(&party_ids) else {
            return;
        };
        self.pick_and_tap_attack_cards(&cards, &nps, &party_ids, None);
    }

    fn advanced_current_party_ids(&self, scene: &AdvancedBattleScene) -> [Option<u32>; 3] {
        let scene_index = self.battle.current_scene_index;
        let executed_control_count = *self
            .battle
            .advanced_control_indices
            .get(&scene_index)
            .unwrap_or(&0);
        if self.battle.advanced_startup_done.contains(&scene_index) {
            let startup_control_count = *self
                .battle
                .advanced_startup_control_indices
                .get(&scene_index)
                .unwrap_or(&executed_control_count);
            self.advanced_party_ids_after_startup_flow(
                scene,
                executed_control_count,
                startup_control_count,
            )
        } else {
            self.advanced_party_ids_after_control(scene, executed_control_count)
        }
    }

    fn read_attack_state(
        &mut self,
        party_ids: &[Option<u32>; 3],
    ) -> Option<(Vec<CommandCardMatch>, Vec<NoblePhantasmMatch>)> {
        // Unique candidate set, stable by party position.
        let mut candidate_ids: Vec<u32> = Vec::with_capacity(3);
        for id in party_ids.iter().flatten() {
            if !candidate_ids.contains(id) {
                candidate_ids.push(*id);
            }
        }
        self.emit("Attack", &format!("指令卡候选从者: {:?}", candidate_ids));

        if self.assets_dir.is_none() {
            self.emit("Attack", "未找到从者资源目录，将无法按从者匹配指令卡");
        }

        let assets_dir = self.assets_dir.clone();
        let cards = loop {
            let cards = match self.sidecar().find_command_cards(
                None,
                None,
                &candidate_ids,
                assets_dir.as_deref(),
            ) {
                Ok(c) => c,
                Err(err) => {
                    self.fail_action("Attack", "识别指令卡", err);
                    return None;
                }
            };
            if !should_retry_command_card_owner_detection(&cards, &candidate_ids) {
                break cards;
            }
            if self.is_cancelled() {
                return None;
            }
            self.emit("Attack", "指令卡从者未识别，等待卡面稳定后重试");
            thread::sleep(ACTION_DELAY);
        };

        let nps = match self.sidecar().find_noble_phantasms(None, None) {
            Ok(n) => n,
            Err(err) => {
                self.fail_action("Attack", "识别宝具卡", err);
                return None;
            }
        };

        let card_summary: Vec<String> = cards
            .iter()
            .map(|c| {
                let owner = c
                    .servant_id
                    .and_then(|id| {
                        party_ids
                            .iter()
                            .position(|party_id| *party_id == Some(id))
                            .map(|index| format!("S{}:{id}", index + 1))
                            .or_else(|| Some(format!("?:{id}")))
                    })
                    .unwrap_or_else(|| "未识别".into());
                format!(
                    "C{}={}/{}",
                    c.slot + 1,
                    c.suit.as_deref().unwrap_or("?"),
                    owner,
                )
            })
            .collect();
        self.emit("Attack", &format!("指令卡: {}", card_summary.join(" ")));
        let ready: Vec<String> = nps
            .iter()
            .filter(|n| n.ready)
            .map(|n| format!("NP{}", n.slot + 1))
            .collect();
        if ready.is_empty() {
            self.emit("Attack", "宝具就绪: 无");
        } else {
            self.emit("Attack", &format!("宝具就绪: {}", ready.join(" ")));
        }

        Some((cards, nps))
    }

    fn pick_and_tap_attack_cards(
        &mut self,
        cards: &[CommandCardMatch],
        nps: &[NoblePhantasmMatch],
        party_ids: &[Option<u32>; 3],
        attack_priority_override: Option<&[AttackCard]>,
    ) {
        let mut used_card_slots: HashSet<u32> = HashSet::new();
        let mut used_np_slots: HashSet<u32> = HashSet::new();
        let mut picks: Vec<Pick> = Vec::with_capacity(3);

        if let Some(priority) = attack_priority_override {
            picks = pick_by_priority(
                priority,
                cards,
                nps,
                party_ids,
                &mut used_card_slots,
                &mut used_np_slots,
            );
        } else if let Some(priority) = attack_priority_for_current_scene(
            self.advanced_mode,
            self.battle.scene_config_used,
            &self.scenes,
            self.battle.current_scene_index,
        ) {
            picks = pick_by_priority(
                priority,
                cards,
                nps,
                party_ids,
                &mut used_card_slots,
                &mut used_np_slots,
            );
        } else {
            self.emit("Attack", "场景未变更，按默认顺序补位");
        }

        if picks.len() < 3 {
            fill_remaining(&mut picks, &cards, &mut used_card_slots);
        }

        if picks.is_empty() {
            self.emit("Attack", "未能选出任何卡，跳过");
            self.battle.scene_config_used = false;
            return;
        }

        self.tap_picks("Attack", &picks);
    }

    fn tap_picks(&mut self, screen: &str, picks: &[Pick]) {
        if picks.is_empty() {
            self.emit(screen, "未能选出任何卡，跳过");
            self.battle.scene_config_used = false;
            return;
        }

        for (i, pick) in picks.iter().enumerate() {
            let (msg, point) = match pick {
                Pick::Card {
                    slot,
                    point,
                    servant_id,
                    suit,
                    from_priority,
                } => {
                    let label = match from_priority {
                        Some(p) => format!("选择 {p} → C{}", slot + 1),
                        None => format!("补位: C{}", slot + 1),
                    };
                    let detail = format!(
                        " ({}{})",
                        suit.as_deref().unwrap_or("?"),
                        servant_id.map(|id| format!("/{id}")).unwrap_or_default(),
                    );
                    (
                        format!("{}/{} {}{}", i + 1, picks.len(), label, detail),
                        *point,
                    )
                }
                Pick::Np {
                    slot,
                    point,
                    from_priority,
                } => (
                    format!(
                        "{}/{} 选择 {} → NP{}",
                        i + 1,
                        picks.len(),
                        from_priority,
                        slot + 1,
                    ),
                    *point,
                ),
            };
            self.emit(screen, &msg);
            if !self.tap_at(screen, point) {
                return;
            }
            thread::sleep(ACTION_DELAY);
        }

        // The 3 cards (especially when an NP is included) trigger a long
        // attack cinematic before the battle screen comes back. While that
        // animation plays the screen classifier returns Unknown, so flag
        // the loop to use the longer Unknown tolerance until handle_battle
        // sees a real Battle frame again.
        self.battle.waiting_for_battle = true;
        self.battle.attack_submitted = true;

        // Reset for next cycle
        self.battle.scene_config_used = false;
    }

    fn handle_advanced_attack(
        &mut self,
        cards: Vec<CommandCardMatch>,
        nps: Vec<NoblePhantasmMatch>,
        party_ids: [Option<u32>; 3],
    ) {
        let Some(scene) = self
            .advanced_scenes
            .get(self.battle.current_scene_index)
            .cloned()
        else {
            self.emit("Attack", "无高级指令配置，按默认顺序补位");
            self.pick_and_tap_attack_cards(&cards, &nps, &party_ids, None);
            return;
        };
        let grand_servants = self.grand_servant_runtime_configs();

        if scene.rules.is_empty() || uses_advanced_strategy_flow(&scene) {
            let scene_index = self.battle.current_scene_index;
            let executed_control_count = *self
                .battle
                .advanced_control_indices
                .get(&scene_index)
                .unwrap_or(&0);
            let current_party_ids = if self.battle.advanced_startup_done.contains(&scene_index) {
                let startup_control_count = *self
                    .battle
                    .advanced_startup_control_indices
                    .get(&scene_index)
                    .unwrap_or(&executed_control_count);
                self.advanced_party_ids_after_startup_flow(
                    &scene,
                    executed_control_count,
                    startup_control_count,
                )
            } else {
                self.advanced_party_ids_after_control(&scene, executed_control_count)
            };
            let (cards, nps, party_ids) = if current_party_ids != party_ids {
                let Some((cards, nps)) = self.read_attack_state(&current_party_ids) else {
                    return;
                };
                (cards, nps, current_party_ids)
            } else {
                (cards, nps, party_ids)
            };

            if !self.battle.advanced_startup_done.contains(&scene_index) {
                if scene.grand_auto_order_change == Some(true) {
                    let auto_order_change =
                        grand_auto_order_change_action(&cards, &party_ids, &grand_servants);
                    let original_ids = self.build_full_party_ids();
                    let mut ids = original_ids;
                    let mut startup_actions = Vec::new();
                    if let Some(action) = auto_order_change.clone() {
                        self.emit("Attack", "启动条件：自动将后排主冠位换至前排");
                        if action_frontline_available(&ids, &action) {
                            apply_party_lineup_change(&mut ids, &action);
                            startup_actions.push(action.clone());
                            self.battle
                                .advanced_auto_order_changes
                                .insert(scene_index, action);
                        } else {
                            self.emit("Battle", "跳过自动换位：目标不在可交换位置");
                        }
                    } else {
                        self.emit(
                            "Attack",
                            "启动条件：主冠位不需要或无法自动换位，直接进入启动阶段",
                        );
                    }
                    let next_control_count = if executed_control_count < scene.control_actions.len()
                    {
                        executed_control_count + 1
                    } else {
                        executed_control_count
                    };
                    if executed_control_count < scene.control_actions.len() {
                        self.emit(
                            "Attack",
                            &format!("启动阶段执行本回合控制行动 {next_control_count}"),
                        );
                    }
                    let pending_actions = scene
                        .control_actions
                        .iter()
                        .skip(executed_control_count)
                        .take(next_control_count.saturating_sub(executed_control_count))
                        .chain(scene.startup_actions.iter())
                        .cloned();
                    for action in pending_actions {
                        let Some(resolved_action) =
                            resolve_action_to_current_positions(&ids, &original_ids, &action)
                        else {
                            self.emit(
                                "Battle",
                                &format!(
                                    "跳过行动：{} 不在当前可用位置",
                                    action_frontline_label(&action)
                                ),
                            );
                            continue;
                        };
                        if !action_frontline_available(&ids, &resolved_action) {
                            self.emit(
                                "Battle",
                                &format!("跳过行动：{} 不在前排", action_frontline_label(&action)),
                            );
                            continue;
                        }
                        apply_party_lineup_change(&mut ids, &resolved_action);
                        startup_actions.push(resolved_action);
                    }
                    let startup_party_ids = [ids[0], ids[1], ids[2]];
                    self.battle
                        .advanced_control_indices
                        .insert(scene_index, next_control_count);
                    self.battle
                        .advanced_startup_control_indices
                        .insert(scene_index, next_control_count);
                    self.battle.advanced_startup_done.insert(scene_index);
                    if !startup_actions.is_empty() {
                        if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                        if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                            return;
                        }
                        let prep_scene = BattleScene {
                            id: scene.id.clone(),
                            preparation_actions: startup_actions,
                            servant_actions: Vec::new(),
                            equipment_actions: Vec::new(),
                            command_spell_actions: Vec::new(),
                            enemy_target: None,
                            attack_priority: Vec::new(),
                        };
                        self.execute_scene_skills(&prep_scene);

                        self.emit("Battle", "启动阶段完成，进入自动战斗");
                        if !self.tap_at("Battle", ATTACK_BUTTON) {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                        let Some((next_cards, next_nps)) =
                            self.read_attack_state(&startup_party_ids)
                        else {
                            return;
                        };
                        let picks = choose_advanced_auto_picks(
                            &scene,
                            &next_cards,
                            &next_nps,
                            &startup_party_ids,
                            &grand_servants,
                            &self.config.grand_card_strategy,
                        );
                        self.tap_picks("Attack", &picks);
                        return;
                    }

                    let picks = choose_advanced_auto_picks(
                        &scene,
                        &cards,
                        &nps,
                        &startup_party_ids,
                        &grand_servants,
                        &self.config.grand_card_strategy,
                    );
                    self.tap_picks("Attack", &picks);
                    return;
                }

                if !advanced_startup_conditions_match(&scene, &cards, &party_ids) {
                    let control_index = executed_control_count;
                    if let Some(control_action) = scene.control_actions.get(control_index).cloned()
                    {
                        self.emit(
                            "Attack",
                            &format!("启动条件未满足，执行控制行动 {}", control_index + 1),
                        );
                        if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                        if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                            return;
                        }
                        let control_scene = BattleScene {
                            id: format!("{}_control_{}", scene.id, control_index + 1),
                            preparation_actions: vec![control_action],
                            servant_actions: Vec::new(),
                            equipment_actions: Vec::new(),
                            command_spell_actions: Vec::new(),
                            enemy_target: None,
                            attack_priority: Vec::new(),
                        };
                        self.execute_scene_skills(&control_scene);
                        self.battle
                            .advanced_control_indices
                            .insert(scene_index, control_index + 1);

                        self.emit("Battle", "控制行动完成，返回指令卡攻击");
                        if !self.tap_at("Battle", ATTACK_BUTTON) {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                        let control_party_ids =
                            self.advanced_party_ids_after_control(&scene, control_index + 1);
                        let Some((next_cards, _next_nps)) =
                            self.read_attack_state(&control_party_ids)
                        else {
                            return;
                        };
                        let picks = choose_advanced_auto_picks(
                            &scene,
                            &next_cards,
                            &[],
                            &control_party_ids,
                            &grand_servants,
                            &self.config.grand_card_strategy,
                        );
                        self.tap_picks("Attack", &picks);
                        return;
                    }

                    self.emit("Attack", "启动条件未满足，按自动优先级攻击且不释放宝具");
                    let picks = choose_advanced_auto_picks(
                        &scene,
                        &cards,
                        &[],
                        &party_ids,
                        &grand_servants,
                        &self.config.grand_card_strategy,
                    );
                    self.tap_picks("Attack", &picks);
                    return;
                }

                self.emit("Attack", "启动条件满足，进入启动阶段");
                let next_control_count = if executed_control_count < scene.control_actions.len() {
                    executed_control_count + 1
                } else {
                    executed_control_count
                };
                if executed_control_count < scene.control_actions.len() {
                    self.emit(
                        "Attack",
                        &format!("启动阶段执行本回合控制行动 {next_control_count}"),
                    );
                }
                let pending_actions = scene
                    .control_actions
                    .iter()
                    .skip(executed_control_count)
                    .take(next_control_count.saturating_sub(executed_control_count))
                    .chain(scene.startup_actions.iter())
                    .cloned();
                let (startup_actions, startup_party_ids) = self.advanced_actions_and_party_after(
                    scene
                        .control_actions
                        .iter()
                        .take(executed_control_count)
                        .cloned(),
                    pending_actions,
                );
                self.battle
                    .advanced_control_indices
                    .insert(scene_index, next_control_count);
                self.battle
                    .advanced_startup_control_indices
                    .insert(scene_index, next_control_count);
                self.battle.advanced_startup_done.insert(scene_index);
                if !startup_actions.is_empty() {
                    if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                        return;
                    }
                    let prep_scene = BattleScene {
                        id: scene.id.clone(),
                        preparation_actions: startup_actions,
                        servant_actions: Vec::new(),
                        equipment_actions: Vec::new(),
                        command_spell_actions: Vec::new(),
                        enemy_target: None,
                        attack_priority: Vec::new(),
                    };
                    self.execute_scene_skills(&prep_scene);

                    self.emit("Battle", "启动阶段完成，进入自动战斗");
                    if !self.tap_at("Battle", ATTACK_BUTTON) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    let Some((next_cards, next_nps)) = self.read_attack_state(&startup_party_ids)
                    else {
                        return;
                    };
                    let picks = choose_advanced_auto_picks(
                        &scene,
                        &next_cards,
                        &next_nps,
                        &startup_party_ids,
                        &grand_servants,
                        &self.config.grand_card_strategy,
                    );
                    self.tap_picks("Attack", &picks);
                    return;
                }

                let picks = choose_advanced_auto_picks(
                    &scene,
                    &cards,
                    &nps,
                    &startup_party_ids,
                    &grand_servants,
                    &self.config.grand_card_strategy,
                );
                self.tap_picks("Attack", &picks);
                return;
            }

            let executed_control_count = *self
                .battle
                .advanced_control_indices
                .get(&self.battle.current_scene_index)
                .unwrap_or(&0);
            let startup_control_count = *self
                .battle
                .advanced_startup_control_indices
                .get(&self.battle.current_scene_index)
                .unwrap_or(&executed_control_count);
            if executed_control_count < scene.control_actions.len() {
                let active_actions = advanced_startup_flow_actions(
                    &scene,
                    executed_control_count,
                    startup_control_count,
                    self.battle
                        .advanced_auto_order_changes
                        .get(&self.battle.current_scene_index),
                );
                let (control_actions, control_party_ids) = self.advanced_actions_and_party_after(
                    active_actions.into_iter(),
                    scene
                        .control_actions
                        .iter()
                        .skip(executed_control_count)
                        .take(1)
                        .cloned(),
                );
                let next_control_count = executed_control_count + 1;
                self.battle
                    .advanced_control_indices
                    .insert(self.battle.current_scene_index, next_control_count);

                if !control_actions.is_empty() {
                    self.emit(
                        "Attack",
                        &format!("自动战斗执行本回合控制行动 {next_control_count}"),
                    );
                    if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                        return;
                    }
                    let control_scene = BattleScene {
                        id: format!("{}_auto_control_{}", scene.id, next_control_count),
                        preparation_actions: control_actions,
                        servant_actions: Vec::new(),
                        equipment_actions: Vec::new(),
                        command_spell_actions: Vec::new(),
                        enemy_target: None,
                        attack_priority: Vec::new(),
                    };
                    self.execute_scene_skills(&control_scene);

                    self.emit("Battle", "控制行动完成，返回指令卡攻击");
                    if !self.tap_at("Battle", ATTACK_BUTTON) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    let Some((next_cards, next_nps)) = self.read_attack_state(&control_party_ids)
                    else {
                        return;
                    };
                    let picks = choose_advanced_auto_picks(
                        &scene,
                        &next_cards,
                        &next_nps,
                        &control_party_ids,
                        &grand_servants,
                        &self.config.grand_card_strategy,
                    );
                    self.tap_picks("Attack", &picks);
                    return;
                }

                let picks = choose_advanced_auto_picks(
                    &scene,
                    &cards,
                    &nps,
                    &control_party_ids,
                    &grand_servants,
                    &self.config.grand_card_strategy,
                );
                self.tap_picks("Attack", &picks);
                return;
            }
            let active_party_ids = self.advanced_party_ids_after_startup_flow(
                &scene,
                executed_control_count,
                startup_control_count,
            );
            let picks = choose_advanced_auto_picks(
                &scene,
                &cards,
                &nps,
                &active_party_ids,
                &grand_servants,
                &self.config.grand_card_strategy,
            );
            self.tap_picks("Attack", &picks);
            return;
        }

        let mut cards = cards;
        let mut nps = nps;
        let mut next_rule_index = 0usize;
        let mut guard = 0usize;
        while next_rule_index < scene.rules.len() && guard <= scene.rules.len() {
            guard += 1;
            let Some((rule_index, rule)) = scene
                .rules
                .iter()
                .enumerate()
                .skip(next_rule_index)
                .find(|(_, rule)| advanced_rule_matches(rule, &cards, &nps, &party_ids))
                .map(|(index, rule)| (index, rule.clone()))
            else {
                break;
            };

            let prep_actions: Vec<Action> = rule
                .actions
                .iter()
                .filter_map(|action| action.as_preparation_action())
                .collect();
            let attack_priority: Vec<AttackCard> = rule
                .actions
                .iter()
                .filter_map(|action| action.as_attack_card())
                .collect();

            if !prep_actions.is_empty() {
                self.emit(
                    "Attack",
                    &format!("高级规则 {} 命中，返回执行准备行动", rule_index + 1),
                );
                if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                    return;
                }
                thread::sleep(ACTION_DELAY);
                if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                    return;
                }
                let prep_scene = BattleScene {
                    id: rule.id.clone(),
                    preparation_actions: prep_actions,
                    servant_actions: Vec::new(),
                    equipment_actions: Vec::new(),
                    command_spell_actions: Vec::new(),
                    enemy_target: None,
                    attack_priority: Vec::new(),
                };
                self.execute_scene_skills(&prep_scene);

                self.emit("Battle", "高级规则准备行动完成，重新进入指令卡");
                if !self.tap_at("Battle", ATTACK_BUTTON) {
                    return;
                }
                thread::sleep(ACTION_DELAY);
                let Some((next_cards, next_nps)) = self.read_attack_state(&party_ids) else {
                    return;
                };
                cards = next_cards;
                nps = next_nps;
                if !attack_priority.is_empty() {
                    self.emit(
                        "Attack",
                        &format!("高级规则 {} 准备后执行攻击", rule_index + 1),
                    );
                    self.pick_and_tap_attack_cards(
                        &cards,
                        &nps,
                        &party_ids,
                        Some(&attack_priority),
                    );
                    return;
                }
                next_rule_index = rule_index + 1;
                continue;
            }

            if !attack_priority.is_empty() {
                self.emit(
                    "Attack",
                    &format!("高级规则 {} 命中，执行攻击", rule_index + 1),
                );
                self.pick_and_tap_attack_cards(&cards, &nps, &party_ids, Some(&attack_priority));
                return;
            }

            next_rule_index = rule_index + 1;
        }

        self.emit("Attack", "无高级规则命中攻击，按默认顺序补位");
        self.pick_and_tap_attack_cards(&cards, &nps, &party_ids, None);
    }

    // -- battle-result screen handlers ---------------------------------------

    fn handle_battle_result_bond(&mut self) {
        self.emit("BattleResultBond", "羁绊点数结算，前往下一画面");
        self.tap_until_screen_changes(
            "BattleResultBond",
            Screen::BattleResultBond,
            BATTLE_RESULT_BOND_NEXT,
            BATTLE_RESULT_TAP_INTERVAL,
            BATTLE_RESULT_TAP_TIMEOUT,
        );
    }

    fn handle_battle_result_exp(&mut self) {
        self.emit("BattleResultExp", "经验结算，前往下一画面");
        self.tap_until_screen_changes(
            "BattleResultExp",
            Screen::BattleResultExp,
            BATTLE_RESULT_EXP_NEXT,
            BATTLE_RESULT_TAP_INTERVAL,
            BATTLE_RESULT_TAP_TIMEOUT,
        );
    }

    fn handle_battle_result_loot(&mut self) {
        self.emit("BattleResultLoot", "掉落结算，前往下一画面");
        if self.tap_at("BattleResultLoot", BATTLE_RESULT_LOOT_NEXT) {
            thread::sleep(ACTION_DELAY);
        }
    }

    fn handle_battle_result_friend_request(&mut self) {
        self.emit("BattleResultFriendRequest", "跳过好友申请");
        if self.tap_at("BattleResultFriendRequest", BATTLE_RESULT_FRIEND_SKIP) {
            thread::sleep(ACTION_DELAY);
        }
    }

    /// Final continue page. Branches on `RunConfig::repeat_mission`:
    ///
    /// * `true` — tap "Next" so FGO re-queues the same quest. Per-run
    ///   bookkeeping (team-change flag, battle turn counters, placed-
    ///   servant set) is reset so the next loop reuses pre-battle handlers
    ///   from a clean slate. The pinned support metadata is intentionally
    ///   kept since the same servant is still desired.
    /// * `false` — tap "Close" and transition to `Finished`. The main
    ///   loop's post-handler check exits cleanly so the user sees the
    ///   "已完成" toast.
    fn handle_battle_result_continue(&mut self) {
        if self.battle_result_continue_handled {
            self.emit("BattleResultContinue", "等待结算页切换…");
            thread::sleep(ACTION_DELAY);
            return;
        }
        self.battle_result_continue_handled = true;

        self.completed_mission_runs += 1;
        let reached_run_cap = self
            .config
            .max_mission_runs
            .is_some_and(|max| self.completed_mission_runs >= max);
        let should_repeat = self.config.max_mission_runs.is_some() || self.config.repeat_mission;

        if should_repeat && !reached_run_cap && !self.should_stop_after_current() {
            self.emit("BattleResultContinue", "继续重复任务");
            if !self.tap_at("BattleResultContinue", BATTLE_RESULT_CONTINUE_REPEAT) {
                return;
            }
            self.team_changed = false;
            self.servants_placed.clear();
            self.battle = BattleState::new();
            self.battle.waiting_for_battle = true;
            thread::sleep(ACTION_DELAY);
        } else {
            self.emit(
                "BattleResultContinue",
                &format!("结束任务（已完成 {} 轮）", self.completed_mission_runs),
            );
            if !self.tap_at("BattleResultContinue", BATTLE_RESULT_CONTINUE_STOP) {
                return;
            }
            self.set_state(RunnerState::Finished);
        }
    }

    fn ap_recovery_candidates(&self, page: ApRecoveryPage) -> Vec<ApRecoveryTemplate> {
        ap_recovery_candidates_for_page(&self.config.ap_recovery_items, page)
    }

    fn find_ap_recovery_item(&mut self, item: ApRecoveryTemplate) -> Result<Option<Point>, String> {
        self.sidecar().find_element(
            None,
            item.template_key,
            AP_RECOVERY_ITEMS_REGION,
            AP_RECOVERY_ITEM_THRESHOLD,
        )
    }

    fn ap_recovery_list_visible(&mut self) -> Result<bool, String> {
        Ok(self
            .sidecar()
            .find_element(
                None,
                AP_RECOVERY_LIST_LABEL_TEMPLATE,
                AP_RECOVERY_ITEMS_REGION,
                0.8,
            )?
            .is_some())
    }

    fn handle_ap_recovery(&mut self) {
        if self.config.ap_recovery_items.is_empty() {
            self.emit("APRecovery", "行动力不足且未配置自动吃苹果，停止");
            self.set_state(RunnerState::Finished);
            return;
        }

        match self.ap_recovery_list_visible() {
            Ok(true) => {}
            Ok(false) => {
                self.emit("APRecovery", "未定位到道具列表，等待重试…");
                thread::sleep(ACTION_DELAY);
                return;
            }
            Err(err) => {
                self.fail_action("APRecovery", "定位道具列表", err);
                return;
            }
        }

        for item in self.ap_recovery_candidates(ApRecoveryPage::Top) {
            match self.find_ap_recovery_item(item) {
                Ok(Some(point)) => {
                    self.emit("APRecovery", &format!("行动力不足，使用{}", item.label));
                    if !self.tap_at("APRecovery", point) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    if !self.tap_at("APRecovery", AP_RECOVERY_CONFIRM_BUTTON) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    return;
                }
                Ok(None) => {}
                Err(err) => {
                    self.fail_action("APRecovery", &format!("识别{}", item.label), err);
                    return;
                }
            }
        }

        let bottom_items = self.ap_recovery_candidates(ApRecoveryPage::Bottom);
        if bottom_items.is_empty() {
            self.emit("APRecovery", "已配置苹果数量不足，停止");
            self.set_state(RunnerState::Finished);
            return;
        }

        self.emit("APRecovery", "上半页未找到可用道具，滚动到底部继续查找");
        if !self.swipe_at(
            "APRecovery",
            AP_RECOVERY_SCROLL_FROM,
            AP_RECOVERY_SCROLL_TO,
            350,
        ) {
            return;
        }
        thread::sleep(ACTION_DELAY);

        for item in bottom_items {
            match self.find_ap_recovery_item(item) {
                Ok(Some(point)) => {
                    self.emit("APRecovery", &format!("行动力不足，使用{}", item.label));
                    if !self.tap_at("APRecovery", point) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    if !self.tap_at("APRecovery", AP_RECOVERY_CONFIRM_BUTTON) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    return;
                }
                Ok(None) => {}
                Err(err) => {
                    self.fail_action("APRecovery", &format!("识别{}", item.label), err);
                    return;
                }
            }
        }

        self.emit("APRecovery", "所有已配置苹果数量不足，停止");
        self.set_state(RunnerState::Finished);
    }

    // -- skill execution -----------------------------------------------------

    fn execute_scene_skills(&mut self, scene: &BattleScene) {
        for action in scene_preparation_actions(scene) {
            match action {
                Action::Servant {
                    servant,
                    skill,
                    target,
                    ..
                } => {
                    let Some(pos) = skill_position(servant.as_deref(), skill.as_deref()) else {
                        continue;
                    };

                    self.emit(
                        "Battle",
                        &format!(
                            "从者技能: {} 使用 {}",
                            servant.as_deref().unwrap_or("?"),
                            skill.as_deref().unwrap_or("?"),
                        ),
                    );
                    if !self.tap_at("Battle", pos) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);

                    if let Some(target_pos) = skill_target_position(target.as_deref()) {
                        self.emit(
                            "Battle",
                            &format!("选择目标: {}", target.as_deref().unwrap_or("?")),
                        );
                        if !self.tap_at("Battle", target_pos) {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                    }

                    self.skip_after_skill();

                    // Skill animation (cut-in, buff effect, etc.) hides the
                    // attack button. Wait for it to reappear before firing
                    // the next skill, otherwise rapid taps land on nothing
                    // or, worse, on whatever overlay is currently shown.
                    if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                        return;
                    }
                }
                Action::Equipment {
                    skill,
                    target,
                    order_change,
                    ..
                } => {
                    let Some(pos) = equipment_skill_position(skill.as_deref()) else {
                        continue;
                    };

                    self.emit("Battle", "打开御主技能面板");
                    if !self.tap_at("Battle", EQUIPMENT_BUTTON) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);

                    self.emit(
                        "Battle",
                        &format!("御主技能: {}", skill.as_deref().unwrap_or("?"),),
                    );
                    if !self.tap_at("Battle", pos) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);

                    if let Some(change) = order_change {
                        self.emit(
                            "Battle",
                            &format!("打开 Order Change: {}", skill.as_deref().unwrap_or("?")),
                        );
                        if !self.execute_order_change(change) {
                            return;
                        }
                    } else if let Some(target_pos) = skill_target_position(target.as_deref()) {
                        self.emit(
                            "Battle",
                            &format!("选择目标: {}", target.as_deref().unwrap_or("?")),
                        );
                        if !self.tap_at("Battle", target_pos) {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                    }

                    if order_change.is_none() {
                        self.skip_after_skill();
                    }

                    if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                        return;
                    }
                }
                // Command Spell (令咒) walks four full-screen modals:
                // button → spell row → 决定 confirm → ally target picker,
                // settling between each step because each tap pops or
                // pushes a modal.
                Action::CommandSpell { spell, target, .. } => {
                    let Some(option_idx) = command_spell_index(spell.as_deref()) else {
                        continue;
                    };
                    let Some(target_pos) = skill_target_position(target.as_deref()) else {
                        continue;
                    };

                    self.emit(
                        "Battle",
                        &format!("令咒: {}", spell.as_deref().unwrap_or("?")),
                    );
                    if !self.tap_at("Battle", COMMAND_SPELL_BUTTON) {
                        return;
                    }
                    thread::sleep(COMMAND_SPELL_DIALOG_SETTLE);

                    if !self.tap_at("Battle", COMMAND_SPELL_OPTIONS[option_idx]) {
                        return;
                    }
                    thread::sleep(COMMAND_SPELL_DIALOG_SETTLE);

                    self.emit("Battle", "确认令咒");
                    if !self.tap_at("Battle", COMMAND_SPELL_CONFIRM) {
                        return;
                    }
                    thread::sleep(COMMAND_SPELL_DIALOG_SETTLE);

                    self.emit(
                        "Battle",
                        &format!("令咒目标: {}", target.as_deref().unwrap_or("?")),
                    );
                    if !self.tap_at("Battle", target_pos) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);

                    self.skip_after_skill();

                    if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                        return;
                    }
                }
            }
        }
    }

    fn execute_order_change(&mut self, change: &crate::OrderChangeSelection) -> bool {
        let Some(front_pos) = order_change_slot_position(change.front.as_deref(), 0..3) else {
            self.emit(
                "Battle",
                &format!(
                    "Order Change 前排目标无效: {}，跳过",
                    change.front.as_deref().unwrap_or("?")
                ),
            );
            return true;
        };
        let Some(back_pos) = order_change_slot_position(change.back.as_deref(), 3..6) else {
            self.emit(
                "Battle",
                &format!(
                    "Order Change 后排目标无效: {}，跳过",
                    change.back.as_deref().unwrap_or("?")
                ),
            );
            return true;
        };

        thread::sleep(ORDER_CHANGE_PANEL_SETTLE);
        self.emit(
            "Battle",
            &format!(
                "Order Change 选择从者: {} ↔ {}",
                change.front.as_deref().unwrap_or("?"),
                change.back.as_deref().unwrap_or("?")
            ),
        );
        if !self.tap_at("Battle", front_pos) {
            return false;
        }
        thread::sleep(ACTION_DELAY);

        if !self.tap_at("Battle", back_pos) {
            return false;
        }
        thread::sleep(ACTION_DELAY);

        self.emit("Battle", "Order Change 点击进行更替");
        if !self.tap_at("Battle", ORDER_CHANGE_CONFIRM) {
            return false;
        }
        thread::sleep(ACTION_DELAY);
        true
    }

    /// Tap the in-game "skip animation" button so the cut-in / buff
    /// effect plays at full speed. The button is in the top-right corner
    /// and is safe to tap whether or not an animation is currently
    /// playing -- on a normal Battle frame this region is the turn-counter
    /// pill which has no interactive effect.
    fn skip_after_skill(&mut self) {
        let _ = self.tap_at("Battle", SKIP_ANIMATION_BUTTON);
        thread::sleep(ACTION_DELAY);
    }

    fn select_enemy_target(&mut self, target: Option<&str>) {
        let Some(point) = enemy_target_position(target) else {
            return;
        };
        self.emit(
            "Battle",
            &format!("选择敌方目标: {}", target.unwrap_or("?")),
        );
        if !self.tap_at("Battle", point) {
            return;
        }
        thread::sleep(ACTION_DELAY);
        self.skip_after_skill();
    }

    // -- utilities -----------------------------------------------------------

    fn next_unfilled_slot(&self) -> Option<ServantSlotConfig> {
        if !ENABLE_PARTY_SERVANT_AUTO_PLACEMENT {
            return None;
        }
        self.config
            .servant_selections
            .iter()
            .find(|s| !self.servants_placed.contains(&s.slot_index))
            .cloned()
    }
}

/// Build the runner's `TouchBackend`. Today there's only one
/// implementation, but the indirection through the trait makes adding
/// a faster transport (minitouch / sendevent / native helper) a
/// localized change later.
fn build_touch_backend(adb: &Adb) -> Box<dyn TouchBackend> {
    let backend = touch::build(adb);
    eprintln!("[touch] backend selected: {}", backend.name());
    backend
}

impl Drop for Runner {
    fn drop(&mut self) {
        // The touch backend's own Drop handles its cleanup. Today the
        // adb-input backend has nothing to clean up, but the trait
        // contract still lets a future backend (e.g. minitouch /
        // sendevent / native helper) release resources here.

        let Some(mut sidecar) = self.sidecar.take() else {
            return;
        };
        if let Err(err) = sidecar.stop_stream() {
            eprintln!("[mash-cv] stop_stream before caching runner sidecar failed: {err}");
        }
        if let Some(cache) = &self.sidecar_cache {
            let mut guard = cache.lock().unwrap();
            if guard.is_none() {
                *guard = Some(sidecar);
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Position mapping helpers
// ---------------------------------------------------------------------------

fn parse_index(s: &str, prefix: &str) -> Option<usize> {
    s.strip_prefix(prefix)
        .and_then(|n| n.parse::<usize>().ok())
        .map(|n| n.saturating_sub(1))
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangeOrderRule {
    servant_id: u32,
    trigger: ChangeOrderTrigger,
    effect: ChangeOrderEffect,
    #[serde(default)]
    timing: ChangeOrderTiming,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum ChangeOrderTiming {
    #[default]
    Immediate,
    EndOfTurn,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ChangeOrderTrigger {
    AttackCard {
        #[serde(rename = "card")]
        card: String,
        #[serde(default)]
        #[serde(rename = "activationUseCount")]
        activation_use_count: Option<u32>,
    },
    ServantSkill {
        skill: String,
    },
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ChangeOrderEffect {
    RemoveSelf,
    RemoveFirstAlly,
    WithdrawSelfToBack,
}

fn change_order_rules() -> &'static [ChangeOrderRule] {
    static RULES: OnceLock<Vec<ChangeOrderRule>> = OnceLock::new();
    RULES
        .get_or_init(|| {
            serde_json::from_str(include_str!("resources/change_order_servants.json"))
                .unwrap_or_default()
        })
        .as_slice()
}

fn compact_backline(ids: &mut [Option<u32>; 6]) {
    let backline: Vec<Option<u32>> = ids[3..]
        .iter()
        .copied()
        .filter(|slot| slot.is_some())
        .collect();
    for slot in 3..6 {
        ids[slot] = backline.get(slot - 3).copied().flatten();
    }
}

fn remove_party_slot(ids: &mut [Option<u32>; 6], index: usize) {
    if index >= 3 || ids[index].is_none() {
        return;
    }
    let replacement = (3..6).find(|slot| ids[*slot].is_some());
    ids[index] = replacement.and_then(|slot| ids[slot]);
    if let Some(slot) = replacement {
        ids[slot] = None;
    }
    compact_backline(ids);
}

fn withdraw_party_slot_to_back(ids: &mut [Option<u32>; 6], index: usize) {
    if index >= 3 {
        return;
    }
    let Some(servant_id) = ids[index] else {
        return;
    };
    compact_backline(ids);
    let Some(replacement) = (3..6).find(|slot| ids[*slot].is_some()) else {
        return;
    };
    ids[index] = ids[replacement];
    ids[replacement] = Some(servant_id);
}

fn apply_change_order_effect(
    ids: &mut [Option<u32>; 6],
    source_index: usize,
    effect: &ChangeOrderEffect,
) {
    match effect {
        ChangeOrderEffect::RemoveSelf => remove_party_slot(ids, source_index),
        ChangeOrderEffect::RemoveFirstAlly => {
            if let Some(target) =
                (0..3).find(|index| *index != source_index && ids[*index].is_some())
            {
                remove_party_slot(ids, target);
            }
        }
        ChangeOrderEffect::WithdrawSelfToBack => withdraw_party_slot_to_back(ids, source_index),
    }
}

fn apply_party_lineup_change(ids: &mut [Option<u32>; 6], action: &Action) {
    apply_party_lineup_change_at(ids, action, ChangeOrderTiming::Immediate);
}

fn apply_party_lineup_change_at(
    ids: &mut [Option<u32>; 6],
    action: &Action,
    timing: ChangeOrderTiming,
) {
    if timing == ChangeOrderTiming::EndOfTurn && !matches!(action, Action::Servant { .. }) {
        return;
    }

    if let Action::Equipment {
        order_change: Some(order_change),
        ..
    } = action
    {
        let Some(front) = order_change
            .front
            .as_deref()
            .and_then(|value| parse_index(value, "servant_"))
        else {
            return;
        };
        let Some(back) = order_change
            .back
            .as_deref()
            .and_then(|value| parse_index(value, "servant_"))
        else {
            return;
        };
        if front < 3 && back < 6 && ids[front].is_some() && ids[back].is_some() {
            ids.swap(front, back);
        }
        return;
    }

    let Action::Servant { servant, skill, .. } = action else {
        return;
    };
    let Some(source_index) = servant
        .as_deref()
        .and_then(|value| parse_index(value, "servant_"))
        .filter(|index| *index < 3)
    else {
        return;
    };
    let (Some(servant_id), Some(skill)) = (ids[source_index], skill.as_deref()) else {
        return;
    };
    for rule in change_order_rules() {
        if rule.servant_id != servant_id {
            continue;
        }
        if rule.timing != timing {
            continue;
        }
        if let ChangeOrderTrigger::ServantSkill {
            skill: trigger_skill,
        } = &rule.trigger
        {
            if trigger_skill == skill {
                apply_change_order_effect(ids, source_index, &rule.effect);
                return;
            }
        }
    }
}

fn apply_attack_card_lineup_change(
    ids: &mut [Option<u32>; 6],
    card: &AttackCard,
    np_use_counts: &mut HashMap<u32, u32>,
) {
    let Some(source_index) = card
        .card
        .as_deref()
        .and_then(|value| value.strip_suffix("_np"))
        .and_then(|value| parse_index(value, "servant_"))
        .filter(|index| *index < 3)
    else {
        return;
    };
    let Some(servant_id) = ids[source_index] else {
        return;
    };
    let next_count = np_use_counts.get(&servant_id).copied().unwrap_or(0) + 1;
    np_use_counts.insert(servant_id, next_count);

    for rule in change_order_rules() {
        if rule.servant_id != servant_id {
            continue;
        }
        if rule.timing != ChangeOrderTiming::Immediate {
            continue;
        }
        if let ChangeOrderTrigger::AttackCard {
            card: trigger_card,
            activation_use_count,
        } = &rule.trigger
        {
            let count_matches = activation_use_count
                .map(|count| count == next_count)
                .unwrap_or(true);
            if trigger_card == "np" && count_matches {
                apply_change_order_effect(ids, source_index, &rule.effect);
                return;
            }
        }
    }
}

fn front_slot_with_most_cards(
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
) -> Option<usize> {
    let mut counts = [0usize; 3];
    for card in cards {
        let Some(servant_id) = card.servant_id else {
            continue;
        };
        if let Some(index) = party_ids
            .iter()
            .position(|party_id| *party_id == Some(servant_id))
        {
            counts[index] += 1;
        }
    }
    (0..3)
        .filter(|index| party_ids[*index].is_some())
        .max_by_key(|index| (counts[*index], std::cmp::Reverse(*index)))
}

fn grand_auto_order_change_action(
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
    grand_servants: &[GrandServantRuntimeConfig],
) -> Option<Action> {
    let main = grand_servants.first()?;
    if !(3..6).contains(&main.slot_index) {
        return None;
    }
    let front_index = front_slot_with_most_cards(cards, party_ids)?;
    Some(Action::Equipment {
        id: "auto_grand_order_change".into(),
        skill: Some("skill_3".into()),
        target: None,
        order_change: Some(crate::OrderChangeSelection {
            front: Some(format!("servant_{}", front_index + 1)),
            back: Some(format!("servant_{}", main.slot_index + 1)),
        }),
    })
}

fn front_slot_has_servant(ids: &[Option<u32>; 6], value: Option<&str>) -> bool {
    let Some(index) = value.and_then(|value| parse_index(value, "servant_")) else {
        return false;
    };
    index < 3 && ids[index].is_some()
}

fn optional_front_target_available(ids: &[Option<u32>; 6], value: Option<&str>) -> bool {
    match value {
        Some(target) => front_slot_has_servant(ids, Some(target)),
        None => true,
    }
}

fn action_frontline_available(ids: &[Option<u32>; 6], action: &Action) -> bool {
    match action {
        Action::Servant {
            servant, target, ..
        } => {
            front_slot_has_servant(ids, servant.as_deref())
                && optional_front_target_available(ids, target.as_deref())
        }
        Action::Equipment {
            target,
            order_change: Some(order_change),
            ..
        } => {
            let front = order_change
                .front
                .as_deref()
                .and_then(|value| parse_index(value, "servant_"));
            let back = order_change
                .back
                .as_deref()
                .and_then(|value| parse_index(value, "servant_"));
            matches!((front, back), (Some(front), Some(back)) if front < 3 && back < 6 && ids[front].is_some() && ids[back].is_some())
                && optional_front_target_available(ids, target.as_deref())
        }
        Action::Equipment { target, .. } | Action::CommandSpell { target, .. } => {
            optional_front_target_available(ids, target.as_deref())
        }
    }
}

fn current_slot_for_original_selection(
    ids: &[Option<u32>; 6],
    original_ids: &[Option<u32>; 6],
    value: &str,
    allowed: std::ops::Range<usize>,
) -> Option<String> {
    let original_index = parse_index(value, "servant_")?;
    let servant_id = original_ids.get(original_index).copied().flatten()?;
    let current_index = ids
        .iter()
        .position(|current_id| *current_id == Some(servant_id))?;
    if !allowed.contains(&current_index) {
        return None;
    }
    Some(format!("servant_{}", current_index + 1))
}

fn resolve_required_slot_to_current_position(
    ids: &[Option<u32>; 6],
    original_ids: &[Option<u32>; 6],
    value: Option<&str>,
    allowed: std::ops::Range<usize>,
) -> Option<Option<String>> {
    Some(Some(current_slot_for_original_selection(
        ids,
        original_ids,
        value?,
        allowed,
    )?))
}

fn resolve_optional_slot_to_current_position(
    ids: &[Option<u32>; 6],
    original_ids: &[Option<u32>; 6],
    value: Option<&str>,
) -> Option<Option<String>> {
    match value {
        Some(value) => Some(Some(current_slot_for_original_selection(
            ids,
            original_ids,
            value,
            0..3,
        )?)),
        None => Some(None),
    }
}

fn resolve_action_to_current_positions(
    ids: &[Option<u32>; 6],
    original_ids: &[Option<u32>; 6],
    action: &Action,
) -> Option<Action> {
    match action {
        Action::Servant {
            id,
            servant,
            target,
            ..
        } => Some(Action::Servant {
            id: id.clone(),
            servant: resolve_required_slot_to_current_position(
                ids,
                original_ids,
                servant.as_deref(),
                0..3,
            )?,
            skill: match action {
                Action::Servant { skill, .. } => skill.clone(),
                _ => None,
            },
            target: resolve_optional_slot_to_current_position(
                ids,
                original_ids,
                target.as_deref(),
            )?,
        }),
        Action::Equipment {
            id,
            skill,
            target,
            order_change: Some(order_change),
            ..
        } => Some(Action::Equipment {
            id: id.clone(),
            skill: skill.clone(),
            target: resolve_optional_slot_to_current_position(
                ids,
                original_ids,
                target.as_deref(),
            )?,
            order_change: Some(crate::OrderChangeSelection {
                front: resolve_required_slot_to_current_position(
                    ids,
                    original_ids,
                    order_change.front.as_deref(),
                    0..3,
                )?,
                back: resolve_required_slot_to_current_position(
                    ids,
                    original_ids,
                    order_change.back.as_deref(),
                    3..6,
                )?,
            }),
        }),
        Action::Equipment {
            id,
            skill,
            target,
            order_change: None,
        } => Some(Action::Equipment {
            id: id.clone(),
            skill: skill.clone(),
            target: resolve_optional_slot_to_current_position(
                ids,
                original_ids,
                target.as_deref(),
            )?,
            order_change: None,
        }),
        Action::CommandSpell { id, spell, target } => Some(Action::CommandSpell {
            id: id.clone(),
            spell: spell.clone(),
            target: resolve_optional_slot_to_current_position(
                ids,
                original_ids,
                target.as_deref(),
            )?,
        }),
    }
}

fn action_frontline_label(action: &Action) -> String {
    match action {
        Action::Servant {
            servant, target, ..
        } => servant
            .as_deref()
            .or(target.as_deref())
            .unwrap_or("从者")
            .to_string(),
        Action::Equipment {
            target,
            order_change: Some(order_change),
            ..
        } => order_change
            .front
            .as_deref()
            .or(order_change.back.as_deref())
            .or(target.as_deref())
            .unwrap_or("从者")
            .to_string(),
        Action::Equipment { target, .. } | Action::CommandSpell { target, .. } => {
            target.as_deref().unwrap_or("从者").to_string()
        }
    }
}

fn advanced_startup_flow_actions(
    scene: &AdvancedBattleScene,
    control_count: usize,
    startup_control_count: usize,
    auto_order_change: Option<&Action>,
) -> Vec<Action> {
    let control_count = control_count.min(scene.control_actions.len());
    let startup_control_count = startup_control_count.min(control_count);
    let mut actions = Vec::new();
    if let Some(action) = auto_order_change {
        actions.push(action.clone());
    }
    actions.extend(
        scene
            .control_actions
            .iter()
            .take(startup_control_count)
            .cloned(),
    );
    actions.extend(scene.startup_actions.iter().cloned());
    actions.extend(
        scene
            .control_actions
            .iter()
            .skip(startup_control_count)
            .take(control_count.saturating_sub(startup_control_count))
            .cloned(),
    );
    actions
}

fn skill_position(servant: Option<&str>, skill: Option<&str>) -> Option<Point> {
    let si = parse_index(servant?, "servant_")?;
    let ki = parse_index(skill?, "skill_")?;
    SERVANT_SKILLS.get(si).and_then(|row| row.get(ki)).copied()
}

fn equipment_skill_position(skill: Option<&str>) -> Option<Point> {
    let ki = parse_index(skill?, "skill_")?;
    EQUIPMENT_SKILLS.get(ki).copied()
}

/// Map a Command Spell name to its index in `COMMAND_SPELL_OPTIONS`.
/// Returns `None` for unknown / missing values so the runner can skip
/// the action gracefully (mirrors how `skill_position` returns `None`
/// for malformed servant/skill strings).
fn command_spell_index(spell: Option<&str>) -> Option<usize> {
    match spell? {
        "np_release" => Some(0),
        "restore" => Some(1),
        _ => None,
    }
}

fn scene_preparation_actions(scene: &BattleScene) -> std::slice::Iter<'_, Action> {
    scene.preparation_actions.iter()
}

fn attack_priority_for_current_scene<'a>(
    advanced_mode: bool,
    scene_config_used: bool,
    scenes: &'a [BattleScene],
    current_scene_index: usize,
) -> Option<&'a [AttackCard]> {
    if advanced_mode && !scene_config_used {
        return None;
    }
    scenes
        .get(current_scene_index)
        .map(|scene| scene.attack_priority.as_slice())
}

fn normal_current_party_ids_from(
    mut ids: [Option<u32>; 6],
    scenes: &[BattleScene],
    current_scene_index: usize,
    executed_scene_index: Option<usize>,
) -> [Option<u32>; 3] {
    let mut np_use_counts: HashMap<u32, u32> = HashMap::new();
    for (index, scene) in scenes.iter().enumerate() {
        let should_apply = index < current_scene_index || executed_scene_index == Some(index);
        if !should_apply {
            continue;
        }
        for action in scene_preparation_actions(scene) {
            if action_frontline_available(&ids, action) {
                apply_party_lineup_change(&mut ids, action);
            }
        }
        if index < current_scene_index {
            for card in &scene.attack_priority {
                apply_attack_card_lineup_change(&mut ids, card, &mut np_use_counts);
            }
            for action in scene_preparation_actions(scene) {
                if action_frontline_available(&ids, action) {
                    apply_party_lineup_change_at(&mut ids, action, ChangeOrderTiming::EndOfTurn);
                }
            }
        }
    }
    [ids[0], ids[1], ids[2]]
}

/// Skill targets are always allies (servant_1, servant_2, servant_3).
fn skill_target_position(target: Option<&str>) -> Option<Point> {
    let t = target?;
    let si = parse_index(t, "servant_")?;
    SKILL_TARGETS.get(si).copied()
}

fn order_change_slot_position(
    target: Option<&str>,
    allowed: std::ops::Range<usize>,
) -> Option<Point> {
    let t = target?;
    let si = parse_index(t, "servant_")?;
    if !allowed.contains(&si) {
        return None;
    }
    ORDER_CHANGE_SLOTS.get(si).copied()
}

/// Enemy targets (enemy_1..enemy_6).
fn enemy_target_position(target: Option<&str>) -> Option<Point> {
    let t = target?;
    let ei = parse_index(t, "enemy_")?;
    ENEMY_TARGETS.get(ei).copied()
}

fn slot_x_position(index: u32) -> f64 {
    match index {
        0 => 0.12,
        1 => 0.28,
        2 => 0.44,
        3 => 0.56,
        4 => 0.72,
        5 => 0.88,
        _ => 0.50,
    }
}

/// Parse a template-key style servant name like ``"servant_215"`` into its
/// numeric id. Returns ``None`` for any other string shape so callers can
/// gracefully drop unknown supports instead of erroring.
fn parse_servant_name_id(name: &str) -> Option<u32> {
    name.strip_prefix("servant_")?.parse::<u32>().ok()
}

/// Cheap (dx, dy) pixel jitter in the range
/// `[-TAP_JITTER_PX, TAP_JITTER_PX]` derived from the current wall-clock
/// nanos. Avoids pulling in a `rand` dependency for what is essentially
/// "make our taps look slightly less robotic" -- the distribution does
/// not need to be cryptographically uniform.
fn jitter_offset() -> (i32, i32) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    // Two independent low-bit slices of the same nanos value.
    let span = (TAP_JITTER_PX * 2 + 1) as u32;
    let dx = (nanos % span) as i32 - TAP_JITTER_PX;
    let dy = ((nanos / span) % span) as i32 - TAP_JITTER_PX;
    (dx, dy)
}

/// Center of a normalized rectangle (used to derive a tap point from a
/// detected NP slot's `card_region`).
fn rect_center(r: &NormRect) -> Point {
    Point::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

// ---------------------------------------------------------------------------
// Attack pick logic
// ---------------------------------------------------------------------------

/// One card chosen by the priority walk / fill step. Carries enough info to
/// log + tap.
#[derive(Debug, Clone)]
enum Pick {
    Card {
        slot: u32,
        point: Point,
        servant_id: Option<u32>,
        suit: Option<String>,
        /// Source of the pick: priority entry text (e.g. ``"servant_2_arts"``)
        /// or ``None`` if it came from the leftmost-fill step.
        from_priority: Option<String>,
    },
    Np {
        slot: u32,
        point: Point,
        from_priority: String,
    },
}

/// Map ``"quick"|"arts"|"buster"`` to the suit code returned by the sidecar.
fn suit_code(suit: &str) -> Option<&'static str> {
    match suit {
        "quick" => Some("q"),
        "arts" => Some("a"),
        "buster" => Some("b"),
        _ => None,
    }
}

fn advanced_rule_matches(
    rule: &AdvancedRule,
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
) -> bool {
    let np_matches = if rule.np_condition_groups.is_empty() {
        true
    } else {
        rule.np_condition_groups.iter().any(|group| {
            group
                .slots
                .iter()
                .all(|slot| np_condition_matches(slot, nps))
        })
    };
    let command_matches = if rule.command_condition_groups.is_empty() {
        true
    } else {
        rule.command_condition_groups.iter().any(|group| {
            group
                .cards
                .iter()
                .all(|condition| command_condition_matches(condition, cards, party_ids))
        })
    };
    np_matches && command_matches
}

fn should_retry_command_card_owner_detection(
    cards: &[CommandCardMatch],
    candidate_ids: &[u32],
) -> bool {
    !candidate_ids.is_empty()
        && (cards.len() < COMMAND_CARD_COUNT || cards.iter().any(|card| card.servant_id.is_none()))
}

fn np_condition_matches(
    condition: &crate::AdvancedNpSlotCondition,
    nps: &[NoblePhantasmMatch],
) -> bool {
    let Some(index) = parse_index(&condition.servant, "servant_") else {
        return false;
    };
    nps.iter()
        .find(|np| np.slot as usize == index)
        .map(|np| np.ready == condition.ready)
        .unwrap_or(false)
}

fn command_condition_matches(
    condition: &AdvancedCommandCardCondition,
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
) -> bool {
    let Some(card) = cards.iter().find(|card| card.slot == condition.slot) else {
        return false;
    };

    if condition.servant != "any" {
        let Some(index) = parse_index(&condition.servant, "servant_") else {
            return false;
        };
        let Some(expected_id) = party_ids.get(index).copied().flatten() else {
            return false;
        };
        if card.servant_id != Some(expected_id) {
            return false;
        }
    }

    if condition.suit != "any" {
        let Some(expected_suit) = suit_code(&condition.suit) else {
            return false;
        };
        if card.suit.as_deref() != Some(expected_suit) {
            return false;
        }
    }

    if let Some(min_crit) = condition.min_crit_chance {
        if card.crit_chance.unwrap_or(0) < min_crit {
            return false;
        }
    }

    true
}

fn command_condition_matches_card(
    condition: &AdvancedCommandCardCondition,
    card: &CommandCardMatch,
    party_ids: &[Option<u32>; 3],
) -> bool {
    if condition.servant != "any" {
        let Some(index) = parse_index(&condition.servant, "servant_") else {
            return false;
        };
        let Some(expected_id) = party_ids.get(index).copied().flatten() else {
            return false;
        };
        if card.servant_id != Some(expected_id) {
            return false;
        }
    }

    if condition.suit != "any" {
        let Some(expected_suit) = suit_code(&condition.suit) else {
            return false;
        };
        if card.suit.as_deref() != Some(expected_suit) {
            return false;
        }
    }

    if let Some(min_crit) = condition.min_crit_chance {
        if card.crit_chance.unwrap_or(0) < min_crit {
            return false;
        }
    }

    true
}

/// Walk the first three priority rows as fixed chain slots, then walk
/// remaining rows as fallback priorities. Empty fixed slots inherit the
/// previous non-NP fixed slot, so a partial ``NP, All, empty`` chain still
/// tries to fill the third card from the ``All`` rule. Fallback rows are
/// repeated while they can still match, before moving to the next priority.
/// Cards / NPs already picked are tracked in `used_card_slots` /
/// `used_np_slots` and will not be re-selected.
fn pick_by_priority(
    priority: &[AttackCard],
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    used_card_slots: &mut HashSet<u32>,
    used_np_slots: &mut HashSet<u32>,
) -> Vec<Pick> {
    let mut picks: Vec<Pick> = Vec::with_capacity(3);
    let mut inherited_fixed_card: Option<String> = None;

    for entry in priority.iter().take(3) {
        if picks.len() >= 3 {
            break;
        }
        let card_str = entry.card.as_deref().or(inherited_fixed_card.as_deref());
        if let Some(card_str) = card_str {
            if let Some(pick) = pick_one_priority(
                card_str,
                cards,
                nps,
                party_ids,
                used_card_slots,
                used_np_slots,
            ) {
                picks.push(pick);
            }
        }
        if let Some(card) = entry.card.as_deref() {
            if !priority_card_is_np(card) && parse_priority_card(card).is_some() {
                inherited_fixed_card = Some(card.to_string());
            }
        }
    }

    for entry in priority.iter().skip(3) {
        let Some(card_str) = entry.card.as_deref() else {
            continue;
        };
        while picks.len() < 3 {
            let Some(pick) = pick_one_priority(
                card_str,
                cards,
                nps,
                party_ids,
                used_card_slots,
                used_np_slots,
            ) else {
                break;
            };
            picks.push(pick);
            if priority_card_is_np(card_str) {
                break;
            }
        }
    }

    picks
}

fn parse_priority_card(card_str: &str) -> Option<(usize, &str)> {
    let rest = card_str.strip_prefix("servant_")?;
    let (idx_str, kind) = rest.split_once('_')?;
    let field_pos = idx_str.parse::<usize>().ok()?;
    if !(1..=3).contains(&field_pos) {
        return None;
    }
    Some((field_pos, kind))
}

fn priority_card_is_np(card_str: &str) -> bool {
    parse_priority_card(card_str)
        .map(|(_, kind)| kind == "np")
        .unwrap_or(false)
}

fn pick_one_priority(
    card_str: &str,
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    used_card_slots: &mut HashSet<u32>,
    used_np_slots: &mut HashSet<u32>,
) -> Option<Pick> {
    let (field_pos, kind) = parse_priority_card(card_str)?;

    if kind == "np" {
        let np_slot = (field_pos - 1) as u32;
        if used_np_slots.contains(&np_slot) {
            return None;
        }
        let np = nps.iter().find(|n| n.slot == np_slot && n.ready)?;
        used_np_slots.insert(np_slot);
        return Some(Pick::Np {
            slot: np_slot,
            point: rect_center(&np.card_region),
            from_priority: card_str.to_string(),
        });
    }

    let wanted_id = party_ids[field_pos - 1]?;
    let wanted_suit = if kind == "all" {
        None
    } else {
        Some(suit_code(kind)?)
    };

    let mut best: Option<&CommandCardMatch> = None;
    for c in cards {
        if used_card_slots.contains(&c.slot) {
            continue;
        }
        if c.servant_id != Some(wanted_id) {
            continue;
        }
        if let Some(wanted_suit) = wanted_suit {
            if c.suit.as_deref() != Some(wanted_suit) {
                continue;
            }
        } else if c.suit.is_none() {
            continue;
        }
        if best.map_or(true, |b| c.slot < b.slot) {
            best = Some(c);
        }
    }

    let c = best?;
    used_card_slots.insert(c.slot);
    Some(Pick::Card {
        slot: c.slot,
        point: Point::new(c.x, c.y),
        servant_id: c.servant_id,
        suit: c.suit.clone(),
        from_priority: Some(card_str.to_string()),
    })
}

/// After the priority walk, top picks up to 3 by choosing the leftmost
/// command cards that have not yet been used.
fn fill_remaining(
    picks: &mut Vec<Pick>,
    cards: &[CommandCardMatch],
    used_card_slots: &mut HashSet<u32>,
) {
    let mut sorted: Vec<&CommandCardMatch> = cards.iter().collect();
    sorted.sort_by_key(|c| c.slot);
    for c in sorted {
        if picks.len() >= 3 {
            break;
        }
        if used_card_slots.contains(&c.slot) {
            continue;
        }
        used_card_slots.insert(c.slot);
        picks.push(Pick::Card {
            slot: c.slot,
            point: Point::new(c.x, c.y),
            servant_id: c.servant_id,
            suit: c.suit.clone(),
            from_priority: None,
        });
    }
}

fn advanced_startup_conditions_match(
    scene: &AdvancedBattleScene,
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
) -> bool {
    let active: Vec<&AdvancedCommandCardCondition> = scene
        .command_conditions
        .iter()
        .filter(|condition| condition.servant != "any" || condition.suit != "any")
        .collect();
    if active.is_empty() {
        return true;
    }
    let mut used_slots = HashSet::new();
    startup_conditions_match_from(0, &active, cards, party_ids, &mut used_slots)
}

fn startup_conditions_match_from(
    condition_index: usize,
    conditions: &[&AdvancedCommandCardCondition],
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
    used_slots: &mut HashSet<u32>,
) -> bool {
    if condition_index >= conditions.len() {
        return true;
    }

    let condition = conditions[condition_index];
    for card in cards {
        if used_slots.contains(&card.slot) {
            continue;
        }
        if !command_condition_matches_card(condition, card, party_ids) {
            continue;
        }
        used_slots.insert(card.slot);
        if startup_conditions_match_from(
            condition_index + 1,
            conditions,
            cards,
            party_ids,
            used_slots,
        ) {
            return true;
        }
        used_slots.remove(&card.slot);
    }
    false
}

fn uses_advanced_strategy_flow(scene: &AdvancedBattleScene) -> bool {
    scene.main_output.is_some()
        || scene.grand_auto_order_change == Some(true)
        || !scene.command_conditions.is_empty()
        || !scene.control_actions.is_empty()
        || !scene.startup_actions.is_empty()
}

#[derive(Clone)]
struct AdvancedPickCandidate {
    pick: Pick,
    servant_index: Option<usize>,
    servant_id: Option<u32>,
    color: Option<String>,
    original_order: u32,
    is_np: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum GrandRole {
    Main,
    Deputy,
    Other,
}

fn grand_role_for_servant(
    servant_id: Option<u32>,
    grand_servants: &[GrandServantRuntimeConfig],
) -> GrandRole {
    match servant_id {
        Some(id)
            if grand_servants
                .first()
                .is_some_and(|config| config.servant_id == id) =>
        {
            GrandRole::Main
        }
        Some(id)
            if grand_servants
                .get(1)
                .is_some_and(|config| config.servant_id == id) =>
        {
            GrandRole::Deputy
        }
        _ => GrandRole::Other,
    }
}

fn grand_config_for_role(
    role: GrandRole,
    grand_servants: &[GrandServantRuntimeConfig],
) -> Option<&GrandServantRuntimeConfig> {
    match role {
        GrandRole::Main => grand_servants.first(),
        GrandRole::Deputy => grand_servants.get(1),
        GrandRole::Other => None,
    }
}

fn grand_np_color(config: &GrandServantRuntimeConfig) -> Option<&'static str> {
    if config.np_card != "auto" {
        return suit_code(&config.np_card);
    }
    servant_np_card_code(config.servant_id)
}

fn main_output_index(scene: &AdvancedBattleScene) -> Option<usize> {
    scene
        .main_output
        .as_ref()
        .and_then(|main| main.servant.as_deref())
        .and_then(|value| parse_index(value, "servant_"))
        .filter(|index| *index < 3)
}

fn main_np_color(
    scene: &AdvancedBattleScene,
    party_ids: &[Option<u32>; 3],
) -> Option<&'static str> {
    let main = scene.main_output.as_ref()?;
    if let Some(card) = main.np_card.as_deref().filter(|card| *card != "auto") {
        return suit_code(card);
    }
    let index = main_output_index(scene)?;
    let servant_id = party_ids.get(index).copied().flatten()?;
    servant_np_card_code(servant_id)
}

fn candidate_color_counts(candidates: &[&AdvancedPickCandidate]) -> (usize, usize, usize) {
    let b = candidates
        .iter()
        .filter(|c| c.color.as_deref() == Some("b"))
        .count();
    let a = candidates
        .iter()
        .filter(|c| c.color.as_deref() == Some("a"))
        .count();
    let q = candidates
        .iter()
        .filter(|c| c.color.as_deref() == Some("q"))
        .count();
    (b, a, q)
}

fn servant_np_card_code(id: u32) -> Option<&'static str> {
    servant_np_card(id).and_then(|card| suit_code(&card))
}

fn score_advanced_combo(
    scene: &AdvancedBattleScene,
    combo: &[&AdvancedPickCandidate],
    main_index: Option<usize>,
) -> i32 {
    let mut score = 0;
    let np_count = combo.iter().filter(|c| c.is_np).count();
    let main_count = combo
        .iter()
        .filter(|c| c.servant_index.is_some() && c.servant_index == main_index)
        .count();
    let (buster, arts, quick) = candidate_color_counts(combo);

    score += (np_count as i32) * 10_000;
    score += (main_count as i32) * 300;
    if main_count == 3 && buster == 1 && arts == 1 && quick == 1 {
        score += 2_000;
    }

    match scene
        .main_output
        .as_ref()
        .and_then(|main| main.output_type.as_ref())
    {
        Some(AdvancedOutputType::Np) => {
            if arts == 3 {
                score += 1_200;
            }
            score += (arts as i32) * 220;
            score += combo
                .iter()
                .filter(|c| c.color.as_deref() == Some("a") && c.servant_index == main_index)
                .count() as i32
                * 120;
        }
        Some(AdvancedOutputType::Critical) => {
            if buster == 1 && quick == 2 {
                score += 1_200;
            }
            score += (quick as i32) * 220;
            score += (buster as i32) * 80;
            score += combo
                .iter()
                .filter(|c| c.color.as_deref() == Some("q") && c.servant_index == main_index)
                .count() as i32
                * 120;
        }
        None => {}
    }

    score
}

fn sort_advanced_picks(scene: &AdvancedBattleScene, picks: &mut Vec<AdvancedPickCandidate>) {
    match scene
        .main_output
        .as_ref()
        .and_then(|main| main.output_type.as_ref())
    {
        Some(AdvancedOutputType::Critical) => picks.sort_by_key(|candidate| {
            (
                if candidate.is_np { 0 } else { 1 },
                match candidate.color.as_deref() {
                    Some("b") => 1,
                    Some("q") => 2,
                    Some("a") => 3,
                    _ => 4,
                },
                candidate.original_order,
            )
        }),
        _ => picks.sort_by_key(|candidate| {
            (
                match candidate.color.as_deref() {
                    Some("a") => 2,
                    _ => 1,
                },
                candidate.original_order,
            )
        }),
    }
}

fn combo_same_role_servant(
    combo: &[&AdvancedPickCandidate],
    role: GrandRole,
    grand_servants: &[GrandServantRuntimeConfig],
) -> bool {
    let Some(config) = grand_config_for_role(role, grand_servants) else {
        return false;
    };
    combo
        .iter()
        .all(|candidate| candidate.servant_id == Some(config.servant_id))
}

fn combo_has_role(
    combo: &[&AdvancedPickCandidate],
    role: GrandRole,
    grand_servants: &[GrandServantRuntimeConfig],
) -> bool {
    let Some(config) = grand_config_for_role(role, grand_servants) else {
        return false;
    };
    combo
        .iter()
        .any(|candidate| candidate.servant_id == Some(config.servant_id))
}

fn combo_has_role_np(
    combo: &[&AdvancedPickCandidate],
    role: GrandRole,
    grand_servants: &[GrandServantRuntimeConfig],
) -> bool {
    let Some(config) = grand_config_for_role(role, grand_servants) else {
        return false;
    };
    combo
        .iter()
        .any(|candidate| candidate.is_np && candidate.servant_id == Some(config.servant_id))
}

fn combo_is_exquisite(combo: &[&AdvancedPickCandidate]) -> bool {
    candidate_color_counts(combo) == (1, 1, 1)
}

fn combo_same_color(combo: &[&AdvancedPickCandidate]) -> bool {
    matches!(
        candidate_color_counts(combo),
        (3, 0, 0) | (0, 3, 0) | (0, 0, 3)
    )
}

fn normalized_grand_chain_priority(strategy: &GrandCardStrategy) -> Vec<GrandChainPriorityItem> {
    let defaults = default_grand_chain_priority();
    let mut normalized = Vec::with_capacity(defaults.len());
    for item in &strategy.chain_priority {
        if defaults.contains(item) && !normalized.contains(item) {
            normalized.push(*item);
        }
    }
    for item in defaults {
        if !normalized.contains(&item) {
            normalized.push(item);
        }
    }
    normalized
}

fn grand_combo_matches_priority(
    combo: &[&AdvancedPickCandidate],
    grand_servants: &[GrandServantRuntimeConfig],
    item: GrandChainPriorityItem,
) -> Option<(i32, Option<GrandRole>)> {
    match item {
        GrandChainPriorityItem::MainBraveChain
            if combo_same_role_servant(combo, GrandRole::Main, grand_servants) =>
        {
            if combo_is_exquisite(combo) {
                Some((300, Some(GrandRole::Main)))
            } else if combo_same_color(combo) {
                Some((200, Some(GrandRole::Main)))
            } else {
                Some((100, Some(GrandRole::Main)))
            }
        }
        GrandChainPriorityItem::MainReadyNp
            if combo_has_role_np(combo, GrandRole::Main, grand_servants) =>
        {
            Some((0, Some(GrandRole::Main)))
        }
        GrandChainPriorityItem::DeputyBraveChain
            if combo_same_role_servant(combo, GrandRole::Deputy, grand_servants) =>
        {
            if combo_is_exquisite(combo) {
                Some((300, Some(GrandRole::Deputy)))
            } else if combo_same_color(combo) {
                Some((200, Some(GrandRole::Deputy)))
            } else {
                Some((100, Some(GrandRole::Deputy)))
            }
        }
        GrandChainPriorityItem::MainColorChain
            if combo_same_color(combo)
                && combo_has_role(combo, GrandRole::Main, grand_servants) =>
        {
            Some((0, Some(GrandRole::Main)))
        }
        GrandChainPriorityItem::DeputyColorChain
            if combo_same_color(combo)
                && combo_has_role(combo, GrandRole::Deputy, grand_servants) =>
        {
            Some((0, Some(GrandRole::Deputy)))
        }
        GrandChainPriorityItem::Fallback => Some((0, None)),
        _ => None,
    }
}

fn grand_combo_tier(
    combo: &[&AdvancedPickCandidate],
    grand_servants: &[GrandServantRuntimeConfig],
    strategy: &GrandCardStrategy,
) -> (i32, Option<GrandRole>) {
    let priority = normalized_grand_chain_priority(strategy);
    for (index, item) in priority.iter().enumerate() {
        if let Some((sub_score, role)) = grand_combo_matches_priority(combo, grand_servants, *item)
        {
            let rank = (priority.len() - index) as i32;
            return (rank * 1_000 + sub_score, role);
        }
    }
    (1_000, None)
}

fn score_grand_combo(
    combo: &[&AdvancedPickCandidate],
    grand_servants: &[GrandServantRuntimeConfig],
    strategy: &GrandCardStrategy,
) -> i32 {
    let (tier, target_role) = grand_combo_tier(combo, grand_servants, strategy);
    let np_count = combo.iter().filter(|candidate| candidate.is_np).count() as i32;
    let main_count = combo
        .iter()
        .filter(|candidate| {
            grand_role_for_servant(candidate.servant_id, grand_servants) == GrandRole::Main
        })
        .count() as i32;
    let deputy_count = combo
        .iter()
        .filter(|candidate| {
            grand_role_for_servant(candidate.servant_id, grand_servants) == GrandRole::Deputy
        })
        .count() as i32;
    let target_count = target_role
        .map(|role| {
            combo
                .iter()
                .filter(|candidate| {
                    grand_role_for_servant(candidate.servant_id, grand_servants) == role
                })
                .count() as i32
        })
        .unwrap_or(main_count.max(deputy_count));

    tier * 1_000 + target_count * 150 + main_count * 60 + deputy_count * 40 + np_count * 20
}

fn target_np_color(
    role: Option<GrandRole>,
    grand_servants: &[GrandServantRuntimeConfig],
) -> Option<String> {
    role.and_then(|role| grand_config_for_role(role, grand_servants))
        .and_then(grand_np_color)
        .map(str::to_string)
}

fn sort_exquisite_grand_picks(
    picks: &mut Vec<AdvancedPickCandidate>,
    target_role: GrandRole,
    grand_servants: &[GrandServantRuntimeConfig],
) {
    let priority = grand_config_for_role(target_role, grand_servants)
        .map(|config| config.priority.as_str())
        .unwrap_or("damage");
    let np_color = target_np_color(Some(target_role), grand_servants);
    let use_damage_order = priority != "np" || np_color.as_deref() == Some("b");

    picks.sort_by_key(|candidate| {
        if use_damage_order {
            (
                if candidate.is_np {
                    3
                } else if np_color
                    .as_deref()
                    .is_some_and(|color| candidate.color.as_deref() == Some(color))
                {
                    2
                } else {
                    1
                },
                candidate.original_order,
            )
        } else {
            (
                if !candidate.is_np && candidate.color.as_deref() == Some("b") {
                    1
                } else if candidate.is_np {
                    2
                } else {
                    3
                },
                candidate.original_order,
            )
        }
    });
}

fn sort_grand_picks(
    picks: &mut Vec<AdvancedPickCandidate>,
    tier_role: Option<GrandRole>,
    grand_servants: &[GrandServantRuntimeConfig],
) {
    if let Some(role) = tier_role {
        let same_servant = picks
            .iter()
            .all(|candidate| grand_role_for_servant(candidate.servant_id, grand_servants) == role);
        let refs: Vec<&AdvancedPickCandidate> = picks.iter().collect();
        if same_servant && combo_is_exquisite(&refs) {
            sort_exquisite_grand_picks(picks, role, grand_servants);
            return;
        }
        if combo_same_color(&refs) {
            picks.sort_by_key(|candidate| {
                (
                    if candidate.is_np { 0 } else { 1 },
                    if grand_role_for_servant(candidate.servant_id, grand_servants) == role {
                        1
                    } else {
                        0
                    },
                    candidate.original_order,
                )
            });
            return;
        }
    }

    let target_role = if picks.iter().any(|candidate| {
        grand_role_for_servant(candidate.servant_id, grand_servants) == GrandRole::Main
    }) {
        GrandRole::Main
    } else {
        GrandRole::Deputy
    };
    let target_np_slot = picks
        .iter()
        .find(|candidate| {
            candidate.is_np
                && grand_role_for_servant(candidate.servant_id, grand_servants) == target_role
        })
        .map(|candidate| candidate.original_order);
    let preferred_dye = grand_config_for_role(target_role, grand_servants)
        .map(|config| if config.priority == "np" { "a" } else { "b" })
        .unwrap_or("b");

    picks.sort_by_key(|candidate| {
        let role = grand_role_for_servant(candidate.servant_id, grand_servants);
        let is_target = role == target_role;
        (
            match (candidate.is_np, is_target, target_np_slot.is_some()) {
                (true, false, true) => 0,
                (true, true, _) => 1,
                (false, false, false) if candidate.color.as_deref() == Some(preferred_dye) => 2,
                (false, true, _) => 4,
                _ => 3,
            },
            match role {
                GrandRole::Main => 2,
                GrandRole::Deputy => 1,
                GrandRole::Other => 0,
            },
            candidate.original_order,
        )
    });
}

fn choose_grand_auto_picks(
    candidates: &[AdvancedPickCandidate],
    grand_servants: &[GrandServantRuntimeConfig],
    strategy: &GrandCardStrategy,
) -> Vec<Pick> {
    let mut best_score = i32::MIN;
    let mut best_role = None;
    let mut best: Vec<AdvancedPickCandidate> = Vec::new();
    for i in 0..candidates.len() {
        for j in (i + 1)..candidates.len() {
            for k in (j + 1)..candidates.len() {
                let combo = vec![&candidates[i], &candidates[j], &candidates[k]];
                let score = score_grand_combo(&combo, grand_servants, strategy);
                if score > best_score {
                    best_score = score;
                    best_role = grand_combo_tier(&combo, grand_servants, strategy).1;
                    best = vec![
                        candidates[i].clone(),
                        candidates[j].clone(),
                        candidates[k].clone(),
                    ];
                }
            }
        }
    }
    sort_grand_picks(&mut best, best_role, grand_servants);
    best.into_iter().map(|candidate| candidate.pick).collect()
}

fn choose_advanced_auto_picks(
    scene: &AdvancedBattleScene,
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    grand_servants: &[GrandServantRuntimeConfig],
    grand_card_strategy: &GrandCardStrategy,
) -> Vec<Pick> {
    let main_index = main_output_index(scene);
    let main_np_color = main_np_color(scene, party_ids);
    let mut candidates: Vec<AdvancedPickCandidate> = Vec::new();

    for np in nps.iter().filter(|np| np.ready) {
        let servant_index = Some(np.slot as usize).filter(|index| *index < 3);
        let servant_id = servant_index.and_then(|index| party_ids.get(index).copied().flatten());
        let color = if let Some(config) =
            servant_id.and_then(|id| grand_servants.iter().find(|config| config.servant_id == id))
        {
            grand_np_color(config).map(str::to_string)
        } else if servant_index == main_index {
            main_np_color.map(str::to_string)
        } else {
            servant_id
                .and_then(servant_np_card_code)
                .map(str::to_string)
        };
        candidates.push(AdvancedPickCandidate {
            pick: Pick::Np {
                slot: np.slot,
                point: rect_center(&np.card_region),
                from_priority: "自动宝具".into(),
            },
            servant_index,
            servant_id,
            color,
            original_order: np.slot,
            is_np: true,
        });
    }

    for card in cards {
        let servant_index = card
            .servant_id
            .and_then(|id| party_ids.iter().position(|party_id| *party_id == Some(id)));
        candidates.push(AdvancedPickCandidate {
            pick: Pick::Card {
                slot: card.slot,
                point: Point::new(card.x, card.y),
                servant_id: card.servant_id,
                suit: card.suit.clone(),
                from_priority: Some("自动策略".into()),
            },
            servant_index,
            servant_id: card.servant_id,
            color: card.suit.clone(),
            original_order: 10 + card.slot,
            is_np: false,
        });
    }

    if candidates.len() <= 3 {
        let mut selected = candidates;
        if grand_servants.is_empty() {
            sort_advanced_picks(scene, &mut selected);
        } else {
            sort_grand_picks(&mut selected, None, grand_servants);
        }
        return selected
            .into_iter()
            .map(|candidate| candidate.pick)
            .collect();
    }

    if !grand_servants.is_empty() {
        return choose_grand_auto_picks(&candidates, grand_servants, grand_card_strategy);
    }

    let mut best_score = i32::MIN;
    let mut best: Vec<AdvancedPickCandidate> = Vec::new();
    for i in 0..candidates.len() {
        for j in (i + 1)..candidates.len() {
            for k in (j + 1)..candidates.len() {
                let combo = vec![&candidates[i], &candidates[j], &candidates[k]];
                let score = score_advanced_combo(scene, &combo, main_index);
                if score > best_score {
                    best_score = score;
                    best = vec![
                        candidates[i].clone(),
                        candidates[j].clone(),
                        candidates[k].clone(),
                    ];
                }
            }
        }
    }
    sort_advanced_picks(scene, &mut best);
    best.into_iter().map(|candidate| candidate.pick).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: assert two `f64` are approximately equal. The CE search
    /// region math is just adds + multiplies on small constants, so the
    /// epsilon is tight.
    fn approx(a: f64, b: f64) {
        assert!(
            (a - b).abs() < 1e-9,
            "expected ≈{b}, got {a} (diff {})",
            (a - b).abs()
        );
    }

    fn rect() -> NormRect {
        NormRect {
            x: 0.0,
            y: 0.0,
            w: 0.1,
            h: 0.1,
        }
    }

    fn command_card(
        slot: u32,
        servant_id: Option<u32>,
        suit: Option<&str>,
        crit: Option<u32>,
    ) -> CommandCardMatch {
        CommandCardMatch {
            slot,
            x: 0.0,
            y: 0.0,
            card_region: rect(),
            face_region: rect(),
            crit_digit_regions: None,
            crit_digit_reads: None,
            suit: suit.map(str::to_string),
            icon_score: None,
            icon_region: None,
            servant_id,
            ascension: None,
            face_score: None,
            crit_chance: crit,
        }
    }

    fn np_slot(slot: u32, ready: bool) -> NoblePhantasmMatch {
        NoblePhantasmMatch {
            slot,
            card_region: rect(),
            ready,
            edge_frac: 0.0,
            std_bgr: 0.0,
            edge_threshold: 0.0,
        }
    }

    fn empty_advanced_scene() -> AdvancedBattleScene {
        AdvancedBattleScene {
            id: "advanced_scene_1".into(),
            main_output: None,
            grand_auto_order_change: None,
            command_conditions: Vec::new(),
            control_actions: Vec::new(),
            startup_actions: Vec::new(),
            rules: Vec::new(),
        }
    }

    fn grand_config(servant_id: u32, np_card: &str, priority: &str) -> GrandServantRuntimeConfig {
        grand_config_at(0, servant_id, np_card, priority)
    }

    fn grand_config_at(
        slot_index: usize,
        servant_id: u32,
        np_card: &str,
        priority: &str,
    ) -> GrandServantRuntimeConfig {
        GrandServantRuntimeConfig {
            slot_index,
            servant_id,
            np_card: np_card.into(),
            priority: priority.into(),
        }
    }

    fn pick_labels(picks: &[Pick]) -> Vec<String> {
        picks
            .iter()
            .map(|pick| match pick {
                Pick::Card { slot, .. } => format!("C{slot}"),
                Pick::Np { slot, .. } => format!("NP{slot}"),
            })
            .collect()
    }

    #[test]
    fn ce_search_region_identity_row_returns_offset() {
        // A unit row at the origin → the absolute window equals the
        // raw `SUPPORT_CE_OFFSET_IN_ROW` (it's already in unit-row coords).
        let row = NormRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        let out = ce_search_region(row);
        approx(out.x, SUPPORT_CE_OFFSET_IN_ROW.x);
        approx(out.y, SUPPORT_CE_OFFSET_IN_ROW.y);
        approx(out.w, SUPPORT_CE_OFFSET_IN_ROW.w);
        approx(out.h, SUPPORT_CE_OFFSET_IN_ROW.h);
    }

    #[test]
    fn ce_search_region_scales_and_translates_offset_row() {
        // Row at (0.10, 0.20) sized (0.50, 0.10): the CE icon search
        // window is the row-local offset, scaled by row size, then
        // translated by row origin.
        let row = NormRect {
            x: 0.10,
            y: 0.20,
            w: 0.50,
            h: 0.10,
        };
        let out = ce_search_region(row);
        approx(out.x, 0.10 + SUPPORT_CE_OFFSET_IN_ROW.x * 0.50);
        approx(out.y, 0.20 + SUPPORT_CE_OFFSET_IN_ROW.y * 0.10);
        approx(out.w, SUPPORT_CE_OFFSET_IN_ROW.w * 0.50);
        approx(out.h, SUPPORT_CE_OFFSET_IN_ROW.h * 0.10);
    }

    #[test]
    fn ce_search_region_handles_negative_offset() {
        // SUPPORT_CE_OFFSET_IN_ROW.x is negative on purpose (the CE icon
        // sits to the *left* of the OCR-anchored row strip). For a row
        // that starts at x=0.20 with w=0.40, the search window should
        // start to the *left* of the row origin.
        let row = NormRect {
            x: 0.20,
            y: 0.30,
            w: 0.40,
            h: 0.10,
        };
        let out = ce_search_region(row);
        assert!(
            out.x < row.x,
            "search window should be left of row origin: got x={} vs row x={}",
            out.x,
            row.x,
        );
    }

    #[test]
    fn grand_ce_search_region_anchors_to_confirm_button() {
        let region = NormRect {
            x: 0.30,
            y: 0.40,
            w: 0.50,
            h: 0.10,
        };
        let mut row = support_row(None, vec![], vec![]);
        row.row_region = region;
        row.score_anchor = Some(NormRect {
            x: 0.80,
            y: 0.60,
            w: 0.04,
            h: 0.06,
        });
        let first = grand_ce_search_region(&row, 0).unwrap();
        let second = grand_ce_search_region(&row, 1).unwrap();
        let third = grand_ce_search_region(&row, 2).unwrap();

        assert!(first.y < second.y);
        assert!(second.y < third.y);
        approx(first.x, SUPPORT_GRAND_CE_X);
        approx(first.w, SUPPORT_GRAND_CE_W);
        approx(
            third.y + third.h / 2.0,
            0.60 + SUPPORT_GRAND_CE_THIRD_CENTER_FROM_BUTTON_TOP_Y,
        );
        assert!(grand_ce_search_region(&row, 3).is_none());
    }

    // --- RunConfig serde -----------------------------------------------

    /// Build the smallest valid RunConfig JSON (omitting all
    /// `#[serde(default)]` fields so the test exercises the defaults).
    fn minimal_run_config_json() -> serde_json::Value {
        serde_json::json!({
            "projectId": "p1",
            "partyOrder": null,
            "supportClassFilter": null,
            "supportServantName": null,
            "servantSelections": [],
            "maxSupportScrolls": 5,
        })
    }

    #[test]
    fn run_config_defaults_support_ce_to_none_when_field_missing() {
        let cfg: RunConfig = serde_json::from_value(minimal_run_config_json()).unwrap();
        assert!(cfg.support_craft_essence_id.is_none());
        assert_eq!(cfg.support_craft_essence_mlb_required, true);
        assert_eq!(cfg.support_grand_mode, false);
        assert_eq!(cfg.support_grand_craft_essence_ids, [None; 3]);
        assert_eq!(cfg.support_grand_craft_essence_mlb_required, [true; 3]);
        assert_eq!(cfg.support_grand_bond_ce_mode, SupportGrandBondCeMode::Any);
        assert!(cfg.grand_servants.is_empty());
        assert_eq!(
            cfg.grand_card_strategy.chain_priority,
            default_grand_chain_priority()
        );
        // Other defaults travel through the same path; sanity-check
        // them so legacy `projects.json` rows keep deserializing.
        assert!(cfg.support_servant_id.is_none());
        assert!(cfg.support_slot_index.is_none());
        assert!(cfg.support_noble_phantasm_level_min.is_none());
        assert_eq!(cfg.support_skill_level_mins, [None; 3]);
        assert_eq!(cfg.support_append_skill_level_mins, [None; 5]);
        assert_eq!(cfg.repeat_mission, false);
        assert_eq!(cfg.max_mission_runs, None);
        assert!(cfg.ap_recovery_items.is_empty());
    }

    #[test]
    fn run_config_round_trips_support_craft_essence_id() {
        let mut payload = minimal_run_config_json();
        payload["supportCraftEssenceId"] = serde_json::json!(1485);
        payload["supportSlotIndex"] = serde_json::json!(5);
        payload["supportGrandMode"] = serde_json::json!(true);
        payload["supportGrandCraftEssenceIds"] = serde_json::json!([1001, null, 1003]);
        payload["supportCraftEssenceMlbRequired"] = serde_json::json!(false);
        payload["supportGrandCraftEssenceMlbRequired"] = serde_json::json!([true, false, true]);
        payload["supportGrandBondCeMode"] = serde_json::json!("bondNp");
        payload["grandServants"] = serde_json::json!([
            { "slotIndex": 0, "npCard": "buster", "priority": "damage" },
            { "slotIndex": 2, "npCard": "auto", "priority": "np" }
        ]);
        let cfg: RunConfig = serde_json::from_value(payload).unwrap();
        assert_eq!(cfg.support_craft_essence_id, Some(1485));
        assert_eq!(cfg.support_craft_essence_mlb_required, false);
        assert_eq!(cfg.support_slot_index, Some(5));
        assert_eq!(cfg.support_grand_mode, true);
        assert_eq!(
            cfg.support_grand_craft_essence_ids,
            [Some(1001), None, Some(1003)]
        );
        assert_eq!(
            cfg.support_grand_craft_essence_mlb_required,
            [true, false, true]
        );
        assert_eq!(
            cfg.support_grand_bond_ce_mode,
            SupportGrandBondCeMode::BondNp
        );
        assert_eq!(cfg.grand_servants.len(), 2);
        assert_eq!(cfg.grand_servants[0].slot_index, 0);
        assert_eq!(cfg.grand_servants[0].np_card, "buster");
        assert_eq!(cfg.grand_servants[1].priority, "np");

        // Re-serialize and confirm the field round-trips under the
        // camelCase rename rule applied to the whole struct.
        let json = serde_json::to_value(&cfg).unwrap();
        assert_eq!(json["supportCraftEssenceId"], serde_json::json!(1485));
        assert_eq!(json["supportSlotIndex"], serde_json::json!(5));
        assert_eq!(json["supportGrandMode"], serde_json::json!(true));
        assert_eq!(
            json["supportGrandCraftEssenceIds"],
            serde_json::json!([1001, null, 1003])
        );
        assert_eq!(
            json["supportCraftEssenceMlbRequired"],
            serde_json::json!(false)
        );
        assert_eq!(
            json["supportGrandCraftEssenceMlbRequired"],
            serde_json::json!([true, false, true])
        );
        assert_eq!(json["supportGrandBondCeMode"], serde_json::json!("bondNp"));
        assert_eq!(
            json["grandServants"],
            serde_json::json!([
                { "slotIndex": 0, "npCard": "buster", "priority": "damage" },
                { "slotIndex": 2, "npCard": "auto", "priority": "np" }
            ])
        );
    }

    #[test]
    fn advanced_rule_matches_np_and_command_groups_with_and_between_types() {
        let rule = AdvancedRule {
            id: "rule".into(),
            np_condition_groups: vec![
                crate::AdvancedNpConditionGroup {
                    id: "np1".into(),
                    slots: vec![crate::AdvancedNpSlotCondition {
                        servant: "servant_1".into(),
                        ready: false,
                    }],
                },
                crate::AdvancedNpConditionGroup {
                    id: "np2".into(),
                    slots: vec![crate::AdvancedNpSlotCondition {
                        servant: "servant_2".into(),
                        ready: true,
                    }],
                },
            ],
            command_condition_groups: vec![crate::AdvancedCommandConditionGroup {
                id: "cmd".into(),
                cards: vec![AdvancedCommandCardCondition {
                    slot: 0,
                    servant: "servant_1".into(),
                    suit: "buster".into(),
                    min_crit_chance: Some(80),
                }],
            }],
            actions: vec![],
        };
        let cards = vec![command_card(0, Some(11), Some("b"), Some(80))];
        let nps = vec![np_slot(0, true), np_slot(1, true), np_slot(2, false)];
        let party_ids = [Some(11), Some(22), Some(33)];

        assert!(advanced_rule_matches(&rule, &cards, &nps, &party_ids));
    }

    #[test]
    fn advanced_rule_rejects_when_command_group_misses_even_if_np_matches() {
        let rule = AdvancedRule {
            id: "rule".into(),
            np_condition_groups: vec![crate::AdvancedNpConditionGroup {
                id: "np".into(),
                slots: vec![crate::AdvancedNpSlotCondition {
                    servant: "servant_1".into(),
                    ready: true,
                }],
            }],
            command_condition_groups: vec![crate::AdvancedCommandConditionGroup {
                id: "cmd".into(),
                cards: vec![AdvancedCommandCardCondition {
                    slot: 0,
                    servant: "servant_1".into(),
                    suit: "arts".into(),
                    min_crit_chance: Some(90),
                }],
            }],
            actions: vec![],
        };
        let cards = vec![command_card(0, Some(11), Some("a"), Some(80))];
        let nps = vec![np_slot(0, true)];
        let party_ids = [Some(11), None, None];

        assert!(!advanced_rule_matches(&rule, &cards, &nps, &party_ids));
    }

    #[test]
    fn advanced_startup_conditions_match_only_configured_command_cards() {
        let scene = AdvancedBattleScene {
            id: "advanced_scene_1".into(),
            main_output: None,
            grand_auto_order_change: None,
            command_conditions: vec![
                AdvancedCommandCardCondition {
                    slot: 0,
                    servant: "servant_1".into(),
                    suit: "buster".into(),
                    min_crit_chance: None,
                },
                AdvancedCommandCardCondition {
                    slot: 1,
                    servant: "any".into(),
                    suit: "any".into(),
                    min_crit_chance: None,
                },
            ],
            control_actions: Vec::new(),
            startup_actions: Vec::new(),
            rules: Vec::new(),
        };
        let cards = vec![
            command_card(0, Some(10), Some("b"), None),
            command_card(1, Some(20), Some("a"), None),
        ];
        let party_ids = [Some(10), Some(20), Some(30)];

        assert!(advanced_startup_conditions_match(
            &scene, &cards, &party_ids
        ));
    }

    #[test]
    fn advanced_startup_conditions_match_duplicate_servant_cards_in_any_slots() {
        let scene = AdvancedBattleScene {
            id: "advanced_scene_1".into(),
            main_output: None,
            grand_auto_order_change: None,
            command_conditions: vec![
                AdvancedCommandCardCondition {
                    slot: 0,
                    servant: "servant_1".into(),
                    suit: "any".into(),
                    min_crit_chance: None,
                },
                AdvancedCommandCardCondition {
                    slot: 1,
                    servant: "servant_1".into(),
                    suit: "any".into(),
                    min_crit_chance: None,
                },
            ],
            control_actions: Vec::new(),
            startup_actions: Vec::new(),
            rules: Vec::new(),
        };
        let cards = vec![
            command_card(0, Some(20), Some("a"), None),
            command_card(1, Some(10), Some("a"), None),
            command_card(2, Some(30), Some("b"), None),
            command_card(3, Some(10), Some("q"), None),
            command_card(4, Some(20), Some("q"), None),
        ];
        let party_ids = [Some(10), Some(20), Some(30)];

        assert!(advanced_startup_conditions_match(
            &scene, &cards, &party_ids
        ));
    }

    #[test]
    fn command_card_owner_detection_retries_until_all_five_cards_have_owners() {
        let cards = vec![
            command_card(0, None, Some("a"), None),
            command_card(1, None, Some("q"), None),
        ];
        assert!(should_retry_command_card_owner_detection(&cards, &[10, 20]));

        let partial_owner = vec![
            command_card(0, None, Some("a"), None),
            command_card(1, Some(10), Some("q"), None),
            command_card(2, Some(20), Some("b"), None),
            command_card(3, Some(10), Some("a"), None),
            command_card(4, Some(20), Some("q"), None),
        ];
        assert!(should_retry_command_card_owner_detection(
            &partial_owner,
            &[10, 20]
        ));

        let complete = vec![
            command_card(0, Some(10), Some("a"), None),
            command_card(1, Some(10), Some("q"), None),
            command_card(2, Some(20), Some("b"), None),
            command_card(3, Some(10), Some("a"), None),
            command_card(4, Some(20), Some("q"), None),
        ];
        assert!(!should_retry_command_card_owner_detection(
            &complete,
            &[10, 20]
        ));
        assert!(!should_retry_command_card_owner_detection(&cards, &[]));
        assert!(should_retry_command_card_owner_detection(&[], &[10]));
    }

    #[test]
    fn normal_priority_skips_missing_chain_card_then_uses_fallbacks() {
        let priority = vec![
            AttackCard {
                id: "chain_1".into(),
                card: Some("servant_1_buster".into()),
            },
            AttackCard {
                id: "chain_2".into(),
                card: Some("servant_1_np".into()),
            },
            AttackCard {
                id: "chain_3".into(),
                card: Some("servant_1_arts".into()),
            },
            AttackCard {
                id: "fallback_1".into(),
                card: Some("servant_2_quick".into()),
            },
        ];
        let cards = vec![
            command_card(0, Some(10), Some("a"), None),
            command_card(1, Some(20), Some("q"), None),
        ];
        let nps = vec![np_slot(0, true)];
        let mut used_cards = HashSet::new();
        let mut used_nps = HashSet::new();

        let picks = pick_by_priority(
            &priority,
            &cards,
            &nps,
            &[Some(10), Some(20), Some(30)],
            &mut used_cards,
            &mut used_nps,
        );

        assert_eq!(pick_labels(&picks), vec!["NP0", "C0", "C1"]);
    }

    #[test]
    fn normal_priority_preserves_chain_order_between_duplicate_card_colors_and_np() {
        let priority = vec![
            AttackCard {
                id: "chain_1".into(),
                card: Some("servant_1_buster".into()),
            },
            AttackCard {
                id: "chain_2".into(),
                card: Some("servant_1_np".into()),
            },
            AttackCard {
                id: "chain_3".into(),
                card: Some("servant_1_buster".into()),
            },
        ];
        let cards = vec![
            command_card(0, Some(10), Some("b"), None),
            command_card(1, Some(10), Some("b"), None),
        ];
        let nps = vec![np_slot(0, true)];
        let mut used_cards = HashSet::new();
        let mut used_nps = HashSet::new();

        let picks = pick_by_priority(
            &priority,
            &cards,
            &nps,
            &[Some(10), Some(20), Some(30)],
            &mut used_cards,
            &mut used_nps,
        );

        assert_eq!(pick_labels(&picks), vec!["C0", "NP0", "C1"]);
    }

    #[test]
    fn normal_priority_all_matches_any_suit_for_servant() {
        let priority = vec![AttackCard {
            id: "chain_1".into(),
            card: Some("servant_1_all".into()),
        }];
        let cards = vec![
            command_card(0, Some(20), Some("b"), None),
            command_card(1, Some(10), Some("q"), None),
            command_card(2, Some(10), Some("a"), None),
        ];
        let mut used_cards = HashSet::new();
        let mut used_nps = HashSet::new();

        let picks = pick_by_priority(
            &priority,
            &cards,
            &[],
            &[Some(10), Some(20), Some(30)],
            &mut used_cards,
            &mut used_nps,
        );

        assert_eq!(pick_labels(&picks), vec!["C1"]);
    }

    #[test]
    fn normal_priority_empty_fixed_slot_inherits_previous_non_np_rule() {
        let priority = vec![
            AttackCard {
                id: "chain_1".into(),
                card: Some("servant_1_np".into()),
            },
            AttackCard {
                id: "chain_2".into(),
                card: Some("servant_1_all".into()),
            },
            AttackCard {
                id: "chain_3".into(),
                card: None,
            },
        ];
        let cards = vec![
            command_card(0, Some(10), Some("b"), None),
            command_card(1, Some(20), Some("b"), None),
            command_card(2, Some(10), Some("a"), None),
            command_card(3, Some(20), Some("a"), None),
            command_card(4, Some(10), Some("q"), None),
        ];
        let nps = vec![np_slot(0, true)];
        let mut used_cards = HashSet::new();
        let mut used_nps = HashSet::new();

        let picks = pick_by_priority(
            &priority,
            &cards,
            &nps,
            &[Some(10), Some(20), Some(30)],
            &mut used_cards,
            &mut used_nps,
        );

        assert_eq!(pick_labels(&picks), vec!["NP0", "C0", "C2"]);
    }

    #[test]
    fn normal_fallback_priority_repeats_before_next_fallback() {
        let priority = vec![
            AttackCard {
                id: "chain_1".into(),
                card: Some("servant_1_buster".into()),
            },
            AttackCard {
                id: "chain_2".into(),
                card: Some("servant_1_np".into()),
            },
            AttackCard {
                id: "chain_3".into(),
                card: Some("servant_1_arts".into()),
            },
            AttackCard {
                id: "fallback_1".into(),
                card: Some("servant_1_all".into()),
            },
            AttackCard {
                id: "fallback_2".into(),
                card: Some("servant_2_all".into()),
            },
        ];
        let cards = vec![
            command_card(0, Some(20), Some("b"), None),
            command_card(1, Some(10), Some("a"), None),
            command_card(2, Some(10), Some("q"), None),
            command_card(3, Some(20), Some("b"), None),
            command_card(4, Some(10), Some("q"), None),
        ];
        let nps = vec![np_slot(0, false)];
        let mut used_cards = HashSet::new();
        let mut used_nps = HashSet::new();

        let picks = pick_by_priority(
            &priority,
            &cards,
            &nps,
            &[Some(10), Some(20), Some(30)],
            &mut used_cards,
            &mut used_nps,
        );

        assert_eq!(pick_labels(&picks), vec!["C1", "C2", "C4"]);
    }

    #[test]
    fn normal_attack_priority_is_used_even_when_scene_was_not_reexecuted() {
        let scene = BattleScene {
            id: "scene_1".into(),
            preparation_actions: vec![],
            servant_actions: vec![],
            equipment_actions: vec![],
            command_spell_actions: vec![],
            enemy_target: None,
            attack_priority: vec![AttackCard {
                id: "chain_1".into(),
                card: Some("servant_1_all".into()),
            }],
        };

        let scenes = [scene];
        let priority = attack_priority_for_current_scene(false, false, &scenes, 0).unwrap();

        assert_eq!(priority[0].card.as_deref(), Some("servant_1_all"));
    }

    #[test]
    fn advanced_attack_priority_still_requires_scene_config_used() {
        let scene = BattleScene {
            id: "scene_1".into(),
            preparation_actions: vec![],
            servant_actions: vec![],
            equipment_actions: vec![],
            command_spell_actions: vec![],
            enemy_target: None,
            attack_priority: vec![AttackCard {
                id: "chain_1".into(),
                card: Some("servant_1_all".into()),
            }],
        };

        assert!(attack_priority_for_current_scene(true, false, &[scene], 0).is_none());
    }

    #[test]
    fn normal_current_party_ids_apply_executed_order_change_before_attack() {
        let scene = BattleScene {
            id: "scene_1".into(),
            preparation_actions: vec![Action::Equipment {
                id: "eq_1".into(),
                skill: Some("skill_3".into()),
                target: None,
                order_change: Some(crate::OrderChangeSelection {
                    front: Some("servant_1".into()),
                    back: Some("servant_4".into()),
                }),
            }],
            servant_actions: vec![],
            equipment_actions: vec![],
            command_spell_actions: vec![],
            enemy_target: None,
            attack_priority: vec![],
        };

        let party_ids = normal_current_party_ids_from(
            [Some(10), Some(20), Some(30), Some(40), None, None],
            &[scene],
            0,
            Some(0),
        );

        assert_eq!(party_ids, [Some(40), Some(20), Some(30)]);
    }

    #[test]
    fn normal_current_party_ids_apply_previous_scene_np_retreat_before_attack() {
        let scene_1 = BattleScene {
            id: "scene_1".into(),
            preparation_actions: vec![],
            servant_actions: vec![],
            equipment_actions: vec![],
            command_spell_actions: vec![],
            enemy_target: None,
            attack_priority: vec![AttackCard {
                id: "atk_1".into(),
                card: Some("servant_2_np".into()),
            }],
        };
        let scene_2 = BattleScene {
            id: "scene_2".into(),
            preparation_actions: vec![],
            servant_actions: vec![],
            equipment_actions: vec![],
            command_spell_actions: vec![],
            enemy_target: None,
            attack_priority: vec![],
        };

        let party_ids = normal_current_party_ids_from(
            [Some(284), Some(16), Some(315), Some(211), None, None],
            &[scene_1, scene_2],
            1,
            Some(1),
        );

        assert_eq!(party_ids, [Some(284), Some(211), Some(315)]);
    }

    #[test]
    fn normal_current_party_ids_do_not_apply_current_scene_np_before_attack() {
        let scene = BattleScene {
            id: "scene_1".into(),
            preparation_actions: vec![],
            servant_actions: vec![],
            equipment_actions: vec![],
            command_spell_actions: vec![],
            enemy_target: None,
            attack_priority: vec![AttackCard {
                id: "atk_1".into(),
                card: Some("servant_2_np".into()),
            }],
        };

        let party_ids = normal_current_party_ids_from(
            [Some(284), Some(16), Some(315), Some(211), None, None],
            &[scene],
            0,
            Some(0),
        );

        assert_eq!(party_ids, [Some(284), Some(16), Some(315)]);
    }

    #[test]
    fn normal_current_party_ids_apply_previous_scene_end_of_turn_skill_exit() {
        let scene_1 = BattleScene {
            id: "scene_1".into(),
            preparation_actions: vec![Action::Servant {
                id: "sa_1".into(),
                servant: Some("servant_1".into()),
                skill: Some("skill_3".into()),
                target: None,
            }],
            servant_actions: vec![],
            equipment_actions: vec![],
            command_spell_actions: vec![],
            enemy_target: None,
            attack_priority: vec![],
        };
        let scene_2 = BattleScene {
            id: "scene_2".into(),
            preparation_actions: vec![],
            servant_actions: vec![],
            equipment_actions: vec![],
            command_spell_actions: vec![],
            enemy_target: None,
            attack_priority: vec![],
        };

        let party_ids = normal_current_party_ids_from(
            [Some(315), Some(434), Some(384), Some(11), Some(22), None],
            &[scene_1, scene_2],
            1,
            Some(1),
        );

        assert_eq!(party_ids, [Some(11), Some(434), Some(384)]);
    }

    #[test]
    fn grand_auto_order_change_targets_front_servant_with_most_cards() {
        let cards = vec![
            command_card(0, Some(20), Some("a"), None),
            command_card(1, Some(10), Some("q"), None),
            command_card(2, Some(20), Some("b"), None),
            command_card(3, Some(30), Some("a"), None),
            command_card(4, Some(10), Some("b"), None),
        ];
        let action = grand_auto_order_change_action(
            &cards,
            &[Some(10), Some(20), Some(30)],
            &[grand_config_at(4, 99, "buster", "damage")],
        )
        .unwrap();

        match action {
            Action::Equipment {
                skill,
                order_change: Some(order_change),
                ..
            } => {
                assert_eq!(skill.as_deref(), Some("skill_3"));
                assert_eq!(order_change.front.as_deref(), Some("servant_1"));
                assert_eq!(order_change.back.as_deref(), Some("servant_5"));
            }
            _ => panic!("expected auto Order Change action"),
        }
    }

    #[test]
    fn grand_auto_order_change_skips_when_main_grand_is_frontline() {
        let cards = vec![command_card(0, Some(99), Some("a"), None)];
        assert!(grand_auto_order_change_action(
            &cards,
            &[Some(99), Some(20), Some(30)],
            &[grand_config_at(0, 99, "buster", "damage")],
        )
        .is_none());
    }

    #[test]
    fn startup_action_selected_slot_resolves_to_current_position_after_auto_order_change() {
        let original_ids = [Some(10), Some(20), Some(30), Some(99), None, None];
        let changed_ids = [Some(99), Some(20), Some(30), Some(10), None, None];
        let swapped_out_source = Action::Servant {
            id: "a1".into(),
            servant: Some("servant_1".into()),
            skill: Some("skill_1".into()),
            target: None,
        };
        let main_grand_source = Action::Servant {
            id: "a4".into(),
            servant: Some("servant_4".into()),
            skill: Some("skill_1".into()),
            target: Some("servant_2".into()),
        };
        let unchanged_source = Action::Servant {
            id: "a2".into(),
            servant: Some("servant_2".into()),
            skill: Some("skill_1".into()),
            target: None,
        };
        let swapped_target = Action::Equipment {
            id: "a3".into(),
            skill: Some("skill_1".into()),
            target: Some("servant_1".into()),
            order_change: None,
        };

        assert!(resolve_action_to_current_positions(
            &changed_ids,
            &original_ids,
            &swapped_out_source
        )
        .is_none());
        let resolved =
            resolve_action_to_current_positions(&changed_ids, &original_ids, &main_grand_source)
                .unwrap();
        match resolved {
            Action::Servant {
                servant, target, ..
            } => {
                assert_eq!(servant.as_deref(), Some("servant_1"));
                assert_eq!(target.as_deref(), Some("servant_2"));
            }
            _ => panic!("expected servant action"),
        }
        assert!(resolve_action_to_current_positions(
            &changed_ids,
            &original_ids,
            &unchanged_source
        )
        .is_some());
        assert!(
            resolve_action_to_current_positions(&changed_ids, &original_ids, &swapped_target)
                .is_none()
        );
    }

    #[test]
    fn auto_order_change_startup_flow_replays_control_after_swap() {
        let auto_order_change = Action::Equipment {
            id: "auto_oc".into(),
            skill: Some("skill_3".into()),
            target: None,
            order_change: Some(crate::OrderChangeSelection {
                front: Some("servant_1".into()),
                back: Some("servant_4".into()),
            }),
        };
        let first_control = Action::Equipment {
            id: "control_1".into(),
            skill: Some("skill_1".into()),
            target: Some("servant_1".into()),
            order_change: None,
        };
        let second_control = Action::Equipment {
            id: "control_2".into(),
            skill: Some("skill_2".into()),
            target: Some("servant_2".into()),
            order_change: None,
        };
        let startup = Action::Servant {
            id: "startup_1".into(),
            servant: Some("servant_1".into()),
            skill: Some("skill_1".into()),
            target: None,
        };
        let scene = AdvancedBattleScene {
            id: "advanced_scene_1".into(),
            main_output: None,
            grand_auto_order_change: Some(true),
            command_conditions: Vec::new(),
            control_actions: vec![first_control, second_control],
            startup_actions: vec![startup],
            rules: Vec::new(),
        };

        let actions = advanced_startup_flow_actions(&scene, 2, 1, Some(&auto_order_change));
        let ids: Vec<&str> = actions
            .iter()
            .map(|action| match action {
                Action::Servant { id, .. }
                | Action::Equipment { id, .. }
                | Action::CommandSpell { id, .. } => id.as_str(),
            })
            .collect();

        assert_eq!(ids, vec!["auto_oc", "control_1", "startup_1", "control_2"]);
    }

    #[test]
    fn party_lineup_change_swaps_support_from_configured_back_slot() {
        let mut ids = [Some(8), Some(434), Some(384), Some(11), Some(22), Some(999)];
        let action = Action::Equipment {
            id: "a1".into(),
            skill: Some("skill_3".into()),
            target: None,
            order_change: Some(crate::OrderChangeSelection {
                front: Some("servant_2".into()),
                back: Some("servant_6".into()),
            }),
        };

        apply_party_lineup_change(&mut ids, &action);

        assert_eq!(
            ids,
            [Some(8), Some(999), Some(384), Some(11), Some(22), Some(434)]
        );
    }

    #[test]
    fn party_lineup_change_does_not_apply_end_of_turn_skill_by_default() {
        let mut ids = [Some(388), Some(434), Some(384), Some(11), Some(22), None];
        let action = Action::Servant {
            id: "a1".into(),
            servant: Some("servant_1".into()),
            skill: Some("skill_2".into()),
            target: None,
        };

        apply_party_lineup_change(&mut ids, &action);

        assert_eq!(
            ids,
            [Some(388), Some(434), Some(384), Some(11), Some(22), None]
        );
    }

    #[test]
    fn party_lineup_change_applies_end_of_turn_servant_skill_withdraw_rule() {
        let mut ids = [Some(388), Some(434), Some(384), Some(11), Some(22), None];
        let action = Action::Servant {
            id: "a1".into(),
            servant: Some("servant_1".into()),
            skill: Some("skill_2".into()),
            target: None,
        };

        apply_party_lineup_change_at(&mut ids, &action, ChangeOrderTiming::EndOfTurn);

        assert_eq!(
            ids,
            [Some(11), Some(434), Some(384), Some(388), Some(22), None]
        );
    }

    #[test]
    fn party_lineup_change_removes_habetrot_at_end_of_turn_after_third_skill() {
        let mut ids = [Some(315), Some(434), Some(384), Some(11), Some(22), None];
        let action = Action::Servant {
            id: "a1".into(),
            servant: Some("servant_1".into()),
            skill: Some("skill_3".into()),
            target: None,
        };

        apply_party_lineup_change_at(&mut ids, &action, ChangeOrderTiming::EndOfTurn);

        assert_eq!(ids, [Some(11), Some(434), Some(384), Some(22), None, None]);
    }

    #[test]
    fn party_lineup_change_removes_ultimate_elisabeth_at_end_of_turn_after_third_skill() {
        let mut ids = [Some(8), Some(458), Some(384), Some(11), Some(22), None];
        let action = Action::Servant {
            id: "a1".into(),
            servant: Some("servant_2".into()),
            skill: Some("skill_3".into()),
            target: None,
        };

        apply_party_lineup_change_at(&mut ids, &action, ChangeOrderTiming::EndOfTurn);

        assert_eq!(ids, [Some(8), Some(11), Some(384), Some(22), None, None]);
    }

    #[test]
    fn action_frontline_available_rejects_source_out_of_frontline() {
        let ids = [Some(8), Some(434), Some(384), Some(11), Some(22), None];
        let action = Action::Servant {
            id: "a1".into(),
            servant: Some("servant_4".into()),
            skill: Some("skill_1".into()),
            target: None,
        };

        assert!(!action_frontline_available(&ids, &action));
    }

    #[test]
    fn action_frontline_available_rejects_missing_frontline_target() {
        let ids = [Some(8), None, Some(384), Some(11), Some(22), None];
        let action = Action::Servant {
            id: "a1".into(),
            servant: Some("servant_1".into()),
            skill: Some("skill_1".into()),
            target: Some("servant_2".into()),
        };

        assert!(!action_frontline_available(&ids, &action));
    }

    #[test]
    fn advanced_auto_np_output_prefers_ready_np_and_arts_cards() {
        let scene = AdvancedBattleScene {
            id: "advanced_scene_1".into(),
            main_output: Some(crate::AdvancedMainOutput {
                servant: Some("servant_1".into()),
                output_type: Some(AdvancedOutputType::Np),
                np_card: Some("arts".into()),
            }),
            grand_auto_order_change: None,
            command_conditions: Vec::new(),
            control_actions: Vec::new(),
            startup_actions: Vec::new(),
            rules: Vec::new(),
        };
        let cards = vec![
            command_card(0, Some(10), Some("b"), None),
            command_card(1, Some(10), Some("a"), None),
            command_card(2, Some(20), Some("a"), None),
            command_card(3, Some(30), Some("q"), None),
            command_card(4, Some(30), Some("b"), None),
        ];
        let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
        let picks = choose_advanced_auto_picks(
            &scene,
            &cards,
            &nps,
            &[Some(10), Some(20), Some(30)],
            &[],
            &GrandCardStrategy::default(),
        );

        assert!(picks
            .iter()
            .any(|pick| matches!(pick, Pick::Np { slot: 0, .. })));
        assert!(picks
            .iter()
            .any(|pick| matches!(pick, Pick::Card { slot: 1, .. })));
        assert!(picks
            .iter()
            .any(|pick| matches!(pick, Pick::Card { slot: 2, .. })));
    }

    #[test]
    fn grand_auto_main_exquisite_damage_puts_np_last() {
        let scene = empty_advanced_scene();
        let cards = vec![
            command_card(0, Some(10), Some("q"), None),
            command_card(1, Some(10), Some("a"), None),
            command_card(2, Some(20), Some("b"), None),
            command_card(3, Some(30), Some("q"), None),
            command_card(4, Some(30), Some("a"), None),
        ];
        let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
        let grands = vec![grand_config(10, "buster", "damage")];
        let picks = choose_advanced_auto_picks(
            &scene,
            &cards,
            &nps,
            &[Some(10), Some(20), Some(30)],
            &grands,
            &GrandCardStrategy::default(),
        );

        assert_eq!(pick_labels(&picks), vec!["C0", "C1", "NP0"]);
    }

    #[test]
    fn grand_auto_same_color_chain_places_np_first() {
        let scene = empty_advanced_scene();
        let cards = vec![
            command_card(0, Some(10), Some("b"), None),
            command_card(1, Some(10), Some("b"), None),
            command_card(2, Some(20), Some("a"), None),
            command_card(3, Some(30), Some("q"), None),
            command_card(4, Some(30), Some("a"), None),
        ];
        let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
        let grands = vec![grand_config(10, "buster", "np")];
        let picks = choose_advanced_auto_picks(
            &scene,
            &cards,
            &nps,
            &[Some(10), Some(20), Some(30)],
            &grands,
            &GrandCardStrategy::default(),
        );

        assert_eq!(pick_labels(&picks), vec!["NP0", "C0", "C1"]);
    }

    #[test]
    fn grand_auto_fallback_uses_deputy_np_as_overcharge_before_main_np() {
        let scene = empty_advanced_scene();
        let cards = vec![
            command_card(0, Some(10), Some("a"), None),
            command_card(1, Some(20), Some("q"), None),
            command_card(2, Some(30), Some("b"), None),
            command_card(3, Some(30), Some("b"), None),
            command_card(4, Some(30), Some("b"), None),
        ];
        let nps = vec![np_slot(0, true), np_slot(1, true), np_slot(2, false)];
        let grands = vec![
            grand_config(10, "arts", "np"),
            grand_config(20, "quick", "damage"),
        ];
        let picks = choose_advanced_auto_picks(
            &scene,
            &cards,
            &nps,
            &[Some(10), Some(20), Some(30)],
            &grands,
            &GrandCardStrategy::default(),
        );

        assert_eq!(pick_labels(&picks), vec!["NP1", "NP0", "C0"]);
    }

    /// Reproduces the real-world Iori (id 405, buster NP) hand:
    /// front [405, 7, 8] with hand [a/8, a/405, a/8, b/7, a/7] and NP1
    /// (slot 0) ready. The hand can form a tier-3 "same-color arts +
    /// has main 405" combo, but main NP is buster and only 1 buster
    /// card is available, so the NP cannot fit any same-color chain.
    /// Pre-fix the picker chose the all-arts combo and let the ready
    /// main NP rot. With the new "main NP ready" tier, the picker must
    /// include NP0 in the final picks.
    #[test]
    fn grand_auto_fires_ready_main_np_when_color_does_not_fit_same_color_chain() {
        let scene = empty_advanced_scene();
        let cards = vec![
            command_card(0, Some(8), Some("a"), None),
            command_card(1, Some(405), Some("a"), None),
            command_card(2, Some(8), Some("a"), None),
            command_card(3, Some(7), Some("b"), None),
            command_card(4, Some(7), Some("a"), None),
        ];
        let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
        let grands = vec![grand_config(405, "buster", "damage")];
        let picks = choose_advanced_auto_picks(
            &scene,
            &cards,
            &nps,
            &[Some(405), Some(7), Some(8)],
            &grands,
            &GrandCardStrategy::default(),
        );

        assert!(
            picks
                .iter()
                .any(|pick| matches!(pick, Pick::Np { slot: 0, .. })),
            "picker dropped the ready main NP: {:?}",
            pick_labels(&picks)
        );
    }

    /// A ready main NP must outrank a deputy three-card same-color
    /// chain, even though tier-4 deputy chains used to score above any
    /// non-main combo. Front party: [10 (main), 20 (deputy), 30].
    /// Hand contains a clean three-arts deputy chain plus the main NP
    /// alone with no support cards from main, so the ONLY way to fire
    /// the main NP is to give up the deputy chain.
    #[test]
    fn grand_auto_main_np_outranks_deputy_three_card_chain() {
        let scene = empty_advanced_scene();
        let cards = vec![
            command_card(0, Some(20), Some("a"), None),
            command_card(1, Some(20), Some("a"), None),
            command_card(2, Some(20), Some("a"), None),
            command_card(3, Some(30), Some("b"), None),
            command_card(4, Some(30), Some("q"), None),
        ];
        let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
        let grands = vec![
            grand_config(10, "buster", "damage"),
            grand_config_at(1, 20, "arts", "np"),
        ];
        let picks = choose_advanced_auto_picks(
            &scene,
            &cards,
            &nps,
            &[Some(10), Some(20), Some(30)],
            &grands,
            &GrandCardStrategy::default(),
        );

        assert!(
            picks
                .iter()
                .any(|pick| matches!(pick, Pick::Np { slot: 0, .. })),
            "main NP was not fired: {:?}",
            pick_labels(&picks)
        );
    }

    #[test]
    fn grand_auto_respects_custom_chain_priority_order() {
        let scene = empty_advanced_scene();
        let cards = vec![
            command_card(0, Some(20), Some("a"), None),
            command_card(1, Some(20), Some("a"), None),
            command_card(2, Some(20), Some("a"), None),
            command_card(3, Some(30), Some("b"), None),
            command_card(4, Some(30), Some("q"), None),
        ];
        let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
        let grands = vec![
            grand_config(10, "buster", "damage"),
            grand_config_at(1, 20, "arts", "np"),
        ];
        let strategy = GrandCardStrategy {
            chain_priority: vec![
                GrandChainPriorityItem::DeputyBraveChain,
                GrandChainPriorityItem::MainReadyNp,
                GrandChainPriorityItem::MainBraveChain,
                GrandChainPriorityItem::MainColorChain,
                GrandChainPriorityItem::DeputyColorChain,
                GrandChainPriorityItem::Fallback,
            ],
        };
        let picks = choose_advanced_auto_picks(
            &scene,
            &cards,
            &nps,
            &[Some(10), Some(20), Some(30)],
            &grands,
            &strategy,
        );

        assert_eq!(pick_labels(&picks), vec!["C0", "C1", "C2"]);
    }

    /// A ready DEPUTY NP, on its own, must NOT outrank a main same-
    /// color chain (tier 3). Only the main NP gets the priority bump,
    /// because the user expectation of "fire NP instead of charging
    /// it" is specifically about the main output.
    #[test]
    fn grand_auto_deputy_ready_np_does_not_outrank_main_same_color_chain() {
        let scene = empty_advanced_scene();
        let cards = vec![
            command_card(0, Some(10), Some("a"), None),
            command_card(1, Some(10), Some("a"), None),
            command_card(2, Some(30), Some("a"), None),
            command_card(3, Some(30), Some("b"), None),
            command_card(4, Some(30), Some("q"), None),
        ];
        let nps = vec![np_slot(0, false), np_slot(1, true), np_slot(2, false)];
        let grands = vec![
            grand_config(10, "arts", "np"),
            grand_config_at(1, 20, "buster", "damage"),
        ];
        let picks = choose_advanced_auto_picks(
            &scene,
            &cards,
            &nps,
            &[Some(10), Some(20), Some(30)],
            &grands,
            &GrandCardStrategy::default(),
        );

        let labels = pick_labels(&picks);
        assert!(
            !labels.iter().any(|label| label == "NP1"),
            "deputy NP should not displace main same-color chain: {:?}",
            labels
        );
    }

    #[test]
    fn run_config_round_trips_support_servant_id_and_repeat_flag() {
        let mut payload = minimal_run_config_json();
        payload["supportServantId"] = serde_json::json!(284);
        payload["supportNoblePhantasmLevelMin"] = serde_json::json!(2);
        payload["supportSkillLevelMins"] = serde_json::json!([10, null, 9]);
        payload["supportAppendSkillLevelMins"] = serde_json::json!([null, 10, null, null, 6]);
        payload["repeatMission"] = serde_json::json!(true);
        payload["maxMissionRuns"] = serde_json::json!(3);
        payload["apRecoveryItems"] = serde_json::json!(["gold", "bronze"]);
        let cfg: RunConfig = serde_json::from_value(payload).unwrap();
        assert_eq!(cfg.support_servant_id, Some(284));
        assert_eq!(cfg.support_noble_phantasm_level_min, Some(2));
        assert_eq!(cfg.support_skill_level_mins, [Some(10), None, Some(9)]);
        assert_eq!(
            cfg.support_append_skill_level_mins,
            [None, Some(10), None, None, Some(6)]
        );
        assert_eq!(cfg.repeat_mission, true);
        assert_eq!(cfg.max_mission_runs, Some(3));
        assert_eq!(
            cfg.ap_recovery_items,
            vec![ApRecoveryItem::Gold, ApRecoveryItem::Bronze]
        );
    }

    #[test]
    fn support_level_meets_treats_none_requirement_as_any() {
        assert!(support_level_meets(None, None));
        assert!(support_level_meets(Some(1), None));
        assert!(!support_level_meets(None, Some(1)));
        assert!(!support_level_meets(Some(4), Some(5)));
        assert!(support_level_meets(Some(5), Some(5)));
        assert!(support_level_meets(Some(10), Some(5)));
    }

    fn support_row(
        panel: Option<&str>,
        skills: Vec<Option<u32>>,
        append: Vec<Option<u32>>,
    ) -> SupportRowMatch {
        let region = NormRect {
            x: 0.1,
            y: 0.5,
            w: 0.4,
            h: 0.1,
        };
        SupportRowMatch {
            row_region: region,
            tap: Point::new(0.3, 0.55),
            name_text: "哈贝特洛特".into(),
            name_score: 1.0,
            name_region: region,
            np_text: "为你纺织的时光之轮等级5".into(),
            np_score: 1.0,
            np_region: region,
            score_anchor: None,
            np_matched_name: "为你纺织的时光之轮".into(),
            np_level: Some(5),
            skill_panel: panel.map(str::to_string),
            skill_levels: skills,
            append_skill_levels: append,
            skill_level_diagnostics: Vec::new(),
        }
    }

    #[test]
    fn support_level_progress_resets_panel_toggle_taps_on_new_candidate() {
        // The runner increments `panel_toggle_taps` outside the matcher,
        // but the matcher owns the lifecycle: whenever it sees a new
        // `candidate_key` it must also reset the tap counter so the
        // budget only applies to the current row. Across calls that
        // keep the same key the counter must be left untouched.
        let mut payload = minimal_run_config_json();
        payload["supportSkillLevelMins"] = serde_json::json!([10, null, null]);
        payload["supportAppendSkillLevelMins"] = serde_json::json!([null, 10, null, null, null]);
        let cfg: RunConfig = serde_json::from_value(payload).unwrap();
        let mut progress = SupportLevelPanelProgress::default();

        // First call: row A on the owned panel — owned passes, still
        // need append → WaitingForPanel; tap counter untouched.
        support_row_matches_level_requirements_with_progress(
            Server::Cn,
            &cfg,
            &support_row(Some("owned"), vec![Some(10), None, None], vec![]),
            &mut progress,
        );
        assert!(progress.owned_met);
        progress.panel_toggle_taps = 2;

        // Second call: row A again, same panel — matcher must not
        // clobber the counter the runner just incremented.
        support_row_matches_level_requirements_with_progress(
            Server::Cn,
            &cfg,
            &support_row(Some("owned"), vec![Some(10), None, None], vec![]),
            &mut progress,
        );
        assert_eq!(progress.panel_toggle_taps, 2);

        // Third call: row B (different y → different candidate_key) —
        // matcher resets the whole accumulator, including tap counter.
        let mut row_b = support_row(Some("owned"), vec![Some(10), None, None], vec![]);
        row_b.row_region.y = 0.62;
        support_row_matches_level_requirements_with_progress(
            Server::Cn,
            &cfg,
            &row_b,
            &mut progress,
        );
        assert_eq!(progress.panel_toggle_taps, 0);
    }

    #[test]
    fn support_level_filter_accumulates_owned_and_append_panels() {
        let mut payload = minimal_run_config_json();
        payload["supportSkillLevelMins"] = serde_json::json!([10, null, 9]);
        payload["supportAppendSkillLevelMins"] = serde_json::json!([null, 10, null, null, null]);
        let cfg: RunConfig = serde_json::from_value(payload).unwrap();
        let mut progress = SupportLevelPanelProgress::default();

        assert_eq!(
            support_row_matches_level_requirements_with_progress(
                Server::Cn,
                &cfg,
                &support_row(Some("owned"), vec![Some(10), Some(1), Some(9)], vec![]),
                &mut progress,
            ),
            SupportLevelFilter::WaitingForPanel
        );
        assert!(progress.owned_met);
        assert!(!progress.append_met);

        assert_eq!(
            support_row_matches_level_requirements_with_progress(
                Server::Cn,
                &cfg,
                &support_row(
                    Some("append"),
                    vec![],
                    vec![None, Some(10), None, None, None],
                ),
                &mut progress,
            ),
            SupportLevelFilter::Pass
        );
    }

    #[test]
    fn support_level_filter_fails_visible_panel_before_waiting_for_other_panel() {
        let mut payload = minimal_run_config_json();
        payload["supportSkillLevelMins"] = serde_json::json!([10, null, null]);
        payload["supportAppendSkillLevelMins"] = serde_json::json!([null, 10, null, null, null]);
        let cfg: RunConfig = serde_json::from_value(payload).unwrap();
        let mut progress = SupportLevelPanelProgress::default();

        assert_eq!(
            support_row_matches_level_requirements_with_progress(
                Server::Cn,
                &cfg,
                &support_row(Some("owned"), vec![Some(9), Some(10), Some(10)], vec![]),
                &mut progress,
            ),
            SupportLevelFilter::Fail("持有技能 1 ≥ 10（实际 9）".into())
        );
        assert!(!progress.owned_met);
    }

    #[test]
    fn support_level_filter_reports_first_mismatch_per_panel() {
        // The Fail payload is what the runner surfaces in the UI log,
        // so pin the format down: NP / 持有 / 追加 each get their own
        // labelled reason, with the offending slot index (1-based)
        // and the OCR'd actual value (or `-` when not detected).
        let mut payload = minimal_run_config_json();
        payload["supportNoblePhantasmLevelMin"] = serde_json::json!(5);
        payload["supportSkillLevelMins"] = serde_json::json!([10, 10, 10]);
        payload["supportAppendSkillLevelMins"] = serde_json::json!([null, 10, null, null, null]);
        let cfg: RunConfig = serde_json::from_value(payload).unwrap();

        // NP miss is checked before the panel branch is even consulted,
        // so a row whose owned panel would otherwise pass still fails
        // early with an NP reason.
        let mut progress = SupportLevelPanelProgress::default();
        let mut np_low = support_row(Some("owned"), vec![Some(10), Some(10), Some(10)], vec![]);
        np_low.np_level = Some(3);
        assert_eq!(
            support_row_matches_level_requirements_with_progress(
                Server::Cn,
                &cfg,
                &np_low,
                &mut progress,
            ),
            SupportLevelFilter::Fail("宝具 ≥ 5（实际 3）".into())
        );

        // Owned miss reports the first failing slot — slot 2 here —
        // even though slot 3 also fails downstream.
        let mut progress = SupportLevelPanelProgress::default();
        assert_eq!(
            support_row_matches_level_requirements_with_progress(
                Server::Cn,
                &cfg,
                &support_row(Some("owned"), vec![Some(10), Some(8), Some(7)], vec![]),
                &mut progress,
            ),
            SupportLevelFilter::Fail("持有技能 2 ≥ 10（实际 8）".into())
        );

        // Append miss with `None` value ⇒ formatted as `-` so
        // "skill icon never OCR'd" reads distinctly from "level 0".
        let mut progress = SupportLevelPanelProgress::default();
        assert_eq!(
            support_row_matches_level_requirements_with_progress(
                Server::Cn,
                &cfg,
                &support_row(Some("append"), vec![], vec![None, None, None, None, None],),
                &mut progress,
            ),
            SupportLevelFilter::Fail("追加技能 2 ≥ 10（实际 -）".into())
        );
    }

    #[test]
    fn ap_recovery_template_maps_frontend_items_to_template_keys() {
        assert_eq!(
            ap_recovery_template(ApRecoveryItem::Rainbow),
            ApRecoveryTemplate {
                item: ApRecoveryItem::Rainbow,
                page: ApRecoveryPage::Top,
                label: "圣晶石",
                template_key: "items/item_saint_quartz",
            }
        );
        assert_eq!(
            ap_recovery_template(ApRecoveryItem::Bronze),
            ApRecoveryTemplate {
                item: ApRecoveryItem::Bronze,
                page: ApRecoveryPage::Bottom,
                label: "青铜苹果",
                template_key: "items/item_apple_bronzed_cobalt",
            }
        );
        assert_eq!(
            ap_recovery_template(ApRecoveryItem::Copper),
            ApRecoveryTemplate {
                item: ApRecoveryItem::Copper,
                page: ApRecoveryPage::Bottom,
                label: "赤铜苹果",
                template_key: "items/item_apple_bronze",
            }
        );
    }

    #[test]
    fn ap_recovery_candidates_for_page_preserves_priority_within_page() {
        let top = ap_recovery_candidates_for_page(
            &[
                ApRecoveryItem::Silver,
                ApRecoveryItem::Copper,
                ApRecoveryItem::Gold,
                ApRecoveryItem::Rainbow,
            ],
            ApRecoveryPage::Top,
        );
        assert_eq!(
            top.iter().map(|item| item.item).collect::<Vec<_>>(),
            vec![
                ApRecoveryItem::Gold,
                ApRecoveryItem::Silver,
                ApRecoveryItem::Rainbow,
            ]
        );

        let bottom = ap_recovery_candidates_for_page(
            &[
                ApRecoveryItem::Silver,
                ApRecoveryItem::Copper,
                ApRecoveryItem::Gold,
                ApRecoveryItem::Bronze,
            ],
            ApRecoveryPage::Bottom,
        );
        assert_eq!(
            bottom.iter().map(|item| item.item).collect::<Vec<_>>(),
            vec![ApRecoveryItem::Bronze, ApRecoveryItem::Copper]
        );
    }

    #[test]
    fn unknown_element_error_helper_matches_exact_lookup() {
        assert!(is_unknown_element_error(
            "unknown element: SupportSelect.dialog_refresh_support",
            "SupportSelect",
            "dialog_refresh_support",
        ));
        assert!(!is_unknown_element_error(
            "unknown element: SupportSelect.support_scroll_end",
            "SupportSelect",
            "dialog_refresh_support",
        ));
    }

    // --- tick_scene_state -----------------------------------------------
    //
    // These cover the full state-transition matrix for the BATTLE m/n
    // reading. The previous implementation reset `skills_executed` (and
    // therefore re-fired skills) on every iteration where the CV read
    // returned `None`, which double-fired skills any time the NP overlay
    // covered the strip. The helper now treats failed reads as "stay
    // put" so the bug can't come back without breaking these tests.

    #[test]
    fn tick_scene_state_first_successful_read_snaps_index_to_screen_scene() {
        // First successful read of scene 1 from a fresh runner. The
        // snap takes m=1 → index 0, which matches the runner's default
        // starting index, so behaviour is identical to the legacy
        // "lock in but don't advance" semantics for the m=1 case.
        let tick = tick_scene_state(None, 0, None, Some(1));
        assert_eq!(
            tick,
            SceneTick {
                last_screen_scene: Some(1),
                current_scene_index: 0,
                needs_exec: true,
            }
        );
    }

    #[test]
    fn tick_scene_state_first_successful_read_at_scene_two_snaps_to_index_one() {
        // Mid-quest start: the runner is launched while the screen is
        // already on scene 2/3. The first successful read must snap
        // current_scene_index to 1 so the user's second configured
        // command block runs — without the snap we would execute
        // block 0 for the actual scene 2 and only advance after the
        // *next* on-screen transition (i.e. when the screen moves to
        // 3/3), wasting block 1 entirely.
        let tick = tick_scene_state(None, 0, None, Some(2));
        assert_eq!(
            tick,
            SceneTick {
                last_screen_scene: Some(2),
                current_scene_index: 1,
                needs_exec: true,
            }
        );
    }

    #[test]
    fn tick_scene_state_delayed_first_read_at_scene_two_replaces_default_exec() {
        // First poll's CV read failed (e.g. NP overlay covered the
        // strip), so the runner emitted the default block 0. The next
        // poll succeeds with m=2: we must snap the index to 1 and
        // re-execute, otherwise the user's scene-2 block never fires
        // for the entire scene.
        let tick = tick_scene_state(None, 0, Some(0), Some(2));
        assert_eq!(
            tick,
            SceneTick {
                last_screen_scene: Some(2),
                current_scene_index: 1,
                needs_exec: true,
            }
        );
    }

    #[test]
    fn tick_scene_state_first_successful_read_at_scene_three_snaps_to_index_two() {
        // Same as the scene-2 case but proves the snap is general,
        // not a special-case of m=2.
        let tick = tick_scene_state(None, 0, None, Some(3));
        assert_eq!(
            tick,
            SceneTick {
                last_screen_scene: Some(3),
                current_scene_index: 2,
                needs_exec: true,
            }
        );
    }

    #[test]
    fn tick_scene_state_failed_read_after_lockin_holds_state_and_skips_exec() {
        // We executed scene 0 last iteration; the CV now fails (NP
        // overlay). Index must not advance, last_screen_scene must
        // stay at the previously locked value, and needs_exec must be
        // false so we don't double-fire skills.
        let tick = tick_scene_state(Some(1), 0, Some(0), None);
        assert_eq!(
            tick,
            SceneTick {
                last_screen_scene: Some(1),
                current_scene_index: 0,
                needs_exec: false,
            }
        );
    }

    #[test]
    fn tick_scene_state_same_scene_read_skips_exec() {
        let tick = tick_scene_state(Some(1), 0, Some(0), Some(1));
        assert_eq!(
            tick,
            SceneTick {
                last_screen_scene: Some(1),
                current_scene_index: 0,
                needs_exec: false,
            }
        );
    }

    #[test]
    fn tick_scene_state_real_transition_advances_and_triggers_exec() {
        let tick = tick_scene_state(Some(1), 0, Some(0), Some(2));
        assert_eq!(
            tick,
            SceneTick {
                last_screen_scene: Some(2),
                current_scene_index: 1,
                needs_exec: true,
            }
        );
    }

    // --- command_spell_index -------------------------------------------

    #[test]
    fn command_spell_index_maps_known_names_to_dialog_rows() {
        // Dialog row order is documented next to `COMMAND_SPELL_OPTIONS`:
        // 0 = 宝具解放 (np_release), 1 = 灵基修复 (restore). If anyone
        // swaps the array entries without updating the helper the runner
        // would tap the wrong spell — this test pins the mapping.
        assert_eq!(command_spell_index(Some("np_release")), Some(0));
        assert_eq!(command_spell_index(Some("restore")), Some(1));
    }

    #[test]
    fn command_spell_index_returns_none_for_unknown_or_missing() {
        // Unknown / missing spell names cause the runner's loop to
        // `continue` instead of tapping a phantom row; mirrors how
        // `skill_position` handles malformed inputs.
        assert_eq!(command_spell_index(None), None);
        assert_eq!(command_spell_index(Some("")), None);
        assert_eq!(command_spell_index(Some("self_destruct")), None);
    }

    #[test]
    fn order_change_slot_position_requires_one_front_and_one_back_range() {
        let front = order_change_slot_position(Some("servant_1"), 0..3).unwrap();
        assert_eq!(
            (front.x, front.y),
            (ORDER_CHANGE_SLOTS[0].x, ORDER_CHANGE_SLOTS[0].y)
        );

        let back = order_change_slot_position(Some("servant_4"), 3..6).unwrap();
        assert_eq!(
            (back.x, back.y),
            (ORDER_CHANGE_SLOTS[3].x, ORDER_CHANGE_SLOTS[3].y)
        );

        assert!(order_change_slot_position(Some("servant_4"), 0..3).is_none());
        assert!(order_change_slot_position(Some("servant_2"), 3..6).is_none());
    }

    #[test]
    fn enemy_target_position_maps_six_2k_reference_points() {
        let expected = [
            (282.0 / 2560.0, 66.0 / 1440.0),
            (682.0 / 2560.0, 66.0 / 1440.0),
            (1082.0 / 2560.0, 66.0 / 1440.0),
            (85.0 / 2560.0, 263.0 / 1440.0),
            (485.0 / 2560.0, 263.0 / 1440.0),
            (885.0 / 2560.0, 263.0 / 1440.0),
        ];

        for (index, (x, y)) in expected.into_iter().enumerate() {
            let point = enemy_target_position(Some(&format!("enemy_{}", index + 1))).unwrap();
            assert!((point.x - x).abs() < f64::EPSILON);
            assert!((point.y - y).abs() < f64::EPSILON);
        }

        assert!(enemy_target_position(None).is_none());
        assert!(enemy_target_position(Some("enemy_7")).is_none());
        assert!(enemy_target_position(Some("servant_1")).is_none());
    }

    #[test]
    fn debug_coordinates_include_all_enemy_targets() {
        let coords = debug_coordinates();
        let group = coords
            .groups
            .iter()
            .find(|group| group.id == "enemyTargets")
            .expect("enemy target coordinate group");

        let labels: Vec<&str> = group.points.iter().map(|point| point.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["Enemy1", "Enemy2", "Enemy3", "Enemy4", "Enemy5", "Enemy6"]
        );
    }

    #[test]
    fn scene_preparation_actions_preserves_configured_row_order() {
        let scene = BattleScene {
            id: "scene_1".into(),
            preparation_actions: vec![
                Action::Equipment {
                    id: "eq_1".into(),
                    skill: Some("skill_2".into()),
                    target: None,
                    order_change: None,
                },
                Action::Servant {
                    id: "sa_1".into(),
                    servant: Some("servant_1".into()),
                    skill: Some("skill_3".into()),
                    target: Some("servant_2".into()),
                },
                Action::CommandSpell {
                    id: "cs_1".into(),
                    spell: Some("restore".into()),
                    target: Some("servant_1".into()),
                },
            ],
            servant_actions: vec![],
            equipment_actions: vec![],
            command_spell_actions: vec![],
            enemy_target: None,
            attack_priority: vec![],
        };

        let kinds: Vec<&str> = scene_preparation_actions(&scene)
            .map(|action| match action {
                Action::Servant { .. } => "servant",
                Action::Equipment { .. } => "equipment",
                Action::CommandSpell { .. } => "commandSpell",
            })
            .collect();

        assert_eq!(kinds, vec!["equipment", "servant", "commandSpell"]);
    }

    #[test]
    fn tick_scene_state_failed_first_read_executes_default_index_zero() {
        // CV fails before we ever locked in a scene → we still want to
        // execute the configured first block so the runner doesn't
        // stall on a missing read. Subsequent successful reads must
        // not re-trigger execution for the same index.
        let first = tick_scene_state(None, 0, None, None);
        assert_eq!(
            first,
            SceneTick {
                last_screen_scene: None,
                current_scene_index: 0,
                needs_exec: true,
            }
        );
        // Caller would then mark executed_scene_index = Some(0). Next
        // iteration: still no successful read.
        let second = tick_scene_state(None, 0, Some(0), None);
        assert_eq!(second.needs_exec, false);
        // First successful read of scene 1: locks in but does NOT
        // advance the index (we never observed a transition), and
        // does NOT re-execute (executed already at index 0).
        let third = tick_scene_state(None, 0, Some(0), Some(1));
        assert_eq!(
            third,
            SceneTick {
                last_screen_scene: Some(1),
                current_scene_index: 0,
                needs_exec: false,
            }
        );
    }

    #[test]
    fn grand_support_section_requires_seen_then_two_misses_before_exhausted() {
        let mut seen = false;
        let mut misses = 0;

        assert!(!support_grand_section_exhausted_after_probe(
            Some(false),
            &mut seen,
            &mut misses
        ));
        assert!(!seen);
        assert_eq!(misses, 0);

        assert!(!support_grand_section_exhausted_after_probe(
            Some(true),
            &mut seen,
            &mut misses
        ));
        assert!(seen);
        assert_eq!(misses, 0);

        assert!(!support_grand_section_exhausted_after_probe(
            Some(false),
            &mut seen,
            &mut misses
        ));
        assert_eq!(misses, 1);
        assert!(support_grand_section_exhausted_after_probe(
            Some(false),
            &mut seen,
            &mut misses
        ));
    }

    #[test]
    fn grand_support_section_ignores_unavailable_probe() {
        let mut seen = true;
        let mut misses = 1;

        assert!(!support_grand_section_exhausted_after_probe(
            None,
            &mut seen,
            &mut misses
        ));
        assert!(seen);
        assert_eq!(misses, 1);
    }

    fn anchor_at_y(y: f64) -> NormRect {
        NormRect {
            x: 0.846,
            y,
            w: 0.079,
            h: 0.05,
        }
    }

    #[test]
    fn scroll_delta_falls_back_to_fixed_when_no_anchors() {
        assert!(
            (scroll_support_list_delta(&[]) - SUPPORT_SCROLL_FALLBACK_DELTA).abs() < 1e-9
        );
    }

    #[test]
    fn scroll_delta_moves_last_anchor_to_first_row_target() {
        let anchors = vec![
            anchor_at_y(0.364),
            anchor_at_y(0.642),
            anchor_at_y(0.919),
        ];
        let delta = scroll_support_list_delta(&anchors);
        assert!((delta - (0.919 - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y)).abs() < 1e-9);
        let new_position_of_last_button = 0.919 - delta;
        assert!(
            (new_position_of_last_button - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y).abs() < 1e-9
        );
    }

    #[test]
    fn scroll_delta_uses_last_anchor_even_with_two_visible_buttons() {
        let anchors = vec![anchor_at_y(0.62), anchor_at_y(0.92)];
        let delta = scroll_support_list_delta(&anchors);
        assert!(
            (delta - (0.92 - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y)).abs() < 1e-9,
            "got {delta}"
        );
    }

    #[test]
    fn scroll_delta_does_not_extrapolate_clipped_offscreen_buttons() {
        let anchors = vec![anchor_at_y(0.462), anchor_at_y(0.740)];
        let delta = scroll_support_list_delta(&anchors);
        assert!(
            (delta - (0.740 - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y)).abs() < 1e-9,
            "got {delta}"
        );
    }

    #[test]
    fn scroll_delta_uses_last_anchor_near_screen_edge() {
        let anchors = vec![anchor_at_y(0.55), anchor_at_y(0.90)];
        let delta = scroll_support_list_delta(&anchors);
        assert!(
            (delta - (0.90 - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y)).abs() < 1e-9,
            "got {delta}"
        );
    }

    #[test]
    fn scroll_delta_uses_single_visible_button() {
        let anchors = vec![anchor_at_y(0.50)];
        let delta = scroll_support_list_delta(&anchors);
        assert!((delta - (0.50 - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y)).abs() < 1e-9);
    }

    #[test]
    fn scroll_delta_clamps_unusually_large_distance() {
        // Pathological case: 4+ visible cards with the bottom button
        // way down at y=0.99. Even though the raw delta would be 0.71,
        // the clamp keeps the swipe inside `SUPPORT_SCROLL_MAX_DELTA`
        // so a misdetection can't fling the list past the bottom.
        let anchors = vec![
            anchor_at_y(0.28),
            anchor_at_y(0.50),
            anchor_at_y(0.72),
            anchor_at_y(0.99),
        ];
        let delta = scroll_support_list_delta(&anchors);
        assert!((delta - SUPPORT_SCROLL_MAX_DELTA).abs() < 1e-9);
    }

    #[test]
    fn scroll_delta_independent_of_anchor_input_order() {
        // Runner must not depend on sidecar returning anchors sorted.
        let sorted = vec![anchor_at_y(0.30), anchor_at_y(0.55), anchor_at_y(0.80)];
        let shuffled = vec![anchor_at_y(0.80), anchor_at_y(0.30), anchor_at_y(0.55)];
        let from_sorted = scroll_support_list_delta(&sorted);
        let from_shuffled = scroll_support_list_delta(&shuffled);
        assert!((from_sorted - from_shuffled).abs() < 1e-9);
    }

    #[test]
    fn format_scroll_debug_lists_sorted_anchor_positions() {
        // The shuffled-input case is intentional: the message must be
        // human-readable regardless of detector order so an operator
        // reading the debug log can quickly eyeball whether the row
        // pitch looks right.
        let anchors = vec![anchor_at_y(0.80), anchor_at_y(0.30), anchor_at_y(0.55)];
        let msg = format_scroll_debug(&anchors, 0.500, 0.78, 0.28, 588, 400);
        assert!(
            msg.contains("0.300, 0.550, 0.800"),
            "expected sorted anchor list in message, got: {msg}"
        );
        assert!(msg.contains("Δ=0.500"));
        assert!(msg.contains("n=3"));
        assert!(msg.contains("swipe=0.78→0.28"));
        assert!(msg.contains("(588ms+400ms settle)"));
    }

    #[test]
    fn format_scroll_debug_handles_empty_anchors() {
        let msg = format_scroll_debug(
            &[],
            SUPPORT_SCROLL_FALLBACK_DELTA,
            0.78,
            0.38,
            SUPPORT_SCROLL_MIN_DURATION_MS,
            SUPPORT_SCROLL_SETTLE_MS,
        );
        assert!(msg.contains("无"), "empty anchors should render as 无, got: {msg}");
        assert!(msg.contains("n=0"));
    }

    #[test]
    fn scroll_duration_scales_linearly_with_delta_within_bounds() {
        // Mid-range delta: duration should equal `delta / velocity * 1000`,
        // i.e. the velocity-matched value, neither clamped to the min
        // nor the max.
        let delta = 0.556;
        let expected = (delta / SUPPORT_SCROLL_VELOCITY * 1000.0).round() as u32;
        assert!(expected > SUPPORT_SCROLL_MIN_DURATION_MS);
        assert!(expected < SUPPORT_SCROLL_MAX_DURATION_MS);
        assert_eq!(scroll_support_list_duration_ms(delta), expected);
    }

    #[test]
    fn scroll_duration_clamped_at_minimum_for_tiny_deltas() {
        // A near-zero delta would compute a duration of just a few ms,
        // which the OS touch dispatcher may reject as too fast. The min
        // clamp keeps every swipe a deliberate gesture.
        assert_eq!(
            scroll_support_list_duration_ms(0.01),
            SUPPORT_SCROLL_MIN_DURATION_MS
        );
        assert_eq!(
            scroll_support_list_duration_ms(0.0),
            SUPPORT_SCROLL_MIN_DURATION_MS
        );
    }

    #[test]
    fn scroll_duration_clamped_at_maximum_for_pathological_deltas() {
        // A pathological delta (e.g. detector returning an anchor near
        // y=1.0 on a misaligned frame) must not stall the runner with a
        // multi-second swipe.
        assert_eq!(
            scroll_support_list_duration_ms(5.0),
            SUPPORT_SCROLL_MAX_DURATION_MS
        );
    }
}
