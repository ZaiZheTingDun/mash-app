use crate::adb::Adb;
use crate::screen::{
    CommandCardMatch, NoblePhantasmMatch, NormRect, Point, Screen, SidecarClient, SupportRowMatch,
};
use crate::{load_servant_metadata, Action, AttackCard, BattleScene, ServantMetadata, Server};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
    /// Craft-essence id pinned via the team-builder support CE slot.
    /// When `Some`, `handle_support_select` runs `verify_support_ce`
    /// against each OCR-detected row and picks the first row whose CE
    /// matches the template at `assets/ces/{id}/card_ce.png`. `None`
    /// (or a missing template) preserves legacy behaviour: pick the
    /// first OCR match.
    #[serde(default)]
    pub support_craft_essence_id: Option<u32>,
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

fn default_support_append_skill_level_mins() -> [Option<u32>; 5] {
    [None; 5]
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

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationEvent {
    pub state: String,
    pub current_screen: String,
    pub message: String,
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

/// Tap target that, when pressed during a skill / NP animation, makes the
/// game skip ahead to the next actionable frame. Same physical button
/// works after every skill on the battle screen.
const SKIP_ANIMATION_BUTTON: Point = Point::new(0.685, 0.095);

const BATTLE_SCREEN: &str = "Battle";
const SUPPORT_SELECT_SCREEN: &str = "SupportSelect";
pub const ATTACK_BUTTON_ELEMENT: &str = "attack_button";
const SUPPORT_SCROLL_END_ELEMENT: &str = "support_scroll_end";

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

/// Enemy target positions for attack targeting (enemy_1, enemy_2, enemy_3)
const ENEMY_TARGETS: [Point; 3] = [
    Point::new(0.035, 0.060),
    Point::new(0.230, 0.060),
    Point::new(0.425, 0.060),
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
}

impl BattleState {
    fn new() -> Self {
        Self {
            current_scene_index: 0,
            last_screen_scene: None,
            executed_scene_index: None,
            scene_config_used: false,
            waiting_for_battle: false,
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

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const ACTION_DELAY: Duration = Duration::from_millis(300);

/// Hard cap on friend-list refreshes inside `handle_support_select`. After
/// this many refreshes (each preceded by a full scroll cycle) without
/// finding the pinned servant, the runner aborts with an error so the user
/// isn't stuck looping forever on a servant that simply isn't available.
const SUPPORT_MAX_REFRESHES: u32 = 3;
/// Settle time after the support-list scroll swipe completes; long enough
/// for momentum scrolling to come to rest before the next OCR pass.
const SUPPORT_SCROLL_SETTLE: Duration = Duration::from_millis(900);
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

/// Refresh-friend-list button on the support-select screen, captured from
/// a 2560x1440 landscape device. Calibrated alongside the class-tab strip
/// (same row, x further right).
const SUPPORT_REFRESH_BUTTON: Point = Point::new(0.726, 0.178);
/// Confirm button inside the support refresh dialog.
const SUPPORT_REFRESH_CONFIRM_BUTTON: Point = Point::new(0.650, 0.779);
const SUPPORT_REFRESH_DIALOG_ELEMENT: &str = "dialog_refresh_support";

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
/// Minimum `TM_CCOEFF_NORMED` score to accept a row's CE icon as the
/// pinned CE. Conservative on purpose — the CE icons share a lot of dark
/// background pixels so even mismatched CEs score ~0.4-0.5; the matched
/// CE typically scores 0.75+.
pub const SUPPORT_CE_THRESHOLD: f64 = 0.70;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SupportLevelFilter {
    Pass,
    Fail,
    WaitingForPanel,
}

#[derive(Debug, Default, Clone)]
struct SupportLevelPanelProgress {
    candidate_key: Option<String>,
    owned_met: bool,
    append_met: bool,
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

fn support_level_meets(actual: Option<u32>, required_min: Option<u32>) -> bool {
    match required_min {
        None => true,
        Some(required) => actual.is_some_and(|level| level >= required),
    }
}

fn support_levels_meet(actual: &[Option<u32>], required: &[Option<u32>]) -> bool {
    required.iter().enumerate().all(|(index, required_min)| {
        support_level_meets(actual.get(index).copied().flatten(), *required_min)
    })
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
    if !support_level_meets(row.np_level, config.support_noble_phantasm_level_min) {
        return SupportLevelFilter::Fail;
    }
    if !needs_owned && !needs_append {
        return SupportLevelFilter::Pass;
    }

    let key = support_level_candidate_key(row);
    if progress.candidate_key.as_deref() != Some(key.as_str()) {
        progress.candidate_key = Some(key);
        progress.owned_met = false;
        progress.append_met = false;
    }

    match row.skill_panel.as_deref() {
        Some("owned") if needs_owned => {
            if !support_levels_meet(&row.skill_levels, &config.support_skill_level_mins) {
                progress.owned_met = false;
                return SupportLevelFilter::Fail;
            }
            progress.owned_met = true;
        }
        Some("append") if needs_append => {
            if !support_levels_meet(
                &row.append_skill_levels,
                &config.support_append_skill_level_mins,
            ) {
                progress.append_met = false;
                return SupportLevelFilter::Fail;
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
    /// Tracks visible skill-panel validation for the current support row.
    /// Owned and append skills are shown on alternating panels, so a row can
    /// only satisfy both groups across multiple OCR polls.
    support_level_progress: SupportLevelPanelProgress,
    servants_placed: Vec<u32>,
    // Battle progress tracking
    battle: BattleState,
    completed_mission_runs: u32,
    battle_result_continue_handled: bool,
}

impl Runner {
    pub fn new(
        adb: Adb,
        sidecar: SidecarClient,
        config: RunConfig,
        scenes: Vec<BattleScene>,
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
        Self {
            adb,
            sidecar: Some(sidecar),
            sidecar_cache,
            config,
            scenes,
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

    fn tap_at(&self, screen: &str, point: Point) -> bool {
        let (px, py) = point.to_physical(self.screen_w, self.screen_h);
        let (jx, jy) = jitter_offset();
        // Saturate at the screen edges so a near-edge button still
        // registers even if the jitter would push it off-screen.
        let tap_x = (px as i32 + jx).clamp(0, self.screen_w.saturating_sub(1) as i32) as u32;
        let tap_y = (py as i32 + jy).clamp(0, self.screen_h.saturating_sub(1) as i32) as u32;
        match self.adb.tap(tap_x, tap_y) {
            Ok(()) => true,
            Err(err) => {
                self.fail_action(screen, "点击", err);
                false
            }
        }
    }

    fn swipe_at(&self, screen: &str, from: Point, to: Point, duration_ms: u32) -> bool {
        let from_px = from.to_physical(self.screen_w, self.screen_h);
        let to_px = to.to_physical(self.screen_w, self.screen_h);
        match self.adb.swipe(from_px, to_px, duration_ms) {
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
                    self.handle_attack();
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
        let ce_template = self.resolve_support_ce_template();
        let mut waiting_for_skill_panel = false;
        let mut level_filter_missed = false;
        let mut chosen_index: Option<usize> = None;
        for (index, row) in result.supports.iter().enumerate() {
            if let Some(template) = ce_template.as_deref() {
                if !self.support_row_matches_ce(row, template) {
                    continue;
                }
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
                SupportLevelFilter::Fail => {
                    level_filter_missed = true;
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
            self.support_class_tab_done = false;
            self.support_level_progress = SupportLevelPanelProgress::default();
            thread::sleep(ACTION_DELAY);
            return;
        }

        // OCR found rows but the pinned CE didn't match any of them —
        // emit a distinct message before falling through to the
        // scroll/refresh branch so the user knows it's a CE filter miss
        // (vs a name miss).
        if ce_template.is_some() && !result.supports.is_empty() {
            self.emit("SupportSelect", "找到从者但礼装不匹配，继续滚动…");
        }
        if waiting_for_skill_panel {
            self.emit("SupportSelect", "找到从者，等待技能显示自动切换…");
            thread::sleep(SUPPORT_SCROLL_SETTLE);
            return;
        }
        if level_filter_missed {
            self.emit("SupportSelect", "找到从者但技能/宝具等级不匹配，继续滚动…");
        }

        // No match in the visible viewport. The scroll-bar tail indicator
        // is the source of truth for "have we reached the bottom of the
        // friend list" — keep swiping until it shows up, then refresh.
        if !self.support_scroll_bar_at_end() {
            self.emit(
                "SupportSelect",
                &format!(
                    "未找到 {}，滚动列表 (第 {} 次)",
                    meta.name,
                    self.support_scroll_count + 1,
                ),
            );
            if !self.swipe_at(
                "SupportSelect",
                Point::new(0.50, 0.70),
                Point::new(0.50, 0.30),
                300,
            ) {
                return;
            }
            self.support_scroll_count += 1;
            self.support_level_progress = SupportLevelPanelProgress::default();
            thread::sleep(SUPPORT_SCROLL_SETTLE);
        } else if self.support_refresh_count < SUPPORT_MAX_REFRESHES {
            self.emit(
                "SupportSelect",
                &format!(
                    "已到底部，刷新助战列表 ({}/{})",
                    self.support_refresh_count + 1,
                    SUPPORT_MAX_REFRESHES,
                ),
            );
            if !self.tap_at("SupportSelect", SUPPORT_REFRESH_BUTTON) {
                return;
            }
            self.support_scroll_count = 0;
            self.support_refresh_count += 1;
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
        let resolved = self.config.support_craft_essence_id.and_then(|ce_id| {
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
        });
        self.support_ce_template = Some(resolved.clone());
        resolved
    }

    /// Compute the absolute search window for a row's CE icon by
    /// applying `SUPPORT_CE_OFFSET_IN_ROW` (a row-local rect) to the
    /// row's full bbox. Thin method wrapper around the pure free helper
    /// [`ce_search_region`] (kept free so unit tests can exercise the
    /// math without constructing a full `SupportRowMatch`).
    fn support_ce_search_region(row: &SupportRowMatch) -> NormRect {
        ce_search_region(row.row_region)
    }

    /// Return true when a row's CE icon scores at or above
    /// `SUPPORT_CE_THRESHOLD` against `template_path`.
    fn support_row_matches_ce(&mut self, row: &SupportRowMatch, template_path: &Path) -> bool {
        let region = Self::support_ce_search_region(row);
        match self
            .sidecar()
            .verify_support_ce(None, region, template_path, SUPPORT_CE_THRESHOLD)
        {
            Ok((score, passed)) => {
                eprintln!(
                    "[runner] support CE verify: score={:.3} threshold={:.2} -> {}",
                    score,
                    SUPPORT_CE_THRESHOLD,
                    if passed { "PASS" } else { "skip" },
                );
                passed
            }
            Err(e) => {
                eprintln!("[runner] support CE verify failed (treating as skip): {e}");
                false
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

    /// After tapping the support-list refresh button, confirm the modal when
    /// the server's cv.json defines one. Servers without a modal template
    /// fall back to the legacy fixed settle delay.
    fn confirm_support_refresh_dialog_if_needed(&mut self) -> bool {
        let appear_deadline = std::time::Instant::now() + SUPPORT_REFRESH_DIALOG_APPEAR_TIMEOUT;
        loop {
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

        self.emit("SupportSelect", "助战刷新需要确认，点击确定");
        if !self.tap_at("SupportSelect", SUPPORT_REFRESH_CONFIRM_BUTTON) {
            return false;
        }

        loop {
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
            if let Some(scene_cfg) = self.scenes.get(self.battle.current_scene_index).cloned() {
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
        let mut ids: [Option<u32>; 3] = [None, None, None];
        for sel in &self.config.servant_selections {
            let slot = sel.slot_index as usize;
            if slot < 3 {
                ids[slot] = Some(sel.servant_id);
            }
        }

        if let Some(support_id) = self
            .config
            .support_servant_name
            .as_deref()
            .and_then(parse_servant_name_id)
        {
            for slot in ids.iter_mut() {
                if slot.is_none() {
                    *slot = Some(support_id);
                    break;
                }
            }
        }
        ids
    }

    fn handle_attack(&mut self) {
        let party_ids = self.build_party_ids();

        // Unique candidate set, stable by party position.
        let mut candidate_ids: Vec<u32> = Vec::with_capacity(3);
        for id in party_ids.iter().flatten() {
            if !candidate_ids.contains(id) {
                candidate_ids.push(*id);
            }
        }

        if self.assets_dir.is_none() {
            self.emit("Attack", "未找到从者资源目录，将无法按从者匹配指令卡");
        }

        let assets_dir = self.assets_dir.clone();
        let cards = match self.sidecar().find_command_cards(
            None,
            None,
            &candidate_ids,
            assets_dir.as_deref(),
        ) {
            Ok(c) => c,
            Err(err) => {
                self.fail_action("Attack", "识别指令卡", err);
                return;
            }
        };

        let nps = match self.sidecar().find_noble_phantasms(None, None) {
            Ok(n) => n,
            Err(err) => {
                self.fail_action("Attack", "识别宝具卡", err);
                return;
            }
        };

        let card_summary: Vec<String> = cards
            .iter()
            .map(|c| {
                format!(
                    "C{}={}{}",
                    c.slot + 1,
                    c.suit.as_deref().unwrap_or("?"),
                    c.servant_id.map(|id| format!("/{id}")).unwrap_or_default(),
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

        let mut used_card_slots: HashSet<u32> = HashSet::new();
        let mut used_np_slots: HashSet<u32> = HashSet::new();
        let mut picks: Vec<Pick> = Vec::with_capacity(3);

        if self.battle.scene_config_used {
            if let Some(scene_cfg) = self.scenes.get(self.battle.current_scene_index) {
                picks = pick_by_priority(
                    &scene_cfg.attack_priority,
                    &cards,
                    &nps,
                    &party_ids,
                    &mut used_card_slots,
                    &mut used_np_slots,
                );
            }
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
            self.emit("Attack", &msg);
            if !self.tap_at("Attack", point) {
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

        // Reset for next cycle
        self.battle.scene_config_used = false;
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
    fn skip_after_skill(&self) {
        let _ = self.tap_at("Battle", SKIP_ANIMATION_BUTTON);
        thread::sleep(ACTION_DELAY);
    }

    // -- utilities -----------------------------------------------------------

    fn next_unfilled_slot(&self) -> Option<ServantSlotConfig> {
        self.config
            .servant_selections
            .iter()
            .find(|s| !self.servants_placed.contains(&s.slot_index))
            .cloned()
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
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

/// Enemy targets (enemy_1, enemy_2, enemy_3). Available for future use.
#[allow(dead_code)]
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

/// Walk `priority` in order, picking matching cards / NPs until 3 are chosen
/// or the list is exhausted. Cards / NPs already picked are tracked in
/// `used_card_slots` / `used_np_slots` and will not be re-selected.
fn pick_by_priority(
    priority: &[AttackCard],
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    used_card_slots: &mut HashSet<u32>,
    used_np_slots: &mut HashSet<u32>,
) -> Vec<Pick> {
    let mut picks: Vec<Pick> = Vec::with_capacity(3);

    for entry in priority {
        if picks.len() >= 3 {
            break;
        }
        let Some(card_str) = entry.card.as_deref() else {
            continue;
        };

        // Expect ``servant_{i}_{suit|np}``; bail on anything else.
        let rest = match card_str.strip_prefix("servant_") {
            Some(r) => r,
            None => continue,
        };
        let (idx_str, kind) = match rest.split_once('_') {
            Some(p) => p,
            None => continue,
        };
        let Ok(field_pos) = idx_str.parse::<usize>() else {
            continue;
        };
        if !(1..=3).contains(&field_pos) {
            continue;
        }

        if kind == "np" {
            let np_slot = (field_pos - 1) as u32;
            if used_np_slots.contains(&np_slot) {
                continue;
            }
            if let Some(np) = nps.iter().find(|n| n.slot == np_slot) {
                if np.ready {
                    used_np_slots.insert(np_slot);
                    picks.push(Pick::Np {
                        slot: np_slot,
                        point: rect_center(&np.card_region),
                        from_priority: card_str.to_string(),
                    });
                }
            }
            continue;
        }

        let Some(wanted_suit) = suit_code(kind) else {
            continue;
        };
        let Some(wanted_id) = party_ids[field_pos - 1] else {
            // No known servant at that field position — can't match by face.
            continue;
        };

        // Leftmost-by-slot scan among unused cards owned by this servant
        // with the desired suit.
        let mut best: Option<&CommandCardMatch> = None;
        for c in cards {
            if used_card_slots.contains(&c.slot) {
                continue;
            }
            if c.servant_id != Some(wanted_id) {
                continue;
            }
            if c.suit.as_deref() != Some(wanted_suit) {
                continue;
            }
            if best.map_or(true, |b| c.slot < b.slot) {
                best = Some(c);
            }
        }
        if let Some(c) = best {
            used_card_slots.insert(c.slot);
            picks.push(Pick::Card {
                slot: c.slot,
                point: Point::new(c.x, c.y),
                servant_id: c.servant_id,
                suit: c.suit.clone(),
                from_priority: Some(card_str.to_string()),
            });
        }
    }

    picks
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
        // Other defaults travel through the same path; sanity-check
        // them so legacy `projects.json` rows keep deserializing.
        assert!(cfg.support_servant_id.is_none());
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
        let cfg: RunConfig = serde_json::from_value(payload).unwrap();
        assert_eq!(cfg.support_craft_essence_id, Some(1485));

        // Re-serialize and confirm the field round-trips under the
        // camelCase rename rule applied to the whole struct.
        let json = serde_json::to_value(&cfg).unwrap();
        assert_eq!(json["supportCraftEssenceId"], serde_json::json!(1485));
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
            np_matched_name: "为你纺织的时光之轮".into(),
            np_level: Some(5),
            skill_panel: panel.map(str::to_string),
            skill_levels: skills,
            append_skill_levels: append,
            skill_level_diagnostics: Vec::new(),
        }
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
            SupportLevelFilter::Fail
        );
        assert!(!progress.owned_met);
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
}
