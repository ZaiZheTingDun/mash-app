use super::*;

pub(crate) const SERVANT_SKILLS: [[Point; 3]; 3] = [
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

pub(crate) const EQUIPMENT_BUTTON: Point = Point::new(0.933, 0.434);

/// Master / equipment skill buttons
pub(crate) const EQUIPMENT_SKILLS: [Point; 3] = [
    Point::new(0.708, 0.436),
    Point::new(0.777, 0.436),
    Point::new(0.848, 0.436),
];

/// Attack button position on the battle screen.
pub(crate) const ATTACK_BUTTON: Point = Point::new(0.887, 0.844);

/// Return button on the attack-card screen, used after advanced-mode card
/// inspection when the runner needs to go back to Battle and run skills.
pub(crate) const ATTACK_SCREEN_RETURN: Point = Point::new(0.938, 0.947);

/// Tap target that, when pressed during a skill / NP animation, makes the
/// game skip ahead to the next actionable frame. Same physical button
/// works after every skill on the battle screen.
pub(crate) const SKIP_ANIMATION_BUTTON: Point = Point::new(0.685, 0.095);

pub(crate) const BATTLE_SCREEN: &str = "Battle";
pub(crate) const SUPPORT_SELECT_SCREEN: &str = "SupportSelect";
pub(crate) const ATTACK_BUTTON_ELEMENT: &str = "attack_button";
pub(crate) const SKILL_TARGET_CLOSE_BUTTON_ELEMENT: &str = "skill_target_close_button";
pub(crate) const COMMAND_SPELL_CLOSE_BUTTON_ELEMENT: &str = "command_spell_close_button";
pub(crate) const ORDER_CHANGE_CLOSE_BUTTON_ELEMENT: &str = "order_change_close_button";
pub(crate) const SUPPORT_SCROLL_START_ELEMENT: &str = "support_scroll_start";
pub(crate) const SUPPORT_SCROLL_END_ELEMENT: &str = "support_scroll_end";
/// Party servant auto-placement is reserved for a later implementation.
/// Keep the config shape intact, but do not enter ServantSelect from
/// TeamConfirm yet.
pub(crate) const ENABLE_PARTY_SERVANT_AUTO_PLACEMENT: bool = false;

/// Region of the top-right `BATTLE m/n` HUD strip. The CV sidecar
/// anchors on the gold `BATTLE` label inside this region and reads
/// the `(m, n)` digit pair to its right; `m` drives which configured
/// `BattleScene` block runs this iteration.
pub(crate) const BATTLE_SCENE_REGION: NormRect = NormRect {
    x: 0.587,
    y: 0.000,
    w: 0.160,
    h: 0.062,
};

/// Ally target positions for skill targeting (servant_1, servant_2, servant_3)
pub(crate) const SKILL_TARGETS: [Point; 3] = [
    Point::new(0.254, 0.474),
    Point::new(0.499, 0.474),
    Point::new(0.744, 0.474),
];

/// Enemy target positions for attack targeting (enemy_1..enemy_6).
/// Coordinates are normalized from 2560x1440 screenshots.
pub(crate) const ENEMY_TARGETS: [Point; 6] = [
    Point::new(0.11015625, 0.04583333333333333),
    Point::new(0.26640625, 0.04583333333333333),
    Point::new(0.42265625, 0.04583333333333333),
    Point::new(0.033203125, 0.18263888888888888),
    Point::new(0.189453125, 0.18263888888888888),
    Point::new(0.345703125, 0.18263888888888888),
];

/// Command card positions on the attack screen (5 cards left to right)
pub(crate) const COMMAND_CARDS: [Point; 5] = [
    Point::new(0.097, 0.678),
    Point::new(0.303, 0.678),
    Point::new(0.504, 0.678),
    Point::new(0.705, 0.678),
    Point::new(0.907, 0.678),
];

pub(crate) const NOBLE_PHANTASMS: [Point; 3] = [
    Point::new(0.319, 0.242),
    Point::new(0.497, 0.242),
    Point::new(0.680, 0.242),
];

/// Command Spell (令咒) entry button on the battle screen — opens the
/// modal listing the available spells.
pub(crate) const COMMAND_SPELL_BUTTON: Point = Point::new(0.829, 0.113);

/// Spell-option tap targets inside the Command Spell modal
/// (`CommandSpell_open.png`). Indices align with `command_spell_index`:
/// 0 = "宝具解放" (np_release), 1 = "灵基修复" (restore).
pub(crate) const COMMAND_SPELL_OPTIONS: [Point; 2] =
    [Point::new(0.500, 0.460), Point::new(0.500, 0.690)];

/// "决定" confirm button on the Command Spell confirmation dialog
/// (`command_spell_confirmation.png`). Its mirror "取消" button at
/// roughly (0.340, 0.600) is intentionally not exposed — the runner
/// always confirms.
pub(crate) const COMMAND_SPELL_CONFIRM: Point = Point::new(0.660, 0.600);

/// In-battle Order Change servant slots, left to right on the change screen.
/// Front-line slots are indices 0-2, back-line slots are 3-5.
pub(crate) const ORDER_CHANGE_SLOTS: [Point; 6] = [
    Point::new(0.107, 0.486),
    Point::new(0.264, 0.486),
    Point::new(0.420, 0.486),
    Point::new(0.576, 0.486),
    Point::new(0.732, 0.486),
    Point::new(0.888, 0.486),
];

pub(crate) const ORDER_CHANGE_CONFIRM: Point = Point::new(0.500, 0.872);

/// Settle time between taps in the Command Spell dialog stack. Each tap
/// pops or pushes a full-screen modal (open dialog → confirmation →
/// target picker), so we wait noticeably longer than `ACTION_DELAY`
/// (which is sized for in-place taps on the battle screen).
pub(crate) const COMMAND_SPELL_DIALOG_SETTLE: Duration = Duration::from_millis(600);

// ---------------------------------------------------------------------------
// Battle-result tap targets. Each post-battle page has a single forward
// button; constants are kept here so the debug page (and future overlay)
// can introspect them without crawling the match arm.
// ---------------------------------------------------------------------------

/// "Next" arrow on the bond-points result page.
pub(crate) const BATTLE_RESULT_BOND_NEXT: Point = Point::new(0.041, 0.945);
/// "Next" arrow on the EXP-gain result page (same physical button as bond).
pub(crate) const BATTLE_RESULT_EXP_NEXT: Point = Point::new(0.041, 0.945);
/// "Next" button on the loot/drops summary page.
pub(crate) const BATTLE_RESULT_LOOT_NEXT: Point = Point::new(0.874, 0.890);
/// "Skip / Close" on the optional friend-request prompt that appears
/// after using a non-friend support.
pub(crate) const BATTLE_RESULT_FRIEND_SKIP: Point = Point::new(0.254, 0.854);
/// "Continue / Repeat" button on the final continue page — taps this when
/// `RunConfig::repeat_mission` is true.
pub(crate) const BATTLE_RESULT_CONTINUE_REPEAT: Point = Point::new(0.657, 0.809);
/// "Close / Stop" button on the final continue page — taps this when
/// `RunConfig::repeat_mission` is false. The runner finishes after.
pub(crate) const BATTLE_RESULT_CONTINUE_STOP: Point = Point::new(0.348, 0.809);
/// Generic skip/close target for transient battle-result popups that can
/// obscure the settlement page and make screen detection return Unknown.
/// Same physical position as the battle animation-skip button, but kept as
/// a separate semantic constant so result-popup behavior can be tuned alone.
pub(crate) const BATTLE_RESULT_POPUP_SKIP: Point = Point::new(0.685, 0.095);

pub(crate) const AP_RECOVERY_ITEMS_REGION: NormRect = NormRect {
    x: 0.244,
    y: 0.142,
    w: 0.095,
    h: 0.659,
};
pub(crate) const AP_RECOVERY_SCROLL_FROM: Point = Point::new(0.780, 0.166);
pub(crate) const AP_RECOVERY_SCROLL_TO: Point = Point::new(0.780, 0.426);
pub(crate) const AP_RECOVERY_CONFIRM_BUTTON: Point = Point::new(0.663, 0.795);
pub(crate) const AP_RECOVERY_LIST_LABEL_TEMPLATE: &str = "items/label_item";
pub(crate) const AP_RECOVERY_ITEM_THRESHOLD: f64 = 0.82;

/// Cadence used by `tap_until_screen_changes` when dismissing post-battle
/// result pages. Slow enough for the device to register each tap and for
/// `detect()` to read a fresh frame, fast enough that a 3–5 s bond /
/// EXP animation only absorbs a few wasted taps before the page actually
/// transitions.
pub(crate) const BATTLE_RESULT_TAP_INTERVAL: Duration = Duration::from_millis(300);
/// Hard ceiling for how long any single result page is allowed to absorb
/// taps before the runner emits a timeout warning. Comfortably above the
/// longest measured bond / EXP animation (~5 s for a multi-servant
/// level-up cascade).
pub(crate) const BATTLE_RESULT_TAP_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn is_battle_result_screen(screen: Screen) -> bool {
    matches!(
        screen,
        Screen::BattleResultBond
            | Screen::BattleResultExp
            | Screen::BattleResultLoot
            | Screen::BattleResultFriendRequest
            | Screen::BattleResultContinue
    )
}

// ---------------------------------------------------------------------------
// Debug: expose coordinate constants for visualization
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LabeledPoint {
    pub label: String,
    pub point: Point,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LabeledRegion {
    pub label: String,
    pub region: NormRect,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CoordGroup {
    pub id: String,
    pub label: String,
    pub points: Vec<LabeledPoint>,
    pub regions: Vec<LabeledRegion>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct DebugCoordinates {
    pub groups: Vec<CoordGroup>,
}

/// Snapshot of every `Point` / `NormRect` constant the runner uses, grouped
/// for display in the debug UI.
pub(crate) fn debug_coordinates() -> DebugCoordinates {
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
