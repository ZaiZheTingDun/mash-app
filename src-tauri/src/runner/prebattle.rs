//! Pre-battle team confirmation and servant placement handlers.
//!
//! This module owns TeamConfirm, TeamChange, and ServantSelect screen handling
//! before the quest enters battle automation.

use super::*;

impl Runner {
    pub(crate) fn handle_team_confirm(&mut self) {
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

    pub(crate) fn handle_servant_select(&mut self) {
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
