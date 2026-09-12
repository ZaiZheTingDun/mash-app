//! Craft essence target-list navigation and target selection.

use super::*;

impl CraftEssenceEnhancementRunner {
    pub(super) fn handle_craft_essence_select(&mut self, descending: bool) -> bool {
        if self.target_tapped {
            self.target_return_waits += 1;
            if self.target_return_waits >= 6 {
                self.fail(
                    "CraftEssenceSelect",
                    "点击第一张概念礼装后未返回强化页面".into(),
                );
                return false;
            }
            thread::sleep(Duration::from_millis(700));
            return true;
        }
        if !self.density_checked {
            if !self.ensure_max_density() {
                return false;
            }
            self.density_checked = true;
        }
        if self.filter_two_star_enabled != Some(false) {
            self.filter_two_star_desired = false;
            self.filter_configured = false;
            self.target_scroll_reset_needed = true;
            self.target_scroll_reset_attempts = 0;
            self.emit("CraftEssenceSelect", "打开概念礼装筛选，只显示 1 星礼装");
            if self.tap_at("CraftEssenceSelect", FILTER_BUTTON) {
                thread::sleep(Duration::from_millis(800));
                return true;
            }
            return false;
        }
        if !self.order_configured {
            self.emit("CraftEssenceSelect", "打开概念礼装排序设置");
            if self.tap_at("CraftEssenceSelect", ORDER_BUTTON) {
                thread::sleep(Duration::from_millis(800));
                return true;
            }
            return false;
        }
        let desired_descending = target_selection_descending(self.strategy_stage);
        if descending != desired_descending {
            self.emit(
                "CraftEssenceSelect",
                if desired_descending {
                    "切换为等级降序，查找丸子"
                } else {
                    "切换为等级升序，查找 1 星经验包底卡"
                },
            );
            if self.tap_at("CraftEssenceSelect", ORDER_DIRECTION_BUTTON) {
                self.target_scroll_reset_needed = true;
                self.target_scroll_reset_attempts = 0;
                thread::sleep(Duration::from_millis(650));
                return true;
            }
            return false;
        }
        self.descending_checked = desired_descending;
        self.emit("CraftEssenceSelect", "读取礼装等级、上限、突破和锁定状态");
        let grid = match self.sidecar().read_craft_essence_grid(
            None,
            "enhancement_ce/item_ce_bar_bronze",
            1920.0,
            ITEM_GRID_REGION,
            1.2,
        ) {
            Ok(grid) => grid,
            Err(err) => {
                self.fail("CraftEssenceSelect", format!("概念礼装网格识别失败: {err}"));
                return false;
            }
        };
        if self.target_scroll_reset_needed {
            match scrollbar_reset_drag_y(
                grid.diagnostics.scrollbar_thumb_y,
                grid.diagnostics.scrollbar_thumb_top_y,
                grid.diagnostics.visible_cell_count,
                grid.diagnostics.grid_cell_count,
                self.target_scroll_reset_attempts,
            ) {
                Ok(None) => {
                    self.target_scroll_reset_needed = false;
                    self.target_scroll_reset_attempts = 0;
                    self.target_scrolls = 0;
                }
                Ok(Some(thumb_y)) => {
                    self.target_scroll_reset_attempts =
                        self.target_scroll_reset_attempts.saturating_add(1);
                    self.emit("CraftEssenceSelect", "将礼装列表复位到顶部");
                    if !self.swipe_at(
                        "CraftEssenceSelect",
                        Point::new(LIST_SCROLLBAR_X, thumb_y),
                        Point::new(LIST_SCROLLBAR_X, LIST_SCROLLBAR_OVERSHOOT_Y),
                        520,
                    ) {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(700));
                    return true;
                }
                Err(message) => {
                    self.fail("CraftEssenceSelect", format!("{message}，拒绝继续选择礼装"));
                    return false;
                }
            }
        }

        if !grid.found || grid.cells.iter().any(|cell| !cell.valid) {
            self.target_grid_read_failures = self.target_grid_read_failures.saturating_add(1);
            if self.target_grid_read_failures < GRID_READ_MAX_FAILURES {
                self.emit(
                    "CraftEssenceSelect",
                    &format!(
                        "礼装列表识别不稳定（无效 {} 张），原地重试",
                        grid.diagnostics.invalid_cell_count
                    ),
                );
                thread::sleep(Duration::from_millis(450));
                return true;
            }
            self.fail(
                "CraftEssenceSelect",
                format!(
                    "礼装列表存在无法安全识别的卡片（无效 {} 张）",
                    grid.diagnostics.invalid_cell_count
                ),
            );
            return false;
        }
        self.target_grid_read_failures = 0;

        if self.strategy_stage == StrategyStage::InspectBomb {
            let inspected = grid
                .cells
                .iter()
                .filter(|cell| {
                    cell.valid
                        && cell.locked
                        && cell.rarity == Some(1)
                        && cell.level_cap == Some(50)
                        && same_art_fingerprint(
                            &cell.art_fingerprint,
                            &self.current_bomb_fingerprint,
                        )
                        && cell
                            .level
                            .is_some_and(|level| level >= self.current_bomb_level)
                })
                .max_by_key(|cell| cell.level.unwrap_or(0));
            if let Some(cell) = inspected {
                self.current_bomb_level = cell.level.unwrap_or(0);
                self.target_scrolls = 0;
                if self.current_bomb_level >= 50 {
                    self.completed_bombs = self.completed_bombs.saturating_add(1);
                    self.emit(
                        "CraftEssenceSelect",
                        &format!(
                            "第 {} / {} 个丸子已达到 50 级",
                            self.completed_bombs, TARGET_BOMB_COUNT
                        ),
                    );
                    if self.completed_bombs >= TARGET_BOMB_COUNT {
                        self.strategy_stage = StrategyStage::QpEfficientComplete;
                        self.transition(LifecycleEvent::Finished);
                        self.emit(
                            "QpEfficientComplete",
                            "8 个丸子已完成，节省 QP 策略结束；程序不会解锁任何礼装",
                        );
                        return false;
                    }
                    self.current_bomb_fingerprint.clear();
                    self.current_bomb_level = 0;
                    self.strategy_stage = StrategyStage::SelectBomb;
                } else {
                    self.strategy_stage = StrategyStage::SelectPacketBase;
                }
                thread::sleep(Duration::from_millis(250));
                return true;
            }
        }

        let candidate = match self.strategy_stage {
            StrategyStage::SelectBomb => choose_incomplete_bomb(&grid.cells),
            StrategyStage::SelectBombBase => choose_bomb_base(&grid.cells),
            StrategyStage::FindBombBaseToLock => grid.cells.iter().find(|cell| {
                cell.valid
                    && !cell.locked
                    && cell.rarity == Some(1)
                    && cell.level_cap == Some(50)
                    && cell.limit_breaks == Some(4)
                    && same_art_fingerprint(&cell.art_fingerprint, &self.current_bomb_fingerprint)
            }),
            StrategyStage::ExitBombBaseLockMode => grid.cells.iter().find(|cell| {
                is_incomplete_locked_bomb(cell)
                    && same_art_fingerprint(&cell.art_fingerprint, &self.current_bomb_fingerprint)
            }),
            StrategyStage::SelectPacketBase => choose_packet_base(&grid.cells),
            StrategyStage::SelectBombForTransfer => grid
                .cells
                .iter()
                .filter(|cell| {
                    is_incomplete_locked_bomb(cell)
                        && same_art_fingerprint(
                            &cell.art_fingerprint,
                            &self.current_bomb_fingerprint,
                        )
                        && cell
                            .level
                            .is_some_and(|level| level >= self.current_bomb_level)
                })
                .max_by_key(|cell| cell.level.unwrap_or(0)),
            _ => None,
        };

        let Some(candidate) = candidate else {
            let target_at_bottom = self.probe("element_enhancement_ce_scroll_end");
            self.target_scrolls = next_page_scan_count(
                self.target_scrolls,
                TARGET_PAGE_MAX_SCROLLS,
                target_at_bottom,
            );
            if self.target_scrolls > TARGET_PAGE_MAX_SCROLLS {
                if target_at_bottom {
                    self.emit("CraftEssenceSelect", "已识别到礼装列表底部，不再继续滚动");
                }
                if self.strategy_stage == StrategyStage::SelectBomb {
                    self.emit(
                        "CraftEssenceSelect",
                        "没有更多已锁定的未满级丸子底卡，开始制作新的 1 星满破底卡",
                    );
                    self.strategy_stage = stage_after_missing_bomb();
                    self.target_scrolls = 0;
                    self.target_scroll_reset_needed = true;
                    self.target_scroll_reset_attempts = 0;
                    return true;
                }
                if self.strategy_stage == StrategyStage::SelectPacketBase {
                    self.feed_inventory_packets = self.packet_fingerprints.is_empty();
                    let message = if self.feed_inventory_packets {
                        "没有更多同名 1/10 底卡，改用库存中未锁定、已升级的 1 星礼装强化丸子"
                            .to_string()
                    } else {
                        format!(
                            "本批已制作 {} 个经验包，未找到更多底卡，开始向丸子转移",
                            self.packet_fingerprints.len()
                        )
                    };
                    self.emit("CraftEssenceSelect", &message);
                    if packet_base_exhaustion_action(self.packet_fingerprints.len())
                        == PacketBaseExhaustionAction::ReturnToCurrentBomb
                    {
                        self.strategy_stage = StrategyStage::BombSelectedForFeed;
                        if !self.tap_at("CraftEssenceSelect", TARGET_LIST_CLOSE_BUTTON) {
                            return false;
                        }
                        thread::sleep(Duration::from_millis(850));
                        return true;
                    }
                    self.strategy_stage = StrategyStage::SelectBombForTransfer;
                    self.target_scrolls = 0;
                    self.target_scroll_reset_needed = true;
                    self.target_scroll_reset_attempts = 0;
                    return true;
                }
                self.fail(
                    "CraftEssenceSelect",
                    match self.strategy_stage {
                        StrategyStage::SelectBomb => {
                            "没有找到锁定、满破、未满 50 级的 1 星丸子底卡".into()
                        }
                        StrategyStage::SelectBombBase => {
                            "没有找到至少 5 张同名的未锁定 1 星 1 级礼装，无法制作新的满破底卡"
                                .into()
                        }
                        StrategyStage::FindBombBaseToLock => {
                            "无法重新找到刚制作的未锁定 1 星满破底卡".into()
                        }
                        StrategyStage::ExitBombBaseLockMode => {
                            "退出锁定模式后未确认刚制作的底卡已上锁".into()
                        }
                        StrategyStage::SelectBombForTransfer | StrategyStage::InspectBomb => {
                            "无法重新找到当前锁定的 1 星丸子，已停止以避免选错".into()
                        }
                        _ => "当前策略阶段无法选择礼装".into(),
                    },
                );
                return false;
            }
            self.emit("CraftEssenceSelect", "当前页没有合适目标，继续向下查找");
            if !self.swipe_at("CraftEssenceSelect", LIST_SWIPE_FROM, LIST_SWIPE_TO, 520) {
                return false;
            }
            thread::sleep(Duration::from_millis(700));
            return true;
        };

        let point = Point::new(
            candidate.region.x + candidate.region.w / 2.0,
            candidate.region.y + candidate.region.h / 2.0,
        );
        match self.strategy_stage {
            StrategyStage::SelectBomb => {
                self.current_bomb_fingerprint = candidate.art_fingerprint.clone();
                self.current_bomb_level = candidate.level.unwrap_or(0);
                self.enter_bomb_enhancement_strategy();
                self.emit(
                    "CraftEssenceSelect",
                    &format!(
                        "选择锁定的 1 星满破丸子（{}/50）{}",
                        self.current_bomb_level,
                        if self.mode == CraftEssenceEnhancementMode::Fast {
                            "，准备使用快速策略"
                        } else {
                            ""
                        }
                    ),
                );
            }
            StrategyStage::SelectBombBase => {
                self.current_bomb_fingerprint = candidate.art_fingerprint.clone();
                self.current_bomb_level = candidate.level.unwrap_or(1);
                self.strategy_stage = StrategyStage::BombBaseSelected;
                self.emit(
                    "CraftEssenceSelect",
                    "选择有 4 张以上同名副本的未锁定 1 星底卡",
                );
            }
            StrategyStage::FindBombBaseToLock => {
                self.lock_candidate_row = Some(candidate.row);
                self.lock_candidate_col = Some(candidate.col);
                self.lock_candidate_point = Some(point);
                self.strategy_stage = StrategyStage::LockBombBaseActive;
                self.target_return_waits = 0;
                self.emit(
                    "CraftEssenceSelect",
                    "已确认刚制作的满破底卡仍未锁定，进入统一锁定模式",
                );
                if !self.tap_at("CraftEssenceSelect", UNIFIED_LOCK_BUTTON) {
                    return false;
                }
                thread::sleep(Duration::from_millis(700));
                return true;
            }
            StrategyStage::ExitBombBaseLockMode => {
                self.current_bomb_level = candidate.level.unwrap_or(0);
                self.enter_bomb_enhancement_strategy();
                self.emit(
                    "CraftEssenceSelect",
                    &format!(
                        "已确认新底卡上锁，选择为当前丸子（{}/50）",
                        self.current_bomb_level
                    ),
                );
            }
            StrategyStage::SelectPacketBase => {
                self.packet_fingerprint = candidate.art_fingerprint.clone();
                self.strategy_stage = StrategyStage::PacketSelected;
                self.emit("CraftEssenceSelect", "选择未锁定的 1 星 1 级经验包底卡");
            }
            StrategyStage::SelectBombForTransfer => {
                self.current_bomb_level = candidate.level.unwrap_or(self.current_bomb_level);
                self.strategy_stage = StrategyStage::BombSelectedForFeed;
                self.emit(
                    "CraftEssenceSelect",
                    &format!("重新选择当前丸子（{}/50）", self.current_bomb_level),
                );
            }
            _ => unreachable!(),
        }
        self.target_scrolls = 0;
        if self.tap_at("CraftEssenceSelect", point) {
            self.target_tapped = true;
            thread::sleep(Duration::from_millis(900));
            return true;
        }
        false
    }
}
