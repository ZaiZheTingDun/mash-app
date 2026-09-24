//! Opt-in test faults kept out of the normal battle-selection policy.

use super::*;
use crate::commands::settings::consume_simulate_stuck_attack_selection;

pub(crate) fn should_consume_stuck_selection_test(screen: &str, selected_count: usize) -> bool {
    screen == "Attack" && selected_count == 2
}

impl Runner {
    /// Simulate one missed third-card tap, then let the normal recovery path run.
    pub(crate) fn tap_pick_with_stuck_selection_test(
        &mut self,
        screen: &str,
        point: Point,
        selected_count: usize,
    ) -> bool {
        if should_consume_stuck_selection_test(screen, selected_count) {
            match consume_simulate_stuck_attack_selection(&self.app_handle) {
                Ok(true) => {
                    self.emit_warn(
                        screen,
                        "调试测试：已故意跳过第 3 张卡的点击，等待触发选卡恢复",
                    );
                    return true;
                }
                Ok(false) => {}
                Err(err) => self.emit_warn(
                    screen,
                    &format!("读取选卡卡住测试开关失败，本次正常选卡: {err}"),
                ),
            }
        }
        self.tap_at(screen, point)
    }
}
