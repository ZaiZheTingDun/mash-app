//! Strategy-stage handling for craft essence enhancement.

use super::*;

impl CraftEssenceEnhancementRunner {
    pub(super) fn handle_strategy_main(&mut self, template_ready: bool) -> bool {
        match self.strategy_stage {
            StrategyStage::BombSelected => {
                self.strategy_stage = StrategyStage::SelectPacketBase;
                self.open_target_select("准备制作 1 破 1 星经验包")
            }
            StrategyStage::FindBombBaseToLock => {
                self.open_target_select("重新找到刚制作的 1 星满破底卡并安全上锁")
            }
            StrategyStage::SelectBombForTransfer => {
                self.open_target_select("经验包制作完成，重新选择丸子")
            }
            StrategyStage::InspectBomb => self.open_target_select("检查丸子当前等级"),
            StrategyStage::SelectBomb
            | StrategyStage::SelectBombBase
            | StrategyStage::SelectPacketBase => self.open_target_select("重新识别目标概念礼装"),
            StrategyStage::PacketAutoFeedPending => {
                self.handle_packet_auto_feed_main(template_ready)
            }
            StrategyStage::FastAutoFeedPending => self.handle_fast_auto_feed_main(template_ready),
            StrategyStage::BombBaseSelected
            | StrategyStage::PacketSelected
            | StrategyStage::BombSelectedForFeed => {
                if !self.materials_committed {
                    self.reset_material_selection();
                    self.emit(
                        "CraftEssenceEnhancement",
                        match self.strategy_stage {
                            StrategyStage::BombBaseSelected => {
                                "打开素材列表：只选择 4 张同名未锁定 1 星，制作满破底卡"
                            }
                            StrategyStage::PacketSelected => {
                                "打开素材列表：只选择 1 张同名未锁定 1 星，先完成 1 破"
                            }
                            StrategyStage::BombSelectedForFeed => {
                                if self.feed_inventory_packets {
                                    "打开素材列表：选择库存中未锁定、已升级的 1 星礼装喂给丸子"
                                } else {
                                    "打开素材列表：丸子只吃本批刚制作的至少 1 破 1 星经验包"
                                }
                            }
                            _ => unreachable!(),
                        },
                    );
                    if !self.tap_at("CraftEssenceEnhancement", MATERIAL_SELECT_BUTTON) {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(850));
                    return true;
                }

                let ready = if template_ready {
                    true
                } else {
                    let button_match = match self.sidecar().find_element_by_name(
                        None,
                        SCREEN_NAME,
                        "button_enhancement_ready",
                    ) {
                        Ok(result) => result,
                        Err(err) => {
                            self.fail(
                                "CraftEssenceEnhancement",
                                format!("识别强化按钮失败: {err}"),
                            );
                            return false;
                        }
                    };
                    let mean_luma =
                        match self.sidecar().read_region_luma(None, ENHANCE_BUTTON_REGION) {
                            Ok(value) => value,
                            Err(err) => {
                                self.fail(
                                    "CraftEssenceEnhancement",
                                    format!("读取强化按钮亮度失败: {err}"),
                                );
                                return false;
                            }
                        };
                    matches!(
                        classify_enhancement_button(button_match.score, mean_luma),
                        EnhancementReadyState::Ready
                    )
                };

                if !ready {
                    self.recommend_ready_waits = self.recommend_ready_waits.saturating_add(1);
                    if self.recommend_ready_waits >= RECOMMEND_READY_MAX_WAITS {
                        self.fail(
                            "CraftEssenceEnhancement",
                            "素材已决定，但强化按钮连续多次未就绪".into(),
                        );
                        return false;
                    }
                    thread::sleep(Duration::from_millis(600));
                    return true;
                }

                self.recommend_ready_waits = 0;
                self.emit(
                    "CraftEssenceEnhancement",
                    match self.strategy_stage {
                        StrategyStage::BombBaseSelected => "制作 1 星满破丸子底卡",
                        StrategyStage::PacketSelected => "只喂 1 张同名礼装，完成经验包 1 破",
                        StrategyStage::BombSelectedForFeed => {
                            "将未锁定、已升级的 1 星经验包喂给丸子"
                        }
                        _ => unreachable!(),
                    },
                );
                if self.enhance_open_attempts >= ENHANCE_OPEN_MAX_ATTEMPTS {
                    self.fail(
                        "CraftEssenceEnhancement",
                        "多次点击强化按钮后，确认对话框仍未打开".into(),
                    );
                    return false;
                }
                if !self.tap_probe_or_point(
                    "CraftEssenceEnhancement",
                    "button_enhancement_ready",
                    ENHANCE_BUTTON,
                ) {
                    return false;
                }
                self.enhance_open_attempts = self.enhance_open_attempts.saturating_add(1);
                thread::sleep(Duration::from_millis(800));
                true
            }
            StrategyStage::LockBombBaseActive
            | StrategyStage::VerifyBombBaseLock
            | StrategyStage::ExitBombBaseLockMode
            | StrategyStage::QpEfficientComplete => false,
        }
    }

    pub(super) fn handle_packet_auto_feed_main(&mut self, template_ready: bool) -> bool {
        self.handle_auto_feed_main(template_ready, AutoFeedStrategy::QpEfficientPacket)
    }

    pub(super) fn handle_fast_auto_feed_main(&mut self, template_ready: bool) -> bool {
        self.handle_auto_feed_main(template_ready, AutoFeedStrategy::FastBomb)
    }

    pub(super) fn handle_auto_feed_main(
        &mut self,
        template_ready: bool,
        strategy: AutoFeedStrategy,
    ) -> bool {
        if recommendation_needs_execution(
            self.recommend_executed_for_target,
            self.recommend_configured_profile,
            self.recommend_profile,
        ) {
            if self.recommend_open_attempts >= RECOMMEND_OPEN_MAX_ATTEMPTS {
                self.fail(
                    "CraftEssenceEnhancement",
                    "多次点击推荐选择后，对话框仍未打开".into(),
                );
                return false;
            }
            self.recommend_reset_done =
                self.recommend_configured_profile == Some(self.recommend_profile);
            self.recommend_execute_tapped = false;
            self.emit(
                "CraftEssenceEnhancement",
                &format!("打开推荐选择，设置为{}", self.recommend_profile.label()),
            );
            if !self.tap_at("CraftEssenceEnhancement", RECOMMEND_MATERIAL_BUTTON) {
                return false;
            }
            self.recommend_open_attempts = self.recommend_open_attempts.saturating_add(1);
            thread::sleep(Duration::from_millis(800));
            return true;
        }

        let ready = if template_ready {
            true
        } else {
            let button_match = match self.sidecar().find_element_by_name(
                None,
                SCREEN_NAME,
                "button_enhancement_ready",
            ) {
                Ok(result) => result,
                Err(err) => {
                    self.fail(
                        "CraftEssenceEnhancement",
                        format!("识别自动配置后的强化按钮失败: {err}"),
                    );
                    return false;
                }
            };
            let mean_luma = match self.sidecar().read_region_luma(None, ENHANCE_BUTTON_REGION) {
                Ok(value) => value,
                Err(err) => {
                    self.fail(
                        "CraftEssenceEnhancement",
                        format!("读取自动配置后的强化按钮亮度失败: {err}"),
                    );
                    return false;
                }
            };
            matches!(
                classify_enhancement_button(button_match.score, mean_luma),
                EnhancementReadyState::Ready
            )
        };

        if !ready {
            self.recommend_ready_waits = self.recommend_ready_waits.saturating_add(1);
            if self.recommend_ready_waits < RECOMMEND_READY_MAX_WAITS {
                self.emit(
                    "CraftEssenceEnhancement",
                    &format!(
                        "等待游戏用{}自动配置强化素材（{}/{RECOMMEND_READY_MAX_WAITS}）",
                        self.recommend_profile.label(),
                        self.recommend_ready_waits
                    ),
                );
                thread::sleep(Duration::from_millis(650));
                return true;
            }
            self.recommend_ready_waits = 0;
            if strategy == AutoFeedStrategy::QpEfficientPacket
                && self.recommend_profile == RecommendMaterialProfile::TwoStarOnly
            {
                self.recommend_profile = RecommendMaterialProfile::OneStarOnly;
                self.recommend_executed_for_target = false;
                self.recommend_open_attempts = 0;
                self.emit(
                    "CraftEssenceEnhancement",
                    "仅二星推荐配置没有可用素材，切换为仅一星",
                );
                return true;
            }
            self.transition(LifecycleEvent::Finished);
            self.emit(
                "CraftEssenceEnhancement",
                if strategy == AutoFeedStrategy::FastBomb {
                    "没有可用的 1 星、2 星未强化素材，快速策略结束"
                } else {
                    "一星和二星推荐素材均已耗尽，丸子制作结束"
                },
            );
            return false;
        }

        self.recommend_ready_waits = 0;
        self.materials_committed = true;
        self.emit(
            "CraftEssenceEnhancement",
            if strategy == AutoFeedStrategy::FastBomb {
                "游戏已为当前丸子自动配置 1 星、2 星未强化素材，继续强化"
            } else {
                "游戏已自动配置当前经验包素材，本经验包只执行这一次自动配置强化"
            },
        );
        if self.enhance_open_attempts >= ENHANCE_OPEN_MAX_ATTEMPTS {
            self.fail(
                "CraftEssenceEnhancement",
                "多次点击自动配置强化按钮后，确认对话框仍未打开".into(),
            );
            return false;
        }
        if !self.tap_probe_or_point(
            "CraftEssenceEnhancement",
            "button_enhancement_ready",
            ENHANCE_BUTTON,
        ) {
            return false;
        }
        self.enhance_open_attempts = self.enhance_open_attempts.saturating_add(1);
        thread::sleep(Duration::from_millis(800));
        true
    }

    pub(super) fn open_target_select(&mut self, message: &str) -> bool {
        self.emit("CraftEssenceEnhancement", message);
        self.target_tapped = false;
        self.target_return_waits = 0;
        self.target_scrolls = 0;
        self.target_scroll_reset_needed = true;
        self.target_scroll_reset_attempts = 0;
        self.target_grid_read_failures = 0;
        self.filter_two_star_enabled = None;
        self.filter_configured = false;
        if !self.tap_at("CraftEssenceEnhancement", TARGET_RESELECT_BUTTON) {
            return false;
        }
        thread::sleep(Duration::from_millis(850));
        true
    }

    pub(super) fn reset_material_selection(&mut self) {
        self.material_selected_count = 0;
        self.material_same_copy_selected = false;
        self.material_same_copy_pending = false;
        self.material_pending_counter_waits = 0;
        self.filter_two_star_enabled = None;
        self.filter_configured = false;
        self.material_seen_cells.clear();
        self.material_scrolls = 0;
        self.material_scroll_reset_needed = true;
        self.material_scroll_reset_attempts = 0;
        self.material_grid_read_failures = 0;
        self.materials_committed = false;
        self.packet_feed_remaining = if self.strategy_stage == StrategyStage::BombSelectedForFeed {
            self.packet_fingerprints.clone()
        } else {
            Vec::new()
        };
    }

    pub(super) fn handle_craft_essence_lock_mode(&mut self) -> bool {
        match self.strategy_stage {
            StrategyStage::LockBombBaseActive => {
                let Some(point) = self.lock_candidate_point else {
                    self.fail(
                        "CraftEssenceLockMode",
                        "缺少刚确认未锁定的满破底卡坐标，拒绝执行锁定".into(),
                    );
                    return false;
                };
                self.emit(
                    "CraftEssenceLockMode",
                    "只锁定刚制作且进入模式前已确认未锁定的 1 星满破底卡",
                );
                if !self.tap_at("CraftEssenceLockMode", point) {
                    return false;
                }
                self.strategy_stage = StrategyStage::VerifyBombBaseLock;
                self.target_return_waits = 0;
                thread::sleep(Duration::from_millis(650));
                true
            }
            StrategyStage::VerifyBombBaseLock => {
                let grid = match self.sidecar().read_craft_essence_grid(
                    None,
                    "enhancement_ce/item_ce_bar_bronze",
                    1920.0,
                    ITEM_GRID_REGION,
                    1.2,
                ) {
                    Ok(grid) => grid,
                    Err(err) => {
                        self.fail(
                            "CraftEssenceLockMode",
                            format!("锁定后读取礼装网格失败: {err}"),
                        );
                        return false;
                    }
                };
                if !grid.found || grid.cells.iter().any(|cell| !cell.valid) {
                    self.fail(
                        "CraftEssenceLockMode",
                        "锁定后存在无法安全识别的礼装，已停止且不会再次点击卡片".into(),
                    );
                    return false;
                }
                let candidate = grid.cells.iter().find(|cell| {
                    Some(cell.row) == self.lock_candidate_row
                        && Some(cell.col) == self.lock_candidate_col
                });
                if candidate.is_some_and(|cell| {
                    is_verified_locked_bomb_base(
                        cell,
                        self.lock_candidate_row,
                        self.lock_candidate_col,
                    )
                }) {
                    self.emit(
                        "CraftEssenceLockMode",
                        "已确认新满破底卡出现锁图标，切回选择对象模式",
                    );
                    if !self.tap_at("CraftEssenceLockMode", SELECT_OBJECT_BUTTON) {
                        return false;
                    }
                    self.strategy_stage = StrategyStage::ExitBombBaseLockMode;
                    self.target_return_waits = 0;
                    thread::sleep(Duration::from_millis(700));
                    return true;
                }
                self.target_return_waits = self.target_return_waits.saturating_add(1);
                if self.target_return_waits >= 5 {
                    self.fail(
                        "CraftEssenceLockMode",
                        "未确认新满破底卡出现锁图标；为避免反向解锁，不会再次点击".into(),
                    );
                    return false;
                }
                thread::sleep(Duration::from_millis(450));
                true
            }
            _ => {
                self.fail(
                    "CraftEssenceLockMode",
                    "检测到非预期的“统一锁定/锁定解除”操作模式。为避免改变既有锁定状态，自动化已停止；请先切回“选择对象”模式"
                        .into(),
                );
                false
            }
        }
    }
}
