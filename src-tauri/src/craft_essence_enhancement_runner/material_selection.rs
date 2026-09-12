//! Enhancement-material grid selection and paging.

use super::*;

impl CraftEssenceEnhancementRunner {
    pub(super) fn handle_strategy_material_select(&mut self) -> bool {
        if !matches!(
            self.strategy_stage,
            StrategyStage::BombBaseSelected
                | StrategyStage::PacketSelected
                | StrategyStage::BombSelectedForFeed
        ) {
            self.unexpected_material_checks = self.unexpected_material_checks.saturating_add(1);
            if self.unexpected_material_checks < GRID_READ_MAX_FAILURES {
                self.emit(
                    "MaterialSelect",
                    "页面信号与当前策略冲突，等待下一帧复核且不点击卡片",
                );
                thread::sleep(Duration::from_millis(500));
                return true;
            }
            self.fail(
                "MaterialSelect",
                format!("当前策略阶段不应进入素材列表: {:?}", self.strategy_stage),
            );
            return false;
        }
        self.unexpected_material_checks = 0;

        if !self.density_checked {
            if !self.ensure_max_density() {
                return false;
            }
            self.density_checked = true;
        }
        let displayed_selected_count = match self.read_material_selected_count() {
            Ok(count) => count,
            Err(message) => {
                self.fail("MaterialSelect", message);
                return false;
            }
        };
        if same_copy_counter_update_pending(
            self.material_same_copy_pending,
            self.material_selected_count,
            displayed_selected_count,
        ) {
            self.material_pending_counter_waits =
                self.material_pending_counter_waits.saturating_add(1);
            if self.material_pending_counter_waits < MATERIAL_PENDING_COUNTER_MAX_WAITS {
                self.emit(
                    "MaterialSelect",
                    &format!(
                        "同名副本点击后的页面计数仍为 {displayed_selected_count}/20，等待游戏更新（{}/{MATERIAL_PENDING_COUNTER_MAX_WAITS}）",
                        self.material_pending_counter_waits
                    ),
                );
                thread::sleep(Duration::from_millis(500));
                return true;
            }
            self.fail(
                "MaterialSelect",
                format!(
                    "同名副本点击后页面计数连续未更新：页面 {displayed_selected_count}/20，程序 {}/20",
                    self.material_selected_count
                ),
            );
            return false;
        }
        self.material_pending_counter_waits = 0;
        let possible_level_max = material_accepts_level_max(self.strategy_stage)
            && self.material_selected_count > displayed_selected_count
            && displayed_selected_count > 0;
        let level_max_reached = if possible_level_max {
            match self.read_material_level_max_reached() {
                Ok(reached) => reached,
                Err(message) => {
                    self.fail("MaterialSelect", message);
                    return false;
                }
            }
        } else {
            false
        };
        let counter_decision = material_counter_decision(
            self.material_selected_count,
            displayed_selected_count,
            level_max_reached,
        );
        if confirm_pending_same_copy(
            counter_decision,
            &mut self.material_same_copy_pending,
            &mut self.material_same_copy_selected,
        ) {
            self.emit("MaterialSelect", "页面计数已确认同名 1 星副本选择成功");
        }
        match counter_decision {
            MaterialCounterDecision::Confirmed => {}
            MaterialCounterDecision::ClearAutomaticSelection => {
                self.emit(
                    "MaterialSelect",
                    &format!(
                        "检测到自动配置已预选 {displayed_selected_count}/20 张素材，先清除后按丸子策略重新选择"
                    ),
                );
                if !self.tap_probe_or_point(
                    "MaterialSelect",
                    "button_enhancement_ce_clean_all_select_ready",
                    MATERIAL_CLEAR_ALL_BUTTON,
                ) {
                    return false;
                }
                thread::sleep(Duration::from_millis(700));
                return true;
            }
            MaterialCounterDecision::AcceptLevelMax => {
                self.material_selected_count = displayed_selected_count;
                if self.strategy_stage == StrategyStage::PacketSelected
                    && !self.material_same_copy_selected
                {
                    self.fail(
                        "MaterialSelect",
                        "目标虽已达到等级上限，但没有选到同名 1 星礼装，拒绝制作经验包".into(),
                    );
                    return false;
                }
                self.emit(
                    "MaterialSelect",
                    &format!(
                        "选择第 {} 张素材时目标已达到等级上限；以页面实际 {displayed_selected_count}/20 为准，点击决定",
                        self.material_selected_count.saturating_add(1)
                    ),
                );
                if !self.tap_probe_or_point(
                    "MaterialSelect",
                    "button_enhancement_ce_select_exp_decide",
                    MATERIAL_DECIDE_BUTTON,
                ) {
                    return false;
                }
                self.materials_committed = true;
                thread::sleep(Duration::from_millis(850));
                return true;
            }
            MaterialCounterDecision::Mismatch => {
                self.fail(
                    "MaterialSelect",
                    format!(
                        "页面已选 {displayed_selected_count}/20，与程序记录 {}/20 不一致，拒绝继续",
                        self.material_selected_count
                    ),
                );
                return false;
            }
        }
        let desired_two_star = false;
        if self.filter_two_star_enabled != Some(desired_two_star) {
            self.filter_two_star_desired = desired_two_star;
            self.filter_configured = false;
            self.material_scroll_reset_needed = true;
            self.material_scroll_reset_attempts = 0;
            self.emit(
                "MaterialSelect",
                "打开素材筛选，只显示 1 星以选择唯一同名副本",
            );
            if !self.tap_at("MaterialSelect", FILTER_BUTTON) {
                return false;
            }
            thread::sleep(Duration::from_millis(800));
            return true;
        }
        if !self.order_configured {
            self.emit("MaterialSelect", "打开素材排序设置");
            if !self.tap_at("MaterialSelect", ORDER_BUTTON) {
                return false;
            }
            thread::sleep(Duration::from_millis(800));
            return true;
        }
        let desired_descending = material_selection_descending(self.strategy_stage);
        let descending = self.probe("button_enhancement_ce_select_ce_desc");
        if descending != desired_descending {
            self.emit("MaterialSelect", "切换为等级升序，从顶部开始选择强化素材");
            if !self.tap_at("MaterialSelect", ORDER_DIRECTION_BUTTON) {
                return false;
            }
            self.material_scroll_reset_needed = true;
            self.material_scroll_reset_attempts = 0;
            thread::sleep(Duration::from_millis(650));
            return true;
        }

        let grid = match self.sidecar().read_craft_essence_grid(
            None,
            "enhancement_ce/item_ce_bar_bronze",
            1920.0,
            ITEM_GRID_REGION,
            1.2,
        ) {
            Ok(grid) => grid,
            Err(err) => {
                self.fail("MaterialSelect", format!("读取礼装素材列表失败: {err}"));
                return false;
            }
        };
        if self.material_scroll_reset_needed {
            match scrollbar_reset_drag_y(
                grid.diagnostics.scrollbar_thumb_y,
                grid.diagnostics.scrollbar_thumb_top_y,
                grid.diagnostics.visible_cell_count,
                grid.diagnostics.grid_cell_count,
                self.material_scroll_reset_attempts,
            ) {
                Ok(None) => {
                    self.material_scroll_reset_needed = false;
                    self.material_scroll_reset_attempts = 0;
                    self.material_scrolls = 0;
                }
                Ok(Some(thumb_y)) => {
                    self.material_scroll_reset_attempts =
                        self.material_scroll_reset_attempts.saturating_add(1);
                    self.emit("MaterialSelect", "将素材列表复位到顶部");
                    if !self.swipe_at(
                        "MaterialSelect",
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
                    self.fail("MaterialSelect", format!("{message}，拒绝继续选择素材"));
                    return false;
                }
            }
        }

        if !grid.found || grid.cells.iter().any(|cell| !cell.valid) {
            self.material_grid_read_failures = self.material_grid_read_failures.saturating_add(1);
            if self.material_grid_read_failures < GRID_READ_MAX_FAILURES {
                self.emit(
                    "MaterialSelect",
                    &format!(
                        "素材列表识别不稳定（无效 {} 张），原地重试",
                        grid.diagnostics.invalid_cell_count
                    ),
                );
                thread::sleep(Duration::from_millis(450));
                return true;
            }
            self.fail(
                "MaterialSelect",
                format!(
                    "素材列表存在无法安全识别的礼装（无效 {} 张）",
                    grid.diagnostics.invalid_cell_count
                ),
            );
            return false;
        }
        self.material_grid_read_failures = 0;

        if self.mode == CraftEssenceEnhancementMode::QpEfficient
            && self.strategy_stage == StrategyStage::SelectBomb
            && !self.initial_bombs_counted
        {
            self.completed_bombs = u8::try_from(
                grid.cells
                    .iter()
                    .filter(|cell| is_complete_locked_bomb(cell))
                    .count(),
            )
            .unwrap_or(TARGET_BOMB_COUNT)
            .min(TARGET_BOMB_COUNT);
            self.initial_bombs_counted = true;
            if self.completed_bombs >= TARGET_BOMB_COUNT {
                self.strategy_stage = StrategyStage::QpEfficientComplete;
                self.transition(LifecycleEvent::Finished);
                self.emit(
                    "QpEfficientComplete",
                    "已识别到 8 个锁定的 50 级丸子，节省 QP 策略完成；程序不会解锁任何礼装",
                );
                return false;
            }
            if self.completed_bombs > 0 {
                self.emit(
                    "CraftEssenceSelect",
                    &format!(
                        "已识别到 {} 个现成的 50 级丸子，继续补足到 {} 个",
                        self.completed_bombs, TARGET_BOMB_COUNT
                    ),
                );
            }
        }

        let signature = grid_signature(&grid.cells);
        let mut choices: Vec<&CraftEssenceGridCell> = Vec::new();
        if self.strategy_stage == StrategyStage::BombBaseSelected {
            for cell in &grid.cells {
                if self.material_selected_count + u8::try_from(choices.len()).unwrap_or(4) >= 4 {
                    break;
                }
                if !is_raw_food(cell) || !cell.same_as_target {
                    continue;
                }
                let key = format!("{signature}:{}:{}", cell.row, cell.col);
                if !self.material_seen_cells.contains(&key) {
                    choices.push(cell);
                }
            }
        } else if self.strategy_stage == StrategyStage::PacketSelected {
            let available = grid
                .cells
                .iter()
                .filter(|cell| {
                    let key = format!("{signature}:{}:{}", cell.row, cell.col);
                    !self.material_seen_cells.contains(&key)
                })
                .collect::<Vec<_>>();
            choices = plan_packet_materials(&available, self.material_same_copy_selected);
        } else if self.strategy_stage == StrategyStage::BombSelectedForFeed {
            let available = grid
                .cells
                .iter()
                .filter(|cell| {
                    let key = format!("{signature}:{}:{}", cell.row, cell.col);
                    !self.material_seen_cells.contains(&key)
                })
                .collect::<Vec<_>>();
            choices = if self.feed_inventory_packets {
                plan_inventory_packet_feed(&available)
            } else {
                plan_packet_feed(&available, &self.packet_feed_remaining)
            };
        }

        let required = match self.strategy_stage {
            StrategyStage::BombBaseSelected => 4,
            StrategyStage::PacketSelected => 1,
            StrategyStage::BombSelectedForFeed => {
                if self.feed_inventory_packets {
                    PACKET_BATCH_SIZE
                } else {
                    u8::try_from(self.packet_fingerprints.len()).unwrap_or(PACKET_BATCH_SIZE)
                }
            }
            _ => unreachable!(),
        };
        // Use one CV/grid read to click the current page's whole safe batch, then
        // reconcile against the game's displayed N/20 counter on the next tick.
        // The single-copy packet break stays capped at exactly one material.
        choices.truncate(material_batch_click_limit(
            self.strategy_stage,
            self.material_selected_count,
            required,
        ));
        let material_tapped = !choices.is_empty();
        for cell in &choices {
            let key = format!("{signature}:{}:{}", cell.row, cell.col);
            self.material_seen_cells.insert(key);
            let point = Point::new(
                cell.region.x + cell.region.w / 2.0,
                cell.region.y + cell.region.h / 2.0,
            );
            if !self.tap_at("MaterialSelect", point) {
                return false;
            }
            if self.strategy_stage == StrategyStage::PacketSelected
                && !self.material_same_copy_selected
                && cell.same_as_target
            {
                self.material_same_copy_pending = true;
            }
            if self.strategy_stage == StrategyStage::BombSelectedForFeed {
                if !self.feed_inventory_packets {
                    let Some(index) = self.packet_feed_remaining.iter().position(|fingerprint| {
                        same_art_fingerprint(fingerprint, &cell.art_fingerprint)
                    }) else {
                        self.fail(
                            "MaterialSelect",
                            "经验包批次记录与待选素材不一致，已停止".into(),
                        );
                        return false;
                    };
                    self.packet_feed_remaining.remove(index);
                }
            }
            self.material_selected_count = self.material_selected_count.saturating_add(1);
            thread::sleep(Duration::from_millis(180));
        }

        if self.material_same_copy_pending {
            self.emit(
                "MaterialSelect",
                "已点击同名 1 星副本，等待下一帧页面计数确认",
            );
            return true;
        }

        if material_selection_ready_to_commit(
            self.material_selected_count,
            required,
            self.material_same_copy_pending,
        ) {
            let displayed_selected_count = match self.read_material_selected_count() {
                Ok(count) => count,
                Err(message) => {
                    self.fail("MaterialSelect", message);
                    return false;
                }
            };
            if displayed_selected_count != required {
                self.fail(
                    "MaterialSelect",
                    format!(
                        "点击决定前页面显示已选 {displayed_selected_count}/20，预期 {required}/20，拒绝继续"
                    ),
                );
                return false;
            }
            if self.strategy_stage == StrategyStage::PacketSelected
                && !self.material_same_copy_selected
            {
                self.fail(
                    "MaterialSelect",
                    "没有选到同名 1 星礼装，拒绝制作经验包".into(),
                );
                return false;
            }
            self.emit(
                "MaterialSelect",
                &format!("已安全选择 {required} 张素材，点击决定"),
            );
            if !self.tap_probe_or_point(
                "MaterialSelect",
                "button_enhancement_ce_select_exp_decide",
                MATERIAL_DECIDE_BUTTON,
            ) {
                return false;
            }
            self.materials_committed = true;
            thread::sleep(Duration::from_millis(850));
            return true;
        }
        if material_tapped {
            return true;
        }

        let material_at_bottom = self.probe("element_enhancement_ce_scroll_end");
        if inventory_feed_ready_at_bottom(
            self.feed_inventory_packets,
            self.material_selected_count,
            material_at_bottom,
        ) {
            let displayed_selected_count = match self.read_material_selected_count() {
                Ok(count) => count,
                Err(message) => {
                    self.fail("MaterialSelect", message);
                    return false;
                }
            };
            if displayed_selected_count != self.material_selected_count {
                self.fail(
                    "MaterialSelect",
                    format!(
                        "库存经验包选择结束时页面显示已选 {displayed_selected_count}/20，程序记录 {}/20，拒绝继续",
                        self.material_selected_count
                    ),
                );
                return false;
            }
            self.emit(
                "MaterialSelect",
                &format!(
                    "已到列表底部，安全选择了 {displayed_selected_count} 张未锁定、已升级的 1 星礼装，点击决定"
                ),
            );
            if !self.tap_probe_or_point(
                "MaterialSelect",
                "button_enhancement_ce_select_exp_decide",
                MATERIAL_DECIDE_BUTTON,
            ) {
                return false;
            }
            self.materials_committed = true;
            thread::sleep(Duration::from_millis(850));
            return true;
        }
        self.material_scrolls = next_page_scan_count(
            self.material_scrolls,
            MATERIAL_PAGE_MAX_SCROLLS,
            material_at_bottom,
        );
        if self.material_scrolls > MATERIAL_PAGE_MAX_SCROLLS {
            if material_at_bottom {
                self.emit("MaterialSelect", "已识别到礼装列表底部，不再继续滚动");
            }
            if self.strategy_stage == StrategyStage::BombSelectedForFeed
                && self.feed_inventory_packets
                && self.material_selected_count == 0
            {
                self.transition(LifecycleEvent::Finished);
                self.emit(
                    "MaterialSelect",
                    "没有可用的未锁定、已升级 1 星礼装；一星和二星素材均已消耗完",
                );
                return false;
            }
            self.fail(
                "MaterialSelect",
                match self.strategy_stage {
                    StrategyStage::BombBaseSelected => format!(
                        "只找到 {} / 4 张同名未锁定 1 星礼装，无法制作满破底卡",
                        self.material_selected_count
                    ),
                    StrategyStage::PacketSelected => format!(
                        "找不到同名未锁定 1 星副本：已选 {} / 1",
                        self.material_selected_count
                    ),
                    StrategyStage::BombSelectedForFeed => {
                        if self.feed_inventory_packets {
                            format!(
                                "只找到 {} 个未锁定、已升级的 1 星经验包",
                                self.material_selected_count
                            )
                        } else {
                            format!(
                                "只找到 {} / {} 个本批刚制作的至少 1 破 1 星经验包",
                                self.material_selected_count,
                                self.packet_fingerprints.len()
                            )
                        }
                    }
                    _ => unreachable!(),
                },
            );
            return false;
        }
        self.emit(
            "MaterialSelect",
            &format!(
                "当前已选 {} / {required}，向下查找更多素材",
                self.material_selected_count
            ),
        );
        if !self.swipe_at("MaterialSelect", LIST_SWIPE_FROM, LIST_SWIPE_TO, 520) {
            return false;
        }
        thread::sleep(Duration::from_millis(700));
        true
    }
}
