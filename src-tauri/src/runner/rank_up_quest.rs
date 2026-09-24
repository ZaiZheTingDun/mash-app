use super::*;

const TARGET_MATCH_THRESHOLD: f64 = 0.70;
const TARGET_SIGNATURE_WEIGHTS: [f64; 3] = [0.45, 0.40, 0.15];
const REQUIRED_STABLE_ROWS: u8 = 2;
const REQUIRED_MISSING_OBSERVATIONS: u8 = 2;
const MAX_TOP_SWIPES: u8 = 24;
const MAX_FORWARD_SWIPES: u8 = 24;
const QUEST_ROW_TAP_X_RATIO: f64 = 0.58;
const LIST_SCROLL_X: f64 = 0.75;
const LIST_SCROLL_TOP: f64 = 0.32;
const LIST_SCROLL_BOTTOM: f64 = 0.78;

fn rank_up_signature_template_size(frame_w: u32, frame_h: u32) -> (u32, u32) {
    // The reference is a full-screen ADB capture. The sidecar resizes first and
    // applies `templateCrop` second, so this must stay the full current frame
    // size rather than the small signature-region size.
    (frame_w, frame_h)
}

fn rank_up_transition_message(phase: RankUpQuestPhase) -> Option<&'static str> {
    match phase {
        RankUpQuestPhase::AwaitingDeparture => Some("等待进入助战选择页面…"),
        RankUpQuestPhase::AwaitingReturn => Some("等待返回强化任务页面…"),
        RankUpQuestPhase::NeedList | RankUpQuestPhase::InQuest => None,
    }
}

fn rank_up_list_is_return(phase: RankUpQuestPhase) -> bool {
    matches!(
        phase,
        RankUpQuestPhase::InQuest | RankUpQuestPhase::AwaitingReturn
    )
}

fn rank_up_list_return_needs_completion_record(phase: RankUpQuestPhase) -> bool {
    phase == RankUpQuestPhase::InQuest
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RankUpQuestPhase {
    NeedList,
    AwaitingDeparture,
    InQuest,
    AwaitingReturn,
}

pub(crate) struct RankUpQuestRuntime {
    pub(crate) config: RankUpQuestWorkflowConfig,
    pub(crate) phase: RankUpQuestPhase,
    stable_key: Option<String>,
    stable_count: u8,
    missing_count: u8,
    seeking_top: bool,
    top_swipes: u8,
    forward_swipes: u8,
    clicked_in_pass: bool,
}

impl RankUpQuestRuntime {
    pub(crate) fn new(config: RankUpQuestWorkflowConfig) -> Self {
        let seeking_top = config.mode == RankUpQuestMode::All;
        Self {
            config,
            phase: RankUpQuestPhase::NeedList,
            stable_key: None,
            stable_count: 0,
            missing_count: 0,
            seeking_top,
            top_swipes: 0,
            forward_swipes: 0,
            clicked_in_pass: false,
        }
    }

    fn reset_stability(&mut self) {
        self.stable_key = None;
        self.stable_count = 0;
    }

    fn observe_row(&mut self, key: String) -> bool {
        if self.stable_key.as_deref() == Some(key.as_str()) {
            self.stable_count = self.stable_count.saturating_add(1);
        } else {
            self.stable_key = Some(key);
            self.stable_count = 1;
        }
        self.stable_count >= REQUIRED_STABLE_ROWS
    }

    fn row_key(row: &crate::screen::RankUpQuestRow) -> String {
        format!("{:.3}:{:.3}", row.region.x, row.region.y)
    }
}

impl Runner {
    pub(crate) fn rank_up_quest_active(&self) -> bool {
        self.rank_up_quest.is_some()
    }

    pub(crate) fn rank_up_quest_transition_message(&self) -> Option<&'static str> {
        rank_up_transition_message(self.rank_up_quest.as_ref()?.phase)
    }

    pub(crate) fn observe_rank_up_quest_screen(&mut self, screen: Screen) -> bool {
        let Some(runtime) = self.rank_up_quest.as_mut() else {
            return true;
        };
        if screen == Screen::RankUpQuest || screen == Screen::Unknown {
            return true;
        }
        match runtime.phase {
            RankUpQuestPhase::AwaitingDeparture => {
                if screen == Screen::SupportSelect {
                    runtime.phase = RankUpQuestPhase::InQuest;
                    runtime.reset_stability();
                    true
                } else {
                    self.fail_action(
                        "RankUpQuest",
                        "进入强化任务",
                        format!("点击后进入了非预期页面: {screen}"),
                    );
                    false
                }
            }
            RankUpQuestPhase::InQuest | RankUpQuestPhase::AwaitingReturn => true,
            RankUpQuestPhase::NeedList => {
                self.fail_action(
                    "RankUpQuest",
                    "启动强化任务",
                    "当前不是强化任务页面，请先在游戏中打开强化任务".into(),
                );
                false
            }
        }
    }

    pub(crate) fn mark_rank_up_quest_return_pending(&mut self) {
        if let Some(runtime) = self.rank_up_quest.as_mut() {
            runtime.phase = RankUpQuestPhase::AwaitingReturn;
            runtime.reset_stability();
        }
    }

    pub(crate) fn complete_rank_up_quest_run(&mut self) {
        self.completed_mission_runs += 1;
        self.emit_run_progress();
        self.team_changed = false;
        self.support_selected = false;
        self.support_scroll_count = 0;
        self.support_refresh_count = 0;
        self.support_class_tab_done = false;
        // Every rank-up quest returns to the list and opens a fresh support
        // selection page. Reapply CN's second-level EXTRA filter there too;
        // its modal choice may persist in-game, but the new page still needs
        // the configured filter to be explicitly selected for this run.
        self.support_extra_class_filter_configured = false;
        self.support_grand_section_seen = false;
        self.support_grand_section_misses = 0;
        self.support_level_progress = SupportLevelPanelProgress::default();
        self.servants_placed.clear();
        self.battle = BattleState::new();
    }

    pub(crate) fn handle_rank_up_quest(&mut self) {
        let Some(mut runtime) = self.rank_up_quest.take() else {
            return;
        };

        if runtime.phase == RankUpQuestPhase::AwaitingDeparture {
            self.emit("RankUpQuest", "等待进入助战选择页面…");
            self.rank_up_quest = Some(runtime);
            return;
        }
        if rank_up_list_is_return(runtime.phase) {
            if rank_up_list_return_needs_completion_record(runtime.phase) {
                // Some rank-up quests return directly to the list without a
                // detectable BattleResultContinue frame. Seeing the list
                // after SupportSelect was reached is the safe completion
                // boundary for that route.
                self.complete_rank_up_quest_run();
            }
            runtime.phase = RankUpQuestPhase::NeedList;
            runtime.reset_stability();
            self.emit("RankUpQuest", "已返回强化任务页面");
        }
        if runtime.phase != RankUpQuestPhase::NeedList {
            self.rank_up_quest = Some(runtime);
            return;
        }

        if runtime.config.mode == RankUpQuestMode::All && runtime.seeking_top {
            let at_top = self
                .sidecar()
                .find_element_by_name(None, "RankUpQuest", "scroll_start")
                .map(|result| result.found)
                .unwrap_or(false);
            if at_top || runtime.top_swipes >= MAX_TOP_SWIPES {
                runtime.seeking_top = false;
                runtime.top_swipes = 0;
                runtime.forward_swipes = 0;
                runtime.reset_stability();
                self.emit("RankUpQuest", "从列表顶部开始扫描强化任务");
            } else {
                runtime.top_swipes += 1;
                self.emit_debug("RankUpQuest", "向列表顶部滚动");
                let _ = self.swipe_with_settle_at(
                    "RankUpQuest",
                    Point::new(LIST_SCROLL_X, LIST_SCROLL_TOP),
                    Point::new(LIST_SCROLL_X, LIST_SCROLL_BOTTOM),
                    500,
                    250,
                );
                self.rank_up_quest = Some(runtime);
                thread::sleep(ACTION_DELAY);
                return;
            }
        }

        let rows = match self.sidecar().find_rank_up_quest_rows(None) {
            Ok(result) => result.rows,
            Err(error) => {
                self.fail_action("RankUpQuest", "识别强化任务列表", error);
                self.rank_up_quest = Some(runtime);
                return;
            }
        };

        match runtime.config.mode {
            RankUpQuestMode::Single => self.handle_single_rank_up_row(&mut runtime, &rows),
            RankUpQuestMode::All => self.handle_all_rank_up_rows(&mut runtime, &rows),
        }
        self.rank_up_quest = Some(runtime);
    }

    fn handle_single_rank_up_row(
        &mut self,
        runtime: &mut RankUpQuestRuntime,
        rows: &[crate::screen::RankUpQuestRow],
    ) {
        let Some(target) = runtime.config.target.clone() else {
            self.fail_action("RankUpQuest", "定位所选强化任务", "缺少目标截图".into());
            return;
        };
        let signature_template_size = rank_up_signature_template_size(self.frame_w, self.frame_h);
        let mut best: Option<(&crate::screen::RankUpQuestRow, f64)> = None;
        for row in rows {
            if row.signature_regions.len() != TARGET_SIGNATURE_WEIGHTS.len()
                || target.signature_regions.len() != TARGET_SIGNATURE_WEIGHTS.len()
            {
                continue;
            }
            let mut weighted_score = 0.0;
            let mut matched = true;
            for ((region, template_crop), weight) in row
                .signature_regions
                .iter()
                .zip(&target.signature_regions)
                .zip(TARGET_SIGNATURE_WEIGHTS)
            {
                let result = self.sidecar().find_region_with_template_crop_full(
                    None,
                    &target.reference_path,
                    *region,
                    *template_crop,
                    Some(signature_template_size),
                    0.0,
                );
                let Ok(result) = result else {
                    matched = false;
                    break;
                };
                weighted_score += result.score * weight;
            }
            if matched && best.is_none_or(|(_, score)| weighted_score > score) {
                best = Some((row, weighted_score));
            }
        }

        let Some((row, score)) = best.filter(|(_, score)| *score >= TARGET_MATCH_THRESHOLD) else {
            runtime.missing_count = runtime.missing_count.saturating_add(1);
            runtime.reset_stability();
            if runtime.missing_count >= REQUIRED_MISSING_OBSERVATIONS {
                if self.completed_mission_runs > 0 {
                    self.emit("RankUpQuest", "所选强化任务已完成");
                    self.transition_lifecycle(RunnerLifecycleEvent::Finished);
                } else {
                    self.fail_action(
                        "RankUpQuest",
                        "定位所选强化任务",
                        "所选任务不在当前列表位置，请重新截图选择".into(),
                    );
                }
            } else {
                self.emit_debug("RankUpQuest", "等待稳定识别所选强化任务");
            }
            return;
        };

        if !row.actionable {
            runtime.missing_count = runtime.missing_count.saturating_add(1);
            runtime.reset_stability();
            if runtime.missing_count >= REQUIRED_MISSING_OBSERVATIONS {
                if self.completed_mission_runs > 0 {
                    self.emit("RankUpQuest", "所选强化任务已完成");
                    self.transition_lifecycle(RunnerLifecycleEvent::Finished);
                } else {
                    self.fail_action("RankUpQuest", "开始强化任务", "所选任务当前不可点击".into());
                }
            } else {
                self.emit_debug("RankUpQuest", "等待稳定确认所选强化任务已变暗");
            }
            return;
        }
        runtime.missing_count = 0;
        self.emit_debug("RankUpQuest", &format!("所选强化任务视觉匹配 {:.3}", score));
        if !runtime.observe_row(RankUpQuestRuntime::row_key(row)) {
            self.emit("RankUpQuest", "确认所选强化任务可点击…");
            return;
        }
        self.tap_rank_up_quest_row(runtime, row);
    }

    fn handle_all_rank_up_rows(
        &mut self,
        runtime: &mut RankUpQuestRuntime,
        rows: &[crate::screen::RankUpQuestRow],
    ) {
        if let Some(row) = rows.iter().find(|row| row.actionable) {
            if !runtime.observe_row(RankUpQuestRuntime::row_key(row)) {
                self.emit("RankUpQuest", "确认下一项强化任务可点击…");
                return;
            }
            runtime.clicked_in_pass = true;
            self.tap_rank_up_quest_row(runtime, row);
            return;
        }

        runtime.reset_stability();
        let at_bottom = self
            .sidecar()
            .find_element_by_name(None, "RankUpQuest", "scroll_end")
            .map(|result| result.found)
            .unwrap_or(false)
            || runtime.forward_swipes >= MAX_FORWARD_SWIPES;
        if at_bottom {
            if runtime.clicked_in_pass {
                runtime.clicked_in_pass = false;
                runtime.seeking_top = true;
                runtime.forward_swipes = 0;
                self.emit("RankUpQuest", "重新从顶部确认是否还有可强化任务");
            } else {
                self.emit("RankUpQuest", "所有可强化任务均已完成");
                self.transition_lifecycle(RunnerLifecycleEvent::Finished);
            }
            return;
        }

        runtime.forward_swipes += 1;
        self.emit_debug("RankUpQuest", "当前画面没有可强化任务，继续向下扫描");
        let _ = self.swipe_with_settle_at(
            "RankUpQuest",
            Point::new(LIST_SCROLL_X, LIST_SCROLL_BOTTOM),
            Point::new(LIST_SCROLL_X, LIST_SCROLL_TOP),
            500,
            250,
        );
        thread::sleep(ACTION_DELAY);
    }

    fn tap_rank_up_quest_row(
        &mut self,
        runtime: &mut RankUpQuestRuntime,
        row: &crate::screen::RankUpQuestRow,
    ) {
        let point = Point::new(
            row.region.x + row.region.w * QUEST_ROW_TAP_X_RATIO,
            row.region.y + row.region.h * 0.5,
        );
        self.emit("RankUpQuest", "点击强化任务，等待进入助战选择");
        if self.tap_at("RankUpQuest", point) {
            runtime.phase = RankUpQuestPhase::AwaitingDeparture;
            runtime.reset_stability();
            thread::sleep(ACTION_DELAY);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_click_requires_two_stable_observations() {
        let mut runtime = RankUpQuestRuntime::new(RankUpQuestWorkflowConfig::all());

        assert!(!runtime.observe_row("row-a".into()));
        assert!(runtime.observe_row("row-a".into()));
    }

    #[test]
    fn changing_rows_resets_the_click_guard() {
        let mut runtime = RankUpQuestRuntime::new(RankUpQuestWorkflowConfig::all());

        assert!(!runtime.observe_row("row-a".into()));
        assert!(!runtime.observe_row("row-b".into()));
        assert!(runtime.observe_row("row-b".into()));
    }

    #[test]
    fn all_mode_starts_by_seeking_the_list_top() {
        let runtime = RankUpQuestRuntime::new(RankUpQuestWorkflowConfig::all());

        assert!(runtime.seeking_top);
        assert_eq!(runtime.phase, RankUpQuestPhase::NeedList);
        assert!(!runtime.clicked_in_pass);
    }

    #[test]
    fn signature_reference_is_resized_to_the_full_stream_before_crop() {
        assert_eq!(rank_up_signature_template_size(1920, 1080), (1920, 1080));
        assert_ne!(rank_up_signature_template_size(1920, 1080), (173, 119));
    }

    #[test]
    fn transition_messages_cover_departure_and_return_without_calling_them_unknown() {
        assert_eq!(
            rank_up_transition_message(RankUpQuestPhase::AwaitingDeparture),
            Some("等待进入助战选择页面…")
        );
        assert_eq!(
            rank_up_transition_message(RankUpQuestPhase::AwaitingReturn),
            Some("等待返回强化任务页面…")
        );
        assert_eq!(rank_up_transition_message(RankUpQuestPhase::InQuest), None);
    }

    #[test]
    fn list_arrival_recovers_both_result_routes_without_double_counting() {
        assert!(rank_up_list_is_return(RankUpQuestPhase::InQuest));
        assert!(rank_up_list_return_needs_completion_record(
            RankUpQuestPhase::InQuest
        ));
        assert!(rank_up_list_is_return(RankUpQuestPhase::AwaitingReturn));
        assert!(!rank_up_list_return_needs_completion_record(
            RankUpQuestPhase::AwaitingReturn
        ));
        assert!(!rank_up_list_is_return(RankUpQuestPhase::NeedList));
        assert!(!rank_up_list_is_return(RankUpQuestPhase::AwaitingDeparture));
    }
}
