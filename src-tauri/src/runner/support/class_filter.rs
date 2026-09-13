//! Support class-tab selection and CN EXTRA-class filtering.

use super::*;

pub(crate) const SUPPORT_EXTRA_FILTER_DIALOG_ELEMENT: &str = "dialog_extra_class_filter";
pub(crate) const SUPPORT_EXTRA_FILTER_LONG_PRESS_MS: u32 = 2000;
pub(crate) const SUPPORT_EXTRA_FILTER_CONFIRM_PRESS_MS: u32 = 100;
pub(crate) const SUPPORT_EXTRA_FILTER_DIALOG_TIMEOUT: Duration = Duration::from_secs(3);
pub(crate) const SUPPORT_EXTRA_FILTER_DIALOG_POLL: Duration = Duration::from_millis(200);
pub(crate) const SUPPORT_EXTRA_FILTER_ACTION_SETTLE: Duration = Duration::from_millis(300);
pub(crate) const SUPPORT_EXTRA_FILTER_RESULT_SETTLE: Duration = Duration::from_secs(2);
pub(crate) const SUPPORT_EXTRA_FILTER_RESET_BUTTON: Point = Point::new(0.292, 0.778);
pub(crate) const SUPPORT_EXTRA_FILTER_CONFIRM_BUTTON: Point = Point::new(0.742, 0.778);

/// Class-filter tab bar across the top of the support-select screen. All
/// tabs share the same y. Order mirrors the FGO UI: all → saber → ...
/// → berserker → extra → mix. Lookup happens via `class_tab_for` which
/// maps Atlas Academy `className` strings into one of these tabs.
pub(crate) const SUPPORT_CLASS_TAB_Y: f64 = 0.178;
pub(crate) const SUPPORT_TAB_SABER: Point = Point::new(0.1246, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_ARCHER: Point = Point::new(0.1773, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_LANCER: Point = Point::new(0.2301, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_RIDER: Point = Point::new(0.2828, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_CASTER: Point = Point::new(0.3355, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_ASSASSIN: Point = Point::new(0.3883, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_BERSERKER: Point = Point::new(0.4410, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_EXTRA: Point = Point::new(0.4938, SUPPORT_CLASS_TAB_Y);

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExtraClassFilter {
    pub(crate) label: &'static str,
    pub(crate) point: Point,
}

impl ExtraClassFilter {
    const fn new(label: &'static str, x: f64, y: f64) -> Self {
        Self {
            label,
            point: Point::new(x, y),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SupportClassFilterAction {
    Tap(Point),
    CnExtra(ExtraClassFilter),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SupportExtraFilterDialogWait {
    Matched,
    TimedOut,
    Aborted,
}

pub(crate) fn should_retry_support_extra_filter_dialog(wait: SupportExtraFilterDialogWait) -> bool {
    wait == SupportExtraFilterDialogWait::TimedOut
}

/// Settle time after tapping a class tab. The list animates a quick fade
/// when filtering; ~600ms is enough for the new rows to render before we
/// kick off the OCR pass.
pub(crate) const SUPPORT_CLASS_TAB_SETTLE: Duration = Duration::from_millis(600);

/// Map an Atlas Academy `className` (already lowercased by
/// `load_servant_metadata`) to the support-select class-filter tab.
/// Returns `None` for unknown / boss-only class variants so the
/// caller can skip the tap and log it instead of guessing wrong.
pub(crate) fn class_tab_for(class_name: &str) -> Option<Point> {
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
        // pretender, plus the three playable Beast classes. Atlas mixes camelCase and lowercase forms so
        // `load_servant_metadata` lowercases before we land here.
        "shielder" | "ruler" | "avenger" | "mooncancer" | "alterego" | "foreigner"
        | "pretender" | "beast" | "beasteresh" | "unbeastolgamarie" => Some(SUPPORT_TAB_EXTRA),
        _ => None,
    }
}

/// CN exposes a second-level EXTRA class dialog on long press. Other
/// servers retain the original single-tap EXTRA behaviour.
pub(crate) fn support_class_filter_action(
    server: Server,
    class_name: &str,
    cn_extra_filter_enabled: bool,
    cn_extra_configured: bool,
) -> Option<SupportClassFilterAction> {
    if server == Server::Cn {
        let extra = match class_name {
            "shielder" => Some(ExtraClassFilter::new("盾兵", 0.260, 0.427)),
            "ruler" => Some(ExtraClassFilter::new("裁定者", 0.420, 0.427)),
            "avenger" => Some(ExtraClassFilter::new("复仇者", 0.580, 0.427)),
            "mooncancer" => Some(ExtraClassFilter::new("月之癌", 0.740, 0.427)),
            "alterego" => Some(ExtraClassFilter::new("他人格", 0.260, 0.649)),
            "foreigner" => Some(ExtraClassFilter::new("降临者", 0.420, 0.649)),
            "pretender" => Some(ExtraClassFilter::new("身披角色者", 0.580, 0.649)),
            "beast" | "beasteresh" | "unbeastolgamarie" => {
                Some(ExtraClassFilter::new("兽", 0.740, 0.649))
            }
            _ => None,
        };
        if let Some(extra) = extra {
            return Some(if !cn_extra_filter_enabled || cn_extra_configured {
                SupportClassFilterAction::Tap(SUPPORT_TAB_EXTRA)
            } else {
                SupportClassFilterAction::CnExtra(extra)
            });
        }
    }
    class_tab_for(class_name).map(SupportClassFilterAction::Tap)
}
