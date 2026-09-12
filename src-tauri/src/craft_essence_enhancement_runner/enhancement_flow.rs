//! Enhancement confirmation, warning, and result-return flow.

use super::*;

impl CraftEssenceEnhancementRunner {
    #[cfg(any())]
    pub(super) fn handle_selected_main(&mut self, template_ready: bool) -> bool {
        let ready = if template_ready {
            self.enhance_button_absent_recovery_taps = 0;
            true
        } else {
            let button_match = match self.sidecar().find_element_by_name(
                None,
                SCREEN_NAME,
                "button_enhancement_ready",
            ) {
                Ok(button_match) => button_match,
                Err(err) => {
                    self.fail(
                        "CraftEssenceEnhancement",
                        format!("识别强化按钮失败: {err}"),
                    );
                    return false;
                }
            };
            let mean_luma = match self.sidecar().read_region_luma(None, ENHANCE_BUTTON_REGION) {
                Ok(mean_luma) => mean_luma,
                Err(err) => {
                    self.fail(
                        "CraftEssenceEnhancement",
                        format!("读取强化按钮亮度失败: {err}"),
                    );
                    return false;
                }
            };
            match classify_enhancement_button(button_match.score, mean_luma) {
                EnhancementReadyState::Absent => {
                    self.enhance_button_absent_recovery_taps += 1;
                    if self.enhance_button_absent_recovery_taps
                        >= ENHANCE_BUTTON_ABSENT_MAX_RECOVERY_TAPS
                    {
                        self.fail(
                            "CraftEssenceEnhancement",
                            format!(
                                "主页面状态中未检测到强化按钮（形状分数 {:.3}）",
                                button_match.score
                            ),
                        );
                        return false;
                    }
                    self.emit(
                        "EnhancementResultRecovery",
                        &format!(
                            "未检测到强化按钮，尝试点击顶部返回（形状分数 {:.3}）",
                            button_match.score
                        ),
                    );
                    if !self.tap_at("EnhancementResultRecovery", ENHANCEMENT_SKIP_BUTTON) {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(700));
                    return true;
                }
                EnhancementReadyState::Ready => {
                    self.enhance_button_absent_recovery_taps = 0;
                    self.emit(
                        "CraftEssenceEnhancement",
                        &format!(
                            "通过按钮形状和亮度确认强化已就绪（分数 {:.3}，亮度 {mean_luma:.1}）",
                            button_match.score
                        ),
                    );
                    true
                }
                EnhancementReadyState::NotReady => {
                    self.enhance_button_absent_recovery_taps = 0;
                    false
                }
                EnhancementReadyState::Transitioning => {
                    self.enhance_button_absent_recovery_taps = 0;
                    self.emit(
                        "CraftEssenceEnhancement",
                        &format!(
                            "强化按钮状态正在变化，继续等待（分数 {:.3}，亮度 {mean_luma:.1}）",
                            button_match.score
                        ),
                    );
                    thread::sleep(Duration::from_millis(500));
                    return true;
                }
            }
        };

        if ready {
            self.post_enhancement_not_ready_checks = 0;
        } else {
            self.post_enhancement_not_ready_checks =
                self.post_enhancement_not_ready_checks.saturating_add(1);
        }

        match selected_main_action(
            ready,
            self.recommend_execute_tapped,
            self.completed_enhancements,
            self.post_enhancement_not_ready_checks,
        ) {
            SelectedMainAction::OpenRecommendation => {
                if self.recommend_open_attempts >= RECOMMEND_OPEN_MAX_ATTEMPTS {
                    self.fail(
                        "CraftEssenceEnhancement",
                        "多次点击推荐选择后，对话框仍未打开".into(),
                    );
                    return false;
                }
                self.emit("CraftEssenceEnhancement", "打开推荐强化素材设置");
                if !self.tap_at("CraftEssenceEnhancement", RECOMMEND_MATERIAL_BUTTON) {
                    return false;
                }
                self.recommend_open_attempts += 1;
                thread::sleep(Duration::from_millis(800));
                true
            }
            SelectedMainAction::WaitForAutoSelection => {
                self.emit("CraftEssenceEnhancement", "等待自动配置强化素材");
                thread::sleep(Duration::from_millis(700));
                true
            }
            SelectedMainAction::Enhance => {
                if self.enhance_open_attempts >= ENHANCE_OPEN_MAX_ATTEMPTS {
                    self.fail(
                        "CraftEssenceEnhancement",
                        "多次点击强化按钮后，确认对话框仍未打开".into(),
                    );
                    return false;
                }
                self.emit(
                    "CraftEssenceEnhancement",
                    &format!("开始第 {} 次强化", self.completed_enhancements + 1),
                );
                if !self.tap_probe_or_point(
                    "CraftEssenceEnhancement",
                    "button_enhancement_ready",
                    ENHANCE_BUTTON,
                ) {
                    return false;
                }
                self.enhance_open_attempts += 1;
                thread::sleep(Duration::from_millis(800));
                true
            }
            SelectedMainAction::Finished => {
                self.transition(LifecycleEvent::Finished);
                self.emit(
                    "CraftEssenceEnhancement",
                    &format!(
                        "自动强化结束：概念礼装已满级或没有可用强化素材（共完成 {} 次强化）",
                        self.completed_enhancements
                    ),
                );
                false
            }
        }
    }

    pub(super) fn handle_enhancement_confirm_dialog(&mut self) -> bool {
        if self.pending_enhancement_stage.is_some() {
            self.awaiting_enhancement_return = true;
            self.enhancement_return_waits = 0;
            self.enhancement_main_return_checks = 0;
            thread::sleep(Duration::from_millis(500));
            return true;
        }
        if !enhancement_confirm_can_arm(self.strategy_stage, self.materials_committed) {
            self.fail(
                "EnhancementConfirmDialog",
                format!(
                    "当前策略阶段或素材提交状态不允许执行强化: {:?}",
                    self.strategy_stage
                ),
            );
            return false;
        }
        self.emit("EnhancementConfirmDialog", "确认执行概念礼装强化");
        if !self.tap_at("EnhancementConfirmDialog", ENHANCE_CONFIRM_BUTTON) {
            return false;
        }
        self.pending_enhancement_stage = Some(self.strategy_stage);
        self.awaiting_enhancement_return = true;
        self.enhancement_left_main = false;
        self.enhancement_return_waits = 0;
        self.enhancement_main_return_checks = 0;
        self.post_enhancement_target_read_failures = 0;
        thread::sleep(Duration::from_millis(900));
        true
    }

    pub(super) fn handle_enhanced_material_warning_dialog(&mut self) -> bool {
        if self.pending_enhancement_stage.is_some()
            || !enhancement_confirm_can_arm(self.strategy_stage, self.materials_committed)
        {
            self.fail(
                "EnhancedMaterialWarningDialog",
                format!(
                    "当前策略阶段或素材提交状态不允许确认已强化素材: {:?}",
                    self.strategy_stage
                ),
            );
            return false;
        }
        self.emit(
            "EnhancedMaterialWarningDialog",
            "确认本批只包含已登记的经验包，滑动解锁本次决定按钮",
        );
        if !self.swipe_at(
            "EnhancedMaterialWarningDialog",
            ENHANCED_MATERIAL_WARNING_SLIDER_FROM,
            ENHANCED_MATERIAL_WARNING_SLIDER_TO,
            900,
        ) {
            return false;
        }
        thread::sleep(Duration::from_millis(450));
        if !self.tap_at(
            "EnhancedMaterialWarningDialog",
            ENHANCED_MATERIAL_WARNING_DECIDE_BUTTON,
        ) {
            return false;
        }
        thread::sleep(Duration::from_millis(800));
        true
    }

    pub(super) fn handle_enhancement_return(&mut self, screen: Screen) -> bool {
        match enhancement_return_action(screen, self.enhancement_left_main) {
            EnhancementReturnAction::ObserveReturnedMain => {
                self.enhancement_main_return_checks =
                    self.enhancement_main_return_checks.saturating_add(1);
                self.emit("EnhancementAnimation", "确认已返回概念礼装强化页面");
                if !self.tap_at("EnhancementAnimation", ENHANCEMENT_SKIP_BUTTON) {
                    return false;
                }
                thread::sleep(Duration::from_millis(700));
                if !enhancement_main_return_confirmed(self.enhancement_main_return_checks) {
                    return true;
                }
                let target = match self.sidecar().read_craft_essence_main_target(None) {
                    Ok(target) => target,
                    Err(err) => {
                        self.post_enhancement_target_read_failures =
                            self.post_enhancement_target_read_failures.saturating_add(1);
                        if self.post_enhancement_target_read_failures >= GRID_READ_MAX_FAILURES {
                            self.fail(
                                "CraftEssenceEnhancement",
                                format!("强化后读取目标等级上限失败: {err}"),
                            );
                            return false;
                        }
                        self.emit(
                            "CraftEssenceEnhancement",
                            "强化后目标等级上限读取不稳定，原地重试",
                        );
                        return true;
                    }
                };
                let completed_stage = match complete_pending_enhancement(
                    &mut self.pending_enhancement_stage,
                    &target,
                ) {
                    Ok(stage) => stage,
                    Err(EnhancementCompletionError::TargetUnreadable) => {
                        self.post_enhancement_target_read_failures =
                            self.post_enhancement_target_read_failures.saturating_add(1);
                        if self.post_enhancement_target_read_failures >= GRID_READ_MAX_FAILURES {
                            self.fail(
                                "CraftEssenceEnhancement",
                                format!("强化后无法确认目标等级上限（OCR：{}）", target.text),
                            );
                            return false;
                        }
                        self.emit(
                            "CraftEssenceEnhancement",
                            &format!(
                                "强化后暂未识别到目标等级上限，原地重试（{}/{GRID_READ_MAX_FAILURES}，OCR：{}）",
                                self.post_enhancement_target_read_failures,
                                target.text
                            ),
                        );
                        return true;
                    }
                    Err(EnhancementCompletionError::CapMismatch { expected, actual }) => {
                        let expected_label = if self.pending_enhancement_stage
                            == Some(StrategyStage::PacketAutoFeedPending)
                        {
                            "20/30/40/50".to_string()
                        } else {
                            expected.to_string()
                        };
                        self.fail(
                            "CraftEssenceEnhancement",
                            format!(
                                "强化后目标等级上限校验失败：识别为 {}/{}，当前阶段要求上限 {expected_label}；不会登记本次产物",
                                target.level.unwrap_or(0),
                                actual.unwrap_or(0)
                            ),
                        );
                        return false;
                    }
                    Err(EnhancementCompletionError::MissingPending) => {
                        self.fail(
                            "CraftEssenceEnhancement",
                            "强化返回时没有待结算记录，拒绝重复结算".into(),
                        );
                        return false;
                    }
                    Err(EnhancementCompletionError::IllegalStage(stage)) => {
                        self.fail(
                            "CraftEssenceEnhancement",
                            format!("强化返回记录包含非法策略阶段: {stage:?}"),
                        );
                        return false;
                    }
                };
                if !target.found {
                    self.emit(
                        "CraftEssenceEnhancement",
                        &format!(
                            "强化后当前等级读取不完整，已由独立上限证据确认上限 {}",
                            target.level_cap.unwrap_or(0)
                        ),
                    );
                }
                self.awaiting_enhancement_return = false;
                self.enhancement_left_main = false;
                self.enhancement_return_waits = 0;
                self.enhancement_main_return_checks = 0;
                self.post_enhancement_target_read_failures = 0;
                self.enhance_open_attempts = 0;
                self.completed_enhancements += 1;
                self.materials_committed = false;
                self.material_seen_cells.clear();
                match completed_stage {
                    StrategyStage::BombBaseSelected => {
                        self.strategy_stage = StrategyStage::FindBombBaseToLock;
                    }
                    StrategyStage::PacketSelected => {
                        self.strategy_stage = next_packet_stage_after_enhancement(
                            completed_stage,
                            self.packet_fingerprints.len(),
                        )
                        .expect("packet break stage must advance to automatic feed");
                        self.recommend_ready_waits = 0;
                        self.recommend_executed_for_target = false;
                        self.enhance_open_attempts = 0;
                        self.emit(
                            "CraftEssenceEnhancement",
                            "同名礼装强化完成，经验包已 1 破；等待游戏自动配置下一次素材",
                        );
                    }
                    StrategyStage::PacketAutoFeedPending => {
                        self.packet_fingerprints
                            .push(self.packet_fingerprint.clone());
                        self.packet_fingerprint.clear();
                        self.strategy_stage = next_packet_stage_after_enhancement(
                            completed_stage,
                            self.packet_fingerprints.len(),
                        )
                        .expect("packet automatic feed stage must advance after one enhancement");
                    }
                    StrategyStage::FastAutoFeedPending => {
                        self.current_bomb_level = target
                            .level
                            .expect("fast strategy requires a readable level");
                        if fast_bomb_is_complete(&target) {
                            self.transition(LifecycleEvent::Finished);
                            self.emit(
                                "CraftEssenceEnhancement",
                                &format!(
                                    "当前丸子已强化至 50 级（共完成 {} 次强化）",
                                    self.completed_enhancements
                                ),
                            );
                            return false;
                        }
                        self.recommend_ready_waits = 0;
                        self.enhance_open_attempts = 0;
                        self.emit(
                            "CraftEssenceEnhancement",
                            &format!(
                                "当前丸子已强化至 {}/50，等待自动配置下一批素材",
                                self.current_bomb_level
                            ),
                        );
                    }
                    StrategyStage::BombSelectedForFeed => {
                        self.packet_fingerprints.clear();
                        self.packet_feed_remaining.clear();
                        self.feed_inventory_packets = false;
                        self.strategy_stage = StrategyStage::InspectBomb;
                    }
                    _ => unreachable!("completed stage was validated before state transition"),
                }
                self.emit(
                    "CraftEssenceEnhancement",
                    &format!(
                        "第 {} 次强化完成，检查下一轮素材",
                        self.completed_enhancements
                    ),
                );
                true
            }
            EnhancementReturnAction::WaitForConfirmationClose => {
                self.enhancement_main_return_checks = 0;
                self.enhancement_return_waits = self.enhancement_return_waits.saturating_add(1);
                if self.enhancement_return_waits >= ENHANCEMENT_RETURN_MAX_WAITS {
                    self.fail(
                        "EnhancementConfirmDialog",
                        "点击决定后，强化确认对话框仍未关闭".into(),
                    );
                    return false;
                }
                thread::sleep(Duration::from_millis(500));
                true
            }
            EnhancementReturnAction::CloseExpOverflow => {
                self.enhancement_main_return_checks = 0;
                self.enhancement_left_main = true;
                self.enhancement_return_waits = self.enhancement_return_waits.saturating_add(1);
                if self.enhancement_return_waits >= ENHANCEMENT_RETURN_MAX_WAITS {
                    self.fail(
                        "EnhancementAnimation",
                        "经验值溢出提示持续未关闭，等待概念礼装强化结束超时".into(),
                    );
                    return false;
                }
                self.emit(
                    "EnhancementAnimation",
                    "检测到大成功或极大成功导致经验值溢出，关闭未使用素材提示",
                );
                if !self.tap_at("EnhancementAnimation", EXP_OVERFLOW_CLOSE_BUTTON) {
                    return false;
                }
                thread::sleep(Duration::from_millis(700));
                true
            }
            EnhancementReturnAction::TapSkip { mark_left_main } => {
                self.enhancement_main_return_checks = 0;
                self.enhancement_left_main |= mark_left_main;
                self.enhancement_return_waits = self.enhancement_return_waits.saturating_add(1);
                if self.enhancement_return_waits >= ENHANCEMENT_RETURN_MAX_WAITS {
                    self.fail(
                        "EnhancementAnimation",
                        if self.enhancement_left_main {
                            "等待概念礼装强化结束超时"
                        } else {
                            "点击决定后未能确认进入强化动画"
                        }
                        .into(),
                    );
                    return false;
                }
                self.emit(
                    "EnhancementAnimation",
                    if self.enhancement_left_main {
                        "点击页面顶部跳过强化动画"
                    } else {
                        "等待进入强化动画"
                    },
                );
                if !self.tap_at("EnhancementAnimation", ENHANCEMENT_SKIP_BUTTON) {
                    return false;
                }
                thread::sleep(Duration::from_millis(700));
                true
            }
            EnhancementReturnAction::Unexpected => {
                self.fail(
                    "EnhancementAnimation",
                    format!("强化动画期间进入了意外页面: {screen:?}"),
                );
                false
            }
        }
    }
}
