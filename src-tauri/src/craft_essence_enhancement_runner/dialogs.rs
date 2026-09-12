//! Filter, recommendation, ordering, and density dialog handling.

use super::*;

impl CraftEssenceEnhancementRunner {
    pub(super) fn enter_bomb_enhancement_strategy(&mut self) {
        if self.mode == CraftEssenceEnhancementMode::Fast {
            self.recommend_profile = RecommendMaterialProfile::OneAndTwoStar;
            self.recommend_configured_profile = None;
            self.recommend_reset_done = false;
            self.recommend_executed_for_target = false;
            self.recommend_execute_tapped = false;
            self.recommend_ready_waits = 0;
            self.recommend_open_attempts = 0;
        }
        self.strategy_stage = stage_after_bomb_selection(self.mode);
    }

    pub(super) fn handle_filter_dialog(&mut self) -> bool {
        if !self.filter_scroll_reset_done {
            let thumb = match self.sidecar().find_element_by_name(
                None,
                SCREEN_NAME,
                "scroll_bar_enhancement_filter",
            ) {
                Ok(result) if result.found => Some(Point::new(result.x, result.y)),
                Ok(_) => None,
                Err(err) => {
                    self.fail("FilterDialog", format!("识别礼装筛选列表滚动条失败: {err}"));
                    return false;
                }
            }
            .filter(|point| {
                point.x.is_finite()
                    && point.y.is_finite()
                    && (0.0..=1.0).contains(&point.x)
                    && (0.0..=1.0).contains(&point.y)
            });
            let Some(from) = thumb else {
                self.fail("FilterDialog", "未识别到礼装筛选列表滚动条位置".into());
                return false;
            };
            let needs_drag = match filter_scrollbar_reset_needs_drag(
                from.y,
                self.filter_scroll_reset_attempts,
            ) {
                Ok(needs_drag) => needs_drag,
                Err(message) => {
                    self.fail("FilterDialog", message.into());
                    return false;
                }
            };
            if !needs_drag {
                self.filter_scroll_reset_done = true;
                self.emit("FilterDialog", "筛选列表滚动条已在顶部");
            } else {
                let message = if self.filter_scroll_reset_attempts == 0 {
                    "先将筛选列表滚动条拖到最顶端"
                } else {
                    "拖动后仍未到顶，再拖动一次"
                };
                self.emit("FilterDialog", message);
                self.filter_scroll_reset_attempts += 1;
                if !self.swipe_at("FilterDialog", from, FILTER_SCROLLBAR_TOP, 250) {
                    return false;
                }
                thread::sleep(Duration::from_millis(350));
                return true;
            }
        }

        if self.filter_configured {
            if self.tap_at("FilterDialog", FILTER_CONFIRM_BUTTON) {
                thread::sleep(Duration::from_millis(700));
                return true;
            }
            return false;
        }
        if !self.filter_reset_done {
            self.emit("FilterDialog", "恢复筛选初始设置");
            if self.tap_probe_or_point(
                "FilterDialog",
                "button_enhancement_ce_filter_init",
                Point::new(0.175, 0.881),
            ) {
                self.filter_reset_done = true;
                thread::sleep(Duration::from_millis(600));
                return true;
            }
            return false;
        }

        for filter in RARITY_FILTERS {
            let target_on = if filter.rarity == 2 {
                self.filter_two_star_desired
            } else {
                filter.target_on
            };
            let mean_luma = match self.sidecar().read_region_luma(None, filter.region) {
                Ok(mean_luma) => mean_luma,
                Err(err) => {
                    self.fail(
                        "FilterDialog",
                        format!("读取 {} 星筛选按钮颜色失败: {err}", filter.rarity),
                    );
                    return false;
                }
            };
            let current_state = classify_filter_toggle_luma(mean_luma);
            match current_state {
                FilterToggleState::On if target_on => continue,
                FilterToggleState::Off if !target_on => continue,
                FilterToggleState::On | FilterToggleState::Off => {
                    self.emit(
                        "FilterDialog",
                        &format!("调整 {} 星筛选状态", filter.rarity),
                    );
                    if self.tap_at("FilterDialog", filter.center()) {
                        thread::sleep(Duration::from_millis(450));
                        return true;
                    }
                    return false;
                }
                FilterToggleState::Ambiguous => {
                    self.fail(
                        "FilterDialog",
                        format!(
                            "无法明确识别 {} 星筛选开关颜色（平均亮度 {:.1}）",
                            filter.rarity, mean_luma
                        ),
                    );
                    return false;
                }
            }
        }

        self.filter_configured = true;
        self.filter_two_star_enabled = Some(self.filter_two_star_desired);
        self.emit("FilterDialog", "筛选状态已确认，保存设置");
        if self.tap_at("FilterDialog", FILTER_CONFIRM_BUTTON) {
            thread::sleep(Duration::from_millis(800));
            return true;
        }
        false
    }

    pub(super) fn handle_recommend_material_empty_dialog(&mut self) -> bool {
        if !matches!(
            self.strategy_stage,
            StrategyStage::PacketAutoFeedPending | StrategyStage::FastAutoFeedPending
        ) {
            self.fail(
                "RecommendMaterialEmptyDialog",
                format!(
                    "当前策略阶段不允许处理推荐素材耗尽提示: {:?}",
                    self.strategy_stage
                ),
            );
            return false;
        }
        let exhausted_profile = self.recommend_profile;
        self.emit(
            "RecommendMaterialEmptyDialog",
            &format!("游戏确认没有可用的{}，关闭提示", exhausted_profile.label()),
        );
        if !self.tap_at("RecommendMaterialEmptyDialog", RECOMMEND_EMPTY_CLOSE_BUTTON) {
            return false;
        }
        self.recommend_execute_tapped = false;
        self.recommend_executed_for_target = false;
        self.recommend_ready_waits = 0;
        self.recommend_open_attempts = 0;
        thread::sleep(Duration::from_millis(800));

        if self.strategy_stage == StrategyStage::PacketAutoFeedPending
            && exhausted_profile == RecommendMaterialProfile::TwoStarOnly
        {
            self.recommend_profile = RecommendMaterialProfile::OneStarOnly;
            self.recommend_reset_done = false;
            self.emit(
                "RecommendMaterialEmptyDialog",
                "二星素材已耗尽，改为仅使用一星未强化礼装",
            );
            return true;
        }

        self.transition(LifecycleEvent::Finished);
        self.emit(
            "RecommendMaterialEmptyDialog",
            if self.strategy_stage == StrategyStage::FastAutoFeedPending {
                "没有可用的 1 星、2 星未强化素材，快速策略结束"
            } else {
                "一星和二星推荐素材均已耗尽，丸子制作结束"
            },
        );
        false
    }

    pub(super) fn handle_recommend_material_dialog(&mut self) -> bool {
        if !matches!(
            self.strategy_stage,
            StrategyStage::PacketAutoFeedPending | StrategyStage::FastAutoFeedPending
        ) {
            self.fail(
                "RecommendMaterialDialog",
                format!("当前策略阶段不允许使用推荐选择: {:?}", self.strategy_stage),
            );
            return false;
        }
        if self.recommend_execute_tapped {
            self.recommend_ready_waits = self.recommend_ready_waits.saturating_add(1);
            if self.recommend_ready_waits >= RECOMMEND_READY_MAX_WAITS {
                self.fail(
                    "RecommendMaterialDialog",
                    "点击执行后推荐素材对话框仍未关闭".into(),
                );
                return false;
            }
            thread::sleep(Duration::from_millis(700));
            return true;
        }

        if !self.recommend_reset_done {
            self.emit("RecommendMaterialDialog", "初始化推荐素材筛选");
            if self.tap_at("RecommendMaterialDialog", RECOMMEND_INIT_BUTTON) {
                self.recommend_reset_done = true;
                thread::sleep(Duration::from_millis(600));
                return true;
            }
            return false;
        }

        for filter in RECOMMEND_FILTERS {
            let target_on = filter.target_on(self.recommend_profile);
            let mean_luma = match self.sidecar().read_region_luma(None, filter.region) {
                Ok(mean_luma) => mean_luma,
                Err(err) => {
                    self.fail(
                        "RecommendMaterialDialog",
                        format!("读取推荐素材“{}”颜色失败: {err}", filter.label),
                    );
                    return false;
                }
            };
            let current_state = classify_filter_toggle_luma(mean_luma);
            match current_state {
                FilterToggleState::On if target_on => continue,
                FilterToggleState::Off if !target_on => continue,
                FilterToggleState::On | FilterToggleState::Off => {
                    self.emit(
                        "RecommendMaterialDialog",
                        &format!("调整推荐素材“{}”筛选状态", filter.label),
                    );
                    if self.tap_at("RecommendMaterialDialog", filter.center()) {
                        thread::sleep(Duration::from_millis(450));
                        return true;
                    }
                    return false;
                }
                FilterToggleState::Ambiguous => {
                    self.fail(
                        "RecommendMaterialDialog",
                        format!(
                            "无法明确识别推荐素材“{}”开关颜色（平均亮度 {:.1}）",
                            filter.label, mean_luma
                        ),
                    );
                    return false;
                }
            }
        }

        let auto_color = match self
            .sidecar()
            .read_region_color(None, RECOMMEND_AUTO_CONFIG_REGION)
        {
            Ok(color) => color,
            Err(err) => {
                self.fail(
                    "RecommendMaterialDialog",
                    format!("读取自动配置开关颜色失败: {err}"),
                );
                return false;
            }
        };
        match classify_auto_config_saturation(auto_color.mean_saturation) {
            AutoConfigState::Off => {
                self.emit("RecommendMaterialDialog", "开启自动配置");
                if self.tap_at("RecommendMaterialDialog", RECOMMEND_AUTO_CONFIG_BUTTON) {
                    thread::sleep(Duration::from_millis(500));
                    return true;
                }
                false
            }
            AutoConfigState::On => {
                self.emit(
                    "RecommendMaterialDialog",
                    &format!("执行推荐素材选择：{}", self.recommend_profile.label()),
                );
                if self.tap_at("RecommendMaterialDialog", RECOMMEND_EXECUTE_BUTTON) {
                    self.recommend_execute_tapped = true;
                    self.recommend_executed_for_target = true;
                    self.recommend_configured_profile = Some(self.recommend_profile);
                    self.recommend_open_attempts = 0;
                    self.recommend_ready_waits = 0;
                    thread::sleep(Duration::from_millis(900));
                    return true;
                }
                false
            }
            AutoConfigState::Ambiguous => {
                self.fail(
                    "RecommendMaterialDialog",
                    format!(
                        "无法明确识别自动配置开关颜色（平均饱和度 {:.1}）",
                        auto_color.mean_saturation
                    ),
                );
                false
            }
        }
    }

    pub(super) fn handle_order_dialog(&mut self) -> bool {
        if self.order_configured {
            if self.tap_at("OrderDialog", ORDER_CONFIRM_BUTTON) {
                thread::sleep(Duration::from_millis(700));
                return true;
            }
            return false;
        }
        if !self.order_level_selected {
            self.emit("OrderDialog", "设置为等级顺序");
            if self.tap_at("OrderDialog", ORDER_LEVEL_BUTTON) {
                self.order_level_selected = true;
                thread::sleep(Duration::from_millis(450));
                return true;
            }
            return false;
        }

        let on = self.probe_score("toggle_enhancement_ce_intelligent_order_on");
        let off = self.probe_score("toggle_enhancement_ce_intelligent_order_off");
        match decide_binary_control(on, off) {
            BinaryControlDecision::TargetConfirmed => {
                self.order_configured = true;
                self.emit("OrderDialog", "智能排序已开启，保存设置");
                if self.tap_at("OrderDialog", ORDER_CONFIRM_BUTTON) {
                    thread::sleep(Duration::from_millis(800));
                    return true;
                }
                false
            }
            BinaryControlDecision::Toggle => {
                self.emit("OrderDialog", "开启智能排序");
                if self.tap_probe_or_point(
                    "OrderDialog",
                    "toggle_enhancement_ce_intelligent_order_off",
                    Point::new(0.451, 0.658),
                ) {
                    thread::sleep(Duration::from_millis(500));
                    return true;
                }
                false
            }
            BinaryControlDecision::Ambiguous => {
                self.fail(
                    "OrderDialog",
                    format!("无法明确识别智能排序开关状态（开启分数 {on:.3}，关闭分数 {off:.3}）"),
                );
                false
            }
        }
    }

    pub(super) fn ensure_max_density(&mut self) -> bool {
        self.emit("CraftEssenceSelect", "确认一屏最多显示模式");
        for taps_done in 0..=3 {
            match density_decision(self.probe("button_scale_level_3"), taps_done) {
                DensityDecision::Confirmed => return true,
                DensityDecision::Toggle => {
                    if !self.tap_at("CraftEssenceSelect", GRID_DENSITY_BUTTON) {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(500));
                }
                DensityDecision::Failed => {
                    self.fail(
                        "CraftEssenceSelect",
                        "无法切换到一屏最多显示模式，button_scale_level_3 未命中".into(),
                    );
                    return false;
                }
            }
        }
        false
    }
}
