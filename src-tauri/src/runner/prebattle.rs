//! Pre-battle team confirmation and servant placement handlers.
//!
//! This module owns TeamConfirm, TeamChange, and ServantSelect screen handling
//! before the quest enters battle automation.

use super::team_verification::{TeamArtworkLoadGate, TeamArtworkLoadWait, TEAM_ARTWORK_LOAD_POLL};
use super::*;

/// Normalized location of the Master outfit icon on the TeamConfirm page.
/// The game keeps this control anchored to the lower-left corner across
/// landscape resolutions; the generous padding covers small layout shifts.
pub(super) const MYSTIC_CODE_ITEM_REGION: NormRect = NormRect {
    x: 0.007,
    y: 0.88,
    w: 0.073,
    h: 0.12,
};
pub(super) const MYSTIC_CODE_ITEM_THRESHOLD: f64 = 0.62;

/// One fixed icon size, normalized from the 84×84 icon at 1920×1080.
pub(super) fn mystic_code_item_template_size(frame_w: u32, frame_h: u32) -> (u32, u32) {
    (
        ((84.0 * frame_w as f64 / 1920.0).round() as u32).max(1),
        ((84.0 * frame_h as f64 / 1080.0).round() as u32).max(1),
    )
}

impl Runner {
    pub(crate) fn handle_team_confirm(&mut self) -> AutomationResult<()> {
        if matches!(self.battle.flow, BattleFlowState::AwaitingBattleLoad { .. }) {
            return Ok(());
        }
        if self.config.party_order.is_some() && !self.team_changed {
            self.emit("TeamConfirm", "需要调整队伍顺序，进入编成变更");
            if !self.tap_at("TeamConfirm", Point::new(0.83, 0.90)) {
                return Ok(());
            }
            thread::sleep(ACTION_DELAY);
            return Ok(());
        }

        if let Some(slot_cfg) = self.next_unfilled_slot() {
            self.emit(
                "TeamConfirm",
                &format!("选择从者到槽位 {}", slot_cfg.slot_index),
            );
            let slot_x = slot_x_position(slot_cfg.slot_index);
            if !self.tap_at("TeamConfirm", Point::new(slot_x, 0.45)) {
                return Ok(());
            }
            thread::sleep(ACTION_DELAY);
            return Ok(());
        }

        self.start_quest()?;
        Ok(())
    }

    pub(crate) fn retry_quest_start(&mut self, attempt: u32) -> AutomationResult<()> {
        self.emit(
            "TeamConfirm",
            &format!("开始任务后仍停留在队伍确认页，重试点击（第 {attempt} 次）"),
        );
        self.start_quest()?;
        Ok(())
    }

    fn start_quest(&mut self) -> AutomationResult<()> {
        let mut artwork_wait = TeamArtworkLoadWait::default();
        let mut loading_message_emitted = false;
        loop {
            if self.is_cancelled() {
                return Ok(());
            }
            // The page, its start button and every check are read
            // from this exact frame, rather than separate live-frame requests.
            let snapshot = match self.capture_team_confirm_frame().recoverable()? {
                Ok(snapshot) => snapshot,
                Err(err) => {
                    self.fail_action("TeamConfirm", "获取开始任务前画面", err.to_string());
                    return Ok(());
                }
            };
            let image_path = snapshot.path().join("team-confirm.png");
            match self.sidecar().detect(Some(&image_path)).recoverable()? {
                Ok(Screen::TeamConfirm) => {}
                Ok(_) => {
                    self.battle
                        .transition(BattleFlowEvent::BattleLoadRetryDeferred);
                    self.emit_debug("TeamConfirm", "当前帧已不在队伍确认页，等待下一次画面识别");
                    return Ok(());
                }
                Err(err) => {
                    self.fail_action("TeamConfirm", "确认开始任务前页面", err.to_string());
                    return Ok(());
                }
            }
            let result = self.check_team(&image_path);
            match artwork_wait.observe(!result.issues.is_empty(), Instant::now()) {
                TeamArtworkLoadGate::Ready => {}
                TeamArtworkLoadGate::Waiting => {
                    if !loading_message_emitted {
                        self.emit("TeamConfirm", "等待编队立绘加载…");
                        loading_message_emitted = true;
                    }
                    drop(snapshot);
                    thread::sleep(TEAM_ARTWORK_LOAD_POLL);
                    continue;
                }
                TeamArtworkLoadGate::Blocked => {
                    self.save_team_verification_failure(&image_path, &result.evidence);
                    drop(snapshot);
                    self.wait_for_team_verification(result.issues);
                    artwork_wait = TeamArtworkLoadWait::default();
                    loading_message_emitted = false;
                    continue;
                }
            }
            if self.is_cancelled() {
                return Ok(());
            }

            self.emit("TeamConfirm", "队伍就绪，点击开始任务");
            if !self.tap_at("TeamConfirm", Point::new(0.90, 0.93)) {
                return Ok(());
            }
            self.battle.transition(BattleFlowEvent::QuestStartTapped {
                source: BattleLoadSource::TeamConfirm,
                at: Instant::now(),
            });
            self.emit("TeamConfirm", "已点击开始任务，等待进入战斗页面");
            thread::sleep(ACTION_DELAY);
            return Ok(());
        }
    }

    fn capture_team_confirm_frame(&mut self) -> AutomationResult<tempfile::TempDir> {
        let dir = tempfile::tempdir().context("创建临时截图目录失败")?;
        let png = self.sidecar().get_frame_png(0.0)?;
        std::fs::write(dir.path().join("team-confirm.png"), png).context("写入临时截图失败")?;
        Ok(dir)
    }

    pub(crate) fn handle_team_change(&mut self) {
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

    pub(crate) fn handle_servant_select(&mut self) -> AutomationResult<()> {
        Ok(if let Some(slot_cfg) = self.next_unfilled_slot() {
            let servant_key = format!("servant_{}", slot_cfg.servant_id);
            let region = NormRect {
                x: 0.0,
                y: 0.10,
                w: 1.0,
                h: 0.85,
            };

            if let Ok(Some(pos)) = self
                .sidecar()
                .find_element(None, &servant_key, region, 0.8)
                .recoverable()?
            {
                self.emit(
                    "ServantSelect",
                    &format!("找到从者 {}，点击选择", slot_cfg.servant_id),
                );
                if !self.tap_at("ServantSelect", pos) {
                    return Ok(());
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
                    return Ok(());
                }
                thread::sleep(ACTION_DELAY);
            }
        } else {
            self.emit("ServantSelect", "所有从者已选择，返回");
            if !self.tap_at("ServantSelect", Point::new(0.05, 0.05)) {
                return Ok(());
            }
            thread::sleep(ACTION_DELAY);
        })
    }

    pub(crate) fn next_unfilled_slot(&self) -> Option<ServantSlotConfig> {
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

pub(crate) fn slot_x_position(index: u32) -> f64 {
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
