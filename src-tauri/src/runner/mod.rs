//! Battle automation engine and runtime helpers.
//! Public runner DTOs live in submodules; this module keeps the high-level loop.

mod actions;
mod ap_recovery;
mod attack;
mod config;
mod coords;
mod engine;
mod grand;
mod party;
mod prebattle;
mod results;
mod runtime;
mod state;
mod support;

pub(crate) use actions::*;
#[cfg(test)]
pub(crate) use ap_recovery::*;
pub(crate) use attack::*;
use config::GrandServantRuntimeConfig;
pub use config::*;
pub(crate) use coords::*;
pub(crate) use grand::*;
pub(crate) use party::*;
pub(crate) use results::*;
pub(crate) use state::*;
pub(crate) use support::*;

use crate::adb::Adb;
use crate::screen::{
    BondLevelUpReadResult, CommandCardMatch, NoblePhantasmMatch, NormRect, Point, Screen,
    SidecarClient, SkillUseDialogProbe, SupportCeArtworkCheck, SupportCeIconCheck,
    SupportCeVerificationOptions, SupportCeVerificationResult, SupportRowMatch,
};
use crate::touch::{self, TouchBackend};
#[cfg(test)]
use crate::GrandCardRuleSlotConfig;
use crate::{
    default_grand_chain_priority, load_servant_metadata, servant_np_card, Action,
    AdvancedBattleScene, AdvancedCommandCardCondition, AdvancedOutputType, AdvancedRule,
    AttackCard, BattleScene, BattleTurn, GrandCardRuleConfig, GrandCardStrategy,
    GrandChainPriorityItem, GrandClass, GrandClassDefinition, GrandRoleDefinition,
    GrandServantConfig, LancerGrandRole, ServantMetadata, Server,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tauri::Emitter;

/// Poll cadence for `wait_for_attack_button` while a skill animation
/// (cut-in, NP charge effect, etc.) is hiding the attack button.
const SKILL_POLL_INTERVAL: Duration = Duration::from_millis(300);
/// Hard cap on how long we'll wait for the attack button to come back
/// after a skill. Some skills trigger long buff cut-ins or animations
/// (and a few skills push an NP-charge cut-in on top), so this needs
/// to cover NP-length animations without hanging forever if something
/// genuinely went wrong.
const SKILL_WAIT_TIMEOUT: Duration = Duration::from_secs(15);
/// Order Change has an extra servant-swap settle window after the Battle
/// HUD becomes visible again. Without this, the next skill tap can land
/// while the swap animation is still unwinding.
const ORDER_CHANGE_EXTRA_SETTLE: Duration = Duration::from_secs(1);
/// Maximum time to suppress duplicate Battle-screen attack taps after tapping
/// Attack and before the Attack screen classifier catches up.
const ATTACK_SCREEN_WAIT_TIMEOUT: Duration = Duration::from_secs(3);
/// Short window after a skill tap where the Battle action menu should
/// disappear if the game accepted the input. Keep this small so a missed
/// tap retries promptly instead of stalling the whole turn.
const SKILL_ACTIVATION_START_TIMEOUT: Duration = Duration::from_secs(2);
/// After a submitted attack resolves back to Battle, wait briefly for a
/// reliable `BATTLE m/n` HUD read before advancing the normal-mode turn
/// counter. If CV keeps failing, fall back to the legacy turn-advance logic
/// so automation does not stall forever.
const POST_ATTACK_HUD_READ_TIMEOUT: Duration = Duration::from_secs(3);
const COMMAND_CARD_COUNT: usize = 5;
/// Maximum per-axis jitter (in physical pixels) added to every tap so
/// repeated runs don't land on identical coordinates. Small enough to
/// stay well inside button hit-boxes; large enough that the noise is
/// distinguishable from a deterministic script.
const TAP_JITTER_PX: i32 = 6;

const DEFAULT_W: u32 = 1080;
const DEFAULT_H: u32 = 1920;
const DEFAULT_FRAME_W: u32 = 1920;
const DEFAULT_FRAME_H: u32 = 1080;

pub struct Runner {
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
    frame_w: u32,
    frame_h: u32,
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
    /// True after this run has saved CN's second-level EXTRA class choice.
    /// The game persists that choice, so later refreshes / repeated quests
    /// only need to tap the ordinary EXTRA tab again.
    support_extra_class_filter_configured: bool,
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
    five_star_ce_drop_count: u32,
    battle_result_loot_handled: bool,
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
        frame_size: Option<(u32, u32)>,
        assets_dir: Option<PathBuf>,
        ce_assets_dir: Option<PathBuf>,
        server: Server,
        sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
    ) -> Self {
        let (screen_w, screen_h) = screen_size.unwrap_or((DEFAULT_W, DEFAULT_H));
        let (frame_w, frame_h) = frame_size.unwrap_or((DEFAULT_FRAME_W, DEFAULT_FRAME_H));
        let touch = build_touch_backend(&adb);
        Self {
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
            frame_w,
            frame_h,
            assets_dir,
            ce_assets_dir,
            server,
            team_changed: false,
            support_selected: false,
            support_scroll_count: 0,
            support_refresh_count: 0,
            support_class_tab_done: false,
            support_extra_class_filter_configured: false,
            support_meta: None,
            support_ce_template: None,
            support_grand_ce_templates: None,
            support_grand_section_seen: false,
            support_grand_section_misses: 0,
            support_level_progress: SupportLevelPanelProgress::default(),
            servants_placed: Vec::new(),
            battle: BattleState::new(),
            completed_mission_runs: 0,
            five_star_ce_drop_count: 0,
            battle_result_loot_handled: false,
            battle_result_continue_handled: false,
        }
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

#[cfg(test)]
mod tests;
