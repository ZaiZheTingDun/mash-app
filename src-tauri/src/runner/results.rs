//! Battle-result screen constants and handlers.
//!
//! This module owns the post-battle settlement flow: dismissing bond/EXP/loot
//! pages, skipping friend requests, and deciding whether to repeat a quest.

use super::*;
use crate::commands::runtime::resolve_resource_image_path;
use crate::paths::app_data_dir;

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
const BATTLE_RESULT_BOND_LEVEL_UP_LABEL: &str = "BattleResultBondLevelUp";
const FIVE_STAR_CE_TEMPLATE_FILE_NAME: &str = "stars_5.png";
const FIVE_STAR_CE_THRESHOLD: f64 = 0.55;
const FIVE_STAR_CE_REFERENCE_W: f64 = 1920.0;
const FIVE_STAR_CE_REFERENCE_H: f64 = 1080.0;
const FIVE_STAR_CE_GRID_COLS: usize = 7;
const FIVE_STAR_CE_GRID_ROWS_TO_CHECK: usize = 2;
const FIVE_STAR_CE_CELL_W: f64 = 177.0;
const FIVE_STAR_CE_CELL_H: f64 = 194.0;
const FIVE_STAR_CE_GRID_X0: f64 = 232.0;
const FIVE_STAR_CE_GRID_Y0: f64 = 131.0;
const FIVE_STAR_CE_GRID_X_GAPS: [f64; 6] = [29.0, 29.0, 30.0, 29.0, 29.0, 29.0];
const FIVE_STAR_CE_GRID_Y_GAP: f64 = 19.0;
const FIVE_STAR_CE_SEARCH_IN_CELL: NormRect = NormRect {
    x: 0.435,
    y: 0.737,
    w: 0.548,
    h: 0.160,
};
const FIVE_STAR_CE_TEMPLATE_REFERENCE_SIZE: (u32, u32) = (88, 20);
const FULL_TEMPLATE_CROP: NormRect = NormRect {
    x: 0.0,
    y: 0.0,
    w: 1.0,
    h: 1.0,
};
const LOOT_SCREENSHOT_WAIT_SECONDS: f64 = 0.2;
const UNKNOWN_SCREEN_TIMEOUT_SCREENSHOT_WAIT_SECONDS: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FiveStarCeDropStopAction {
    Continue,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BondResultStopAction {
    Continue,
    StopOnLevelUp,
    StopOnMaxLevel,
}

pub(crate) fn bond_result_stop_action(
    is_level_up_overlay: bool,
    read: Option<&BondLevelUpReadResult>,
    stop_on_bond_level_up: bool,
    stop_on_bond_max_level: bool,
) -> BondResultStopAction {
    if !is_level_up_overlay {
        return BondResultStopAction::Continue;
    }
    if stop_on_bond_level_up {
        return BondResultStopAction::StopOnLevelUp;
    }
    if stop_on_bond_max_level
        && read
            .and_then(|result| result.bond_level_after)
            .is_some_and(|level| level >= 10)
    {
        return BondResultStopAction::StopOnMaxLevel;
    }
    BondResultStopAction::Continue
}

pub(crate) fn five_star_ce_drop_target_count(value: u32) -> u32 {
    value.max(1)
}

pub(crate) fn five_star_ce_drop_stop_action(
    current_total: u32,
    new_drops: u32,
    target_count: u32,
) -> (u32, FiveStarCeDropStopAction) {
    let next_total = current_total.saturating_add(new_drops);
    let action = if next_total >= five_star_ce_drop_target_count(target_count) {
        FiveStarCeDropStopAction::Stop
    } else {
        FiveStarCeDropStopAction::Continue
    };
    (next_total, action)
}

pub(crate) fn five_star_ce_drop_regions() -> Vec<NormRect> {
    let mut regions = Vec::with_capacity(FIVE_STAR_CE_GRID_COLS * FIVE_STAR_CE_GRID_ROWS_TO_CHECK);
    let mut col_x = [0.0; FIVE_STAR_CE_GRID_COLS];
    col_x[0] = FIVE_STAR_CE_GRID_X0;
    for col in 1..FIVE_STAR_CE_GRID_COLS {
        col_x[col] = col_x[col - 1] + FIVE_STAR_CE_CELL_W + FIVE_STAR_CE_GRID_X_GAPS[col - 1];
    }

    for row in 0..FIVE_STAR_CE_GRID_ROWS_TO_CHECK {
        let cell_y =
            FIVE_STAR_CE_GRID_Y0 + row as f64 * (FIVE_STAR_CE_CELL_H + FIVE_STAR_CE_GRID_Y_GAP);
        for cell_x in col_x {
            let x = cell_x + FIVE_STAR_CE_CELL_W * FIVE_STAR_CE_SEARCH_IN_CELL.x;
            let y = cell_y + FIVE_STAR_CE_CELL_H * FIVE_STAR_CE_SEARCH_IN_CELL.y;
            let w = FIVE_STAR_CE_CELL_W * FIVE_STAR_CE_SEARCH_IN_CELL.w;
            let h = FIVE_STAR_CE_CELL_H * FIVE_STAR_CE_SEARCH_IN_CELL.h;
            regions.push(NormRect {
                x: x / FIVE_STAR_CE_REFERENCE_W,
                y: y / FIVE_STAR_CE_REFERENCE_H,
                w: w / FIVE_STAR_CE_REFERENCE_W,
                h: h / FIVE_STAR_CE_REFERENCE_H,
            });
        }
    }
    regions
}

pub(crate) fn five_star_ce_template_size(screen_w: u32, screen_h: u32) -> (u32, u32) {
    let width_scale = screen_w as f64 / FIVE_STAR_CE_REFERENCE_W;
    let height_scale = screen_h as f64 / FIVE_STAR_CE_REFERENCE_H;
    (
        ((FIVE_STAR_CE_TEMPLATE_REFERENCE_SIZE.0 as f64 * width_scale).round() as u32).max(1),
        ((FIVE_STAR_CE_TEMPLATE_REFERENCE_SIZE.1 as f64 * height_scale).round() as u32).max(1),
    )
}

pub(crate) fn battle_result_loot_screenshot_dir(app: &tauri::AppHandle) -> PathBuf {
    battle_result_loot_screenshot_dir_in_root(&app_data_dir(app))
}

pub(crate) fn battle_result_loot_screenshot_dir_in_root(root: &Path) -> PathBuf {
    root.join("debug").join("loot-screenshots")
}

pub(crate) fn battle_result_loot_screenshot_filename(
    timestamp: std::time::SystemTime,
    completed_mission_runs: u32,
) -> String {
    let millis = timestamp
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("loot-{millis:013}-run{:04}.jpg", completed_mission_runs + 1,)
}

pub(crate) fn unknown_screen_timeout_screenshot_dir(app: &tauri::AppHandle) -> PathBuf {
    unknown_screen_timeout_screenshot_dir_in_root(&app_data_dir(app))
}

pub(crate) fn unknown_screen_timeout_screenshot_dir_in_root(root: &Path) -> PathBuf {
    root.join("debug").join("unknown-screen-timeouts")
}

pub(crate) fn unknown_screen_timeout_screenshot_filename(
    timestamp: std::time::SystemTime,
    completed_mission_runs: u32,
) -> String {
    let millis = timestamp
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!(
        "unknown-{millis:013}-run{:04}.jpg",
        completed_mission_runs + 1,
    )
}

pub(crate) fn bond_level_up_screenshot_dir(app: &tauri::AppHandle) -> PathBuf {
    bond_level_up_screenshot_dir_in_root(&app_data_dir(app))
}

pub(crate) fn bond_level_up_screenshot_dir_in_root(root: &Path) -> PathBuf {
    root.join("screenshots").join("bond-level-up")
}

pub(crate) fn bond_level_up_screenshot_filename(
    timestamp: std::time::SystemTime,
    completed_mission_runs: u32,
) -> String {
    let millis = timestamp
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!(
        "bond-level-up-{millis:013}-run{:04}.jpg",
        completed_mission_runs + 1
    )
}

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

impl Runner {
    pub(crate) fn handle_battle_result_bond(&mut self) {
        let detected_label = match self.sidecar().detect_label_full(None) {
            Ok((label, _score)) => label,
            Err(err) => {
                self.emit_warn(
                    "BattleResultBond",
                    &format!("牵绊结算页面识别失败，继续结算流程: {err}"),
                );
                Screen::BattleResultBond.to_string()
            }
        };
        let is_level_up_overlay = detected_label == BATTLE_RESULT_BOND_LEVEL_UP_LABEL;

        if is_level_up_overlay {
            if self.config.auto_capture_bond_level_up {
                match self.capture_bond_level_up_screenshot() {
                    Ok(path) => self.emit(
                        "BattleResultBond",
                        &format!("牵绊升级截图已保存: {}", path.display()),
                    ),
                    Err(err) => self.emit_warn(
                        "BattleResultBond",
                        &format!("牵绊升级截图保存失败，继续结算流程: {err}"),
                    ),
                }
            }

            // Always read and log the level. Previously this was only called
            // for the max-level stop setting, leaving ordinary level-up
            // overlays unobserved and unlogged.
            let read = match self.sidecar().read_bond_level_up(None, false) {
                Ok(result) if result.ok => Some(result),
                Ok(result) => {
                    let reason = result.reason.unwrap_or_else(|| "未知原因".into());
                    self.emit_warn(
                        "BattleResultBond",
                        &format!("牵绊等级读取失败，继续结算流程: {reason}"),
                    );
                    None
                }
                Err(err) => {
                    self.emit_warn(
                        "BattleResultBond",
                        &format!("牵绊等级读取失败，继续结算流程: {err}"),
                    );
                    None
                }
            };
            match bond_result_stop_action(
                is_level_up_overlay,
                read.as_ref(),
                self.config.stop_on_bond_level_up,
                self.config.stop_on_bond_max_level,
            ) {
                BondResultStopAction::StopOnLevelUp => {
                    let level = read
                        .as_ref()
                        .and_then(|result| result.bond_level_after)
                        .map(|level| format!("至 {level}"))
                        .unwrap_or_default();
                    self.emit(
                        "BattleResultBond",
                        &format!("检测到牵绊等级提升{level}，自动停止"),
                    );
                    self.transition_lifecycle(RunnerLifecycleEvent::Finished);
                    return;
                }
                BondResultStopAction::StopOnMaxLevel => {
                    let level = read
                        .as_ref()
                        .and_then(|result| result.bond_level_after)
                        .unwrap_or(10);
                    self.emit(
                        "BattleResultBond",
                        &format!("检测到牵绊等级达到 {level}，自动停止"),
                    );
                    self.transition_lifecycle(RunnerLifecycleEvent::Finished);
                    return;
                }
                BondResultStopAction::Continue => {}
            }

            if let Some(level) = read.and_then(|result| result.bond_level_after) {
                self.emit(
                    "BattleResultBond",
                    &format!("牵绊等级提升至 {level}，前往下一画面"),
                );
            } else {
                self.emit("BattleResultBond", "牵绊等级提升，前往下一画面");
            }
        } else {
            if !self.battle_result_bond_handled {
                self.battle_result_bond_handled = true;
                self.emit("BattleResultBond", "羁绊点数结算，前往下一画面");
            }
        }

        self.tap_until_screen_label_changes(
            "BattleResultBond",
            &detected_label,
            BATTLE_RESULT_BOND_NEXT,
            BATTLE_RESULT_TAP_INTERVAL,
            BATTLE_RESULT_TAP_TIMEOUT,
        );
    }

    pub(crate) fn handle_battle_result_exp(&mut self) {
        self.emit("BattleResultExp", "经验结算，前往下一画面");
        self.tap_until_screen_changes(
            "BattleResultExp",
            Screen::BattleResultExp,
            BATTLE_RESULT_EXP_NEXT,
            BATTLE_RESULT_TAP_INTERVAL,
            BATTLE_RESULT_TAP_TIMEOUT,
        );
    }

    pub(crate) fn handle_battle_result_loot(&mut self) {
        if self.battle_result_loot_handled {
            self.emit("BattleResultLoot", "等待掉落结算页切换…");
            thread::sleep(ACTION_DELAY);
            return;
        }
        self.battle_result_loot_handled = true;

        if self.config.auto_capture_battle_result_loot {
            match self.capture_battle_result_loot_screenshot() {
                Ok(path) => self.emit(
                    "BattleResultLoot",
                    &format!("战利品截图已保存: {}", path.display()),
                ),
                Err(err) => self.emit_warn(
                    "BattleResultLoot",
                    &format!("战利品截图保存失败，继续结算流程: {err}"),
                ),
            }
        }

        if self.config.stop_on_five_star_ce_drop {
            let target = five_star_ce_drop_target_count(self.config.five_star_ce_drop_target_count);
            match self.count_visible_five_star_ce_drops() {
                Ok(new_drops) => {
                    let (next_total, action) = five_star_ce_drop_stop_action(
                        self.five_star_ce_drop_count,
                        new_drops,
                        target,
                    );
                    self.five_star_ce_drop_count = next_total;
                    self.emit(
                        "BattleResultLoot",
                        &format!(
                            "本场检测到五星礼装掉落 {new_drops} 个，累计 {next_total}/{target}"
                        ),
                    );
                    if action == FiveStarCeDropStopAction::Stop {
                        self.emit(
                            "BattleResultLoot",
                            &format!("五星礼装掉落累计达到 {target} 个，自动停止"),
                        );
                        self.transition_lifecycle(RunnerLifecycleEvent::Finished);
                        return;
                    }
                }
                Err(err) => {
                    self.emit_warn(
                        "BattleResultLoot",
                        &format!("五星礼装掉落检测失败，继续结算流程: {err}"),
                    );
                }
            }
        }

        self.emit("BattleResultLoot", "掉落结算，前往下一画面");
        if self.tap_at("BattleResultLoot", BATTLE_RESULT_LOOT_NEXT) {
            thread::sleep(ACTION_DELAY);
        }
    }

    fn count_visible_five_star_ce_drops(&mut self) -> Result<u32, String> {
        let path = resolve_resource_image_path(&self.app_handle, FIVE_STAR_CE_TEMPLATE_FILE_NAME)
            .ok_or_else(|| {
            format!("未找到五星礼装星级模板: {FIVE_STAR_CE_TEMPLATE_FILE_NAME}")
        })?;

        let template_size = five_star_ce_template_size(self.frame_w, self.frame_h);
        let mut count = 0;
        for region in five_star_ce_drop_regions() {
            let found = self
                .sidecar()
                .find_region_with_template_crop(
                    None,
                    &path,
                    region,
                    FULL_TEMPLATE_CROP,
                    Some(template_size),
                    FIVE_STAR_CE_THRESHOLD,
                )?
                .is_some();
            if found {
                count += 1;
            }
        }
        Ok(count)
    }

    fn capture_battle_result_loot_screenshot(&mut self) -> Result<PathBuf, String> {
        let dir = battle_result_loot_screenshot_dir(&self.app_handle);
        std::fs::create_dir_all(&dir).map_err(|err| format!("创建战利品截图目录失败: {err}"))?;
        let path = dir.join(battle_result_loot_screenshot_filename(
            std::time::SystemTime::now(),
            self.completed_mission_runs,
        ));
        let jpeg = self
            .sidecar()
            .get_frame_jpeg(LOOT_SCREENSHOT_WAIT_SECONDS)
            .map_err(|err| format!("获取视频帧失败: {err}"))?;
        std::fs::write(&path, jpeg).map_err(|err| format!("写入截图失败: {err}"))?;
        Ok(path)
    }

    fn capture_bond_level_up_screenshot(&mut self) -> Result<PathBuf, String> {
        let dir = bond_level_up_screenshot_dir(&self.app_handle);
        std::fs::create_dir_all(&dir).map_err(|err| format!("创建牵绊升级截图目录失败: {err}"))?;
        let path = dir.join(bond_level_up_screenshot_filename(
            std::time::SystemTime::now(),
            self.completed_mission_runs,
        ));
        let jpeg = self
            .sidecar()
            .get_frame_jpeg(LOOT_SCREENSHOT_WAIT_SECONDS)
            .map_err(|err| format!("获取视频帧失败: {err}"))?;
        std::fs::write(&path, jpeg).map_err(|err| format!("写入截图失败: {err}"))?;
        Ok(path)
    }

    pub(crate) fn capture_unknown_screen_timeout_screenshot(&mut self) -> Result<PathBuf, String> {
        let dir = unknown_screen_timeout_screenshot_dir(&self.app_handle);
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("创建无法识别画面截图目录失败: {err}"))?;
        let path = dir.join(unknown_screen_timeout_screenshot_filename(
            std::time::SystemTime::now(),
            self.completed_mission_runs,
        ));
        let jpeg = self
            .sidecar()
            .get_frame_jpeg(UNKNOWN_SCREEN_TIMEOUT_SCREENSHOT_WAIT_SECONDS)
            .map_err(|err| format!("获取视频帧失败: {err}"))?;
        std::fs::write(&path, jpeg).map_err(|err| format!("写入截图失败: {err}"))?;
        Ok(path)
    }

    pub(crate) fn handle_battle_result_friend_request(&mut self) {
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
    pub(crate) fn handle_battle_result_continue(&mut self) {
        if self.battle_result_continue_handled {
            self.emit("BattleResultContinue", "等待结算页切换…");
            thread::sleep(ACTION_DELAY);
            return;
        }
        self.battle_result_continue_handled = true;

        self.completed_mission_runs += 1;
        self.emit_run_progress();
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
            thread::sleep(ACTION_DELAY);
        } else {
            self.emit(
                "BattleResultContinue",
                &format!("结束任务（已完成 {} 轮）", self.completed_mission_runs),
            );
            if !self.tap_at("BattleResultContinue", BATTLE_RESULT_CONTINUE_STOP) {
                return;
            }
            self.transition_lifecycle(RunnerLifecycleEvent::Finished);
        }
    }
}
