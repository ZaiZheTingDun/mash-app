//! Device-facing servant enhancement orchestration.
//!
//! Screen classification and OCR parsing remain in the parent module; this
//! module owns runner lifecycle, device input, routing, and sidecar reuse.

use super::*;

impl EnhancementRunner {
    pub fn new(
        adb: Adb,
        sidecar: SidecarClient,
        app_handle: tauri::AppHandle,
        state: Arc<Mutex<EnhancementRunnerState>>,
        cancel: Arc<AtomicBool>,
        screen_size: (u32, u32),
        target: EnhancementTarget,
        sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
    ) -> Self {
        let touch = touch::build(&adb, &app_handle, screen_size);
        Self {
            touch,
            sidecar: Some(sidecar),
            sidecar_cache,
            app_handle,
            state,
            cancel,
            screen_w: screen_size.0,
            screen_h: screen_size.1,
            target,
            pending_confirm: None,
            exp_materials_verified: false,
            servant_select_density_checked: false,
        }
    }

    pub fn run(mut self) {
        self.transition_lifecycle(EnhancementLifecycleEvent::WorkerStarted);
        self.emit(
            "",
            &format!(
                "强化自动化已启动，目标: {} / {} / {}星 / {}",
                self.target.name_jp,
                self.target.class_name,
                self.target.rarity,
                self.target.variant_key
            ),
        );

        let mut unknown_count = 0_u32;
        loop {
            if self.is_cancelled() {
                self.transition_lifecycle(EnhancementLifecycleEvent::StopRequested);
                self.emit("", "强化自动化已停止");
                return;
            }

            let (screen, ocr) = match self.detect_screen() {
                Ok(result) => result,
                Err(err) => {
                    self.fail("", format!("画面识别失败: {err}"));
                    return;
                }
            };

            if screen != EnhancementScreen::ServantSelect {
                self.servant_select_density_checked = false;
            }

            match screen {
                EnhancementScreen::HomeMenuClosed => {
                    unknown_count = 0;
                    self.handle_home_menu_closed();
                }
                EnhancementScreen::HomeMenuOpen => {
                    unknown_count = 0;
                    self.handle_home_menu_open();
                }
                EnhancementScreen::EnhanceMenu => {
                    unknown_count = 0;
                    self.handle_enhance_menu();
                }
                EnhancementScreen::ServantEnhance => {
                    unknown_count = 0;
                    self.handle_servant_enhance(&ocr);
                }
                EnhancementScreen::ServantSelect => {
                    unknown_count = 0;
                    self.handle_servant_select(&ocr);
                }
                EnhancementScreen::FilterDialog => {
                    unknown_count = 0;
                    self.handle_filter_dialog(&ocr);
                }
                EnhancementScreen::MaterialSelect => {
                    unknown_count = 0;
                    self.handle_material_select(&ocr);
                }
                EnhancementScreen::ConfirmDialog => {
                    unknown_count = 0;
                    self.handle_confirm_dialog(&ocr);
                }
                EnhancementScreen::Ascension => {
                    unknown_count = 0;
                    self.handle_ascension();
                }
                EnhancementScreen::AscensionNotReady => {
                    unknown_count = 0;
                    self.handle_ascension_not_ready();
                }
                EnhancementScreen::AscensionResult => {
                    unknown_count = 0;
                    self.handle_ascension_result();
                }
                EnhancementScreen::ProfileUpdateDialog => {
                    unknown_count = 0;
                    self.handle_profile_update_dialog(&ocr);
                }
                EnhancementScreen::Unknown => {
                    unknown_count += 1;
                    self.emit(
                        "Unknown",
                        &format!(
                            "未能识别当前页面，继续观察… ({unknown_count}/6) OCR={}",
                            summarize_ocr(&ocr)
                        ),
                    );
                    if unknown_count >= 6 {
                        self.fail(
                            "Unknown",
                            format!(
                                "连续多次无法识别当前页面，请提供当前截图。最后一次 OCR={}",
                                summarize_ocr(&ocr)
                            ),
                        );
                        return;
                    }
                    thread::sleep(Duration::from_millis(800));
                }
            }
        }
    }

    fn transition_lifecycle(
        &self,
        event: EnhancementLifecycleEvent,
    ) -> EnhancementLifecycleTransition {
        let mut state = self.state.lock().unwrap();
        let transition = enhancement_lifecycle_transition(state.clone(), event);
        if transition.accepted {
            *state = transition.next.clone();
        }
        transition
    }

    fn emit(&self, screen: &str, message: &str) {
        self.emit_with_level(screen, message, LogLevel::Info);
    }

    /// Debug-level counterpart to [`emit`]; see `Runner::emit_debug` for
    /// the full rationale. Use sparingly for technical diagnostics that
    /// don't belong in the user-facing operation log.
    #[allow(dead_code)]
    fn emit_debug(&self, screen: &str, message: &str) {
        self.emit_with_level(screen, message, LogLevel::Debug);
    }

    fn emit_with_level(&self, screen: &str, message: &str, level: LogLevel) {
        let (state_str, status) = {
            let s = self.state.lock().unwrap();
            (format!("{:?}", *s), s.status())
        };
        let _ = self.app_handle.emit(
            ENHANCEMENT_EVENT_NAME,
            EnhancementAutomationEvent {
                state: state_str,
                status,
                current_screen: screen.to_string(),
                message: message.to_string(),
                level,
            },
        );
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn sidecar(&mut self) -> &mut SidecarClient {
        self.sidecar.as_mut().expect("enhancement sidecar missing")
    }

    fn fail(&self, screen: &str, message: String) {
        self.transition_lifecycle(EnhancementLifecycleEvent::Failed {
            message: message.clone(),
        });
        self.emit(screen, &message);
    }

    fn tap_at(&mut self, screen: &str, point: Point) -> bool {
        let (px, py) = point.to_physical(self.screen_w, self.screen_h);
        match self.touch.tap(px, py) {
            Ok(()) => true,
            Err(err) => {
                self.fail(screen, format!("点击失败: {err}"));
                false
            }
        }
    }

    fn swipe_at(&mut self, screen: &str, from: Point, to: Point, duration_ms: u32) -> bool {
        let from_px = from.to_physical(self.screen_w, self.screen_h);
        let to_px = to.to_physical(self.screen_w, self.screen_h);
        match self.touch.swipe(from_px, to_px, duration_ms) {
            Ok(()) => true,
            Err(err) => {
                self.fail(screen, format!("滑动失败: {err}"));
                false
            }
        }
    }

    fn tap_template_or_point(
        &mut self,
        screen: &str,
        probe: TemplateProbe,
        fallback: Point,
    ) -> bool {
        let result = self.probe_template(probe);
        if let Some(center) = result.center {
            return self.tap_at(screen, center);
        }
        self.emit(
            screen,
            &format!(
                "模板 {} 未命中，使用兼容坐标点击。{}",
                probe.key,
                result.summary()
            ),
        );
        self.tap_at(screen, fallback)
    }

    fn ocr_region(&mut self, region: NormRect) -> Result<OcrRegionResult, String> {
        self.sidecar().ocr_region(None, region)
    }

    fn probe_template(&mut self, probe: TemplateProbe) -> TemplateProbeResult {
        match self
            .sidecar()
            .find_element_by_name(None, probe.screen, probe.element)
        {
            Ok(result) => TemplateProbeResult::from_match(probe, result),
            Err(err) => TemplateProbeResult {
                probe,
                found: false,
                score: 0.0,
                center: None,
                region: None,
                error: Some(err),
            },
        }
    }

    fn detect_screen(&mut self) -> Result<(EnhancementScreen, OcrRegionResult), String> {
        let dialog = self.ocr_region(DIALOG_CLASSIFIER_REGION)?;
        let dialog_text = normalize_text(&dialog.full_text);
        if dialog_text.contains("プロフィール") && dialog_text.contains("閉じる") {
            return Ok((EnhancementScreen::ProfileUpdateDialog, dialog));
        }
        if dialog_text.contains("★4以上")
            || (dialog_text.contains("決定")
                && (dialog_text.contains("キャンセル") || dialog_text.contains("取消")))
        {
            return Ok((EnhancementScreen::ConfirmDialog, dialog));
        }

        let template_results = self.collect_template_probe_results();
        if let Some(route) = detect_enhancement_screen(&template_results) {
            return self.route_to_screen(route, &template_results);
        }

        let result = self.ocr_region(ASCENSION_RESULT_REGION)?;
        let result_text = normalize_text(&result.full_text);
        if contains_servant_word(&result_text) && result_text.contains("強化へ") {
            return Ok((EnhancementScreen::AscensionResult, result));
        }

        let unknown_summary = self.collect_unknown_region_summaries(&template_results)?;
        Ok((
            EnhancementScreen::Unknown,
            merge_unknown_diagnostics(&template_results, &unknown_summary),
        ))
    }

    fn collect_template_probe_results(&mut self) -> Vec<TemplateProbeResult> {
        UNKNOWN_DIAGNOSTIC_PROBES
            .into_iter()
            .map(|probe| self.probe_template(probe))
            .collect()
    }

    fn route_to_screen(
        &mut self,
        route: EnhancementRoute,
        probes: &[TemplateProbeResult],
    ) -> Result<(EnhancementScreen, OcrRegionResult), String> {
        let screen = match (route.screen, route.variant, route.status) {
            (EnhancementTopScreen::Main, EnhancementVariant::Main, EnhancementStatus::MenuOpen) => {
                EnhancementScreen::HomeMenuOpen
            }
            (EnhancementTopScreen::Main, EnhancementVariant::Main, _) => {
                EnhancementScreen::HomeMenuClosed
            }
            (EnhancementTopScreen::Enhancement, EnhancementVariant::Main, _) => {
                EnhancementScreen::EnhanceMenu
            }
            (EnhancementTopScreen::ServantEnhancement, EnhancementVariant::Main, _) => {
                EnhancementScreen::ServantEnhance
            }
            (EnhancementTopScreen::ServantEnhancement, EnhancementVariant::ServantSelect, _)
            | (EnhancementTopScreen::Ascension, EnhancementVariant::ServantSelect, _) => {
                EnhancementScreen::ServantSelect
            }
            (
                EnhancementTopScreen::ServantEnhancement,
                EnhancementVariant::MaterialSelect,
                EnhancementStatus::FilterDialogOpen,
            ) => EnhancementScreen::FilterDialog,
            (EnhancementTopScreen::ServantEnhancement, EnhancementVariant::MaterialSelect, _) => {
                EnhancementScreen::MaterialSelect
            }
            (
                EnhancementTopScreen::Ascension,
                EnhancementVariant::Main,
                EnhancementStatus::NotReady,
            ) => EnhancementScreen::AscensionNotReady,
            (EnhancementTopScreen::Ascension, EnhancementVariant::Main, _) => {
                EnhancementScreen::Ascension
            }
            _ => EnhancementScreen::Unknown,
        };

        let ocr = match screen {
            EnhancementScreen::ServantEnhance => self.ocr_region(ASCENSION_ENTRY_OCR_REGION)?,
            EnhancementScreen::ServantSelect => self.ocr_region(SERVANT_LIST_REGION)?,
            EnhancementScreen::MaterialSelect => {
                let material_list = self.ocr_region(MATERIAL_LIST_REGION)?;
                let material_counter = self.ocr_region(MATERIAL_COUNTER_REGION)?;
                merge_ocr_results(&[&material_list, &material_counter])
            }
            EnhancementScreen::FilterDialog
            | EnhancementScreen::ConfirmDialog
            | EnhancementScreen::ProfileUpdateDialog => self.ocr_region(FILTER_DIALOG_REGION)?,
            _ => probe_diagnostics_as_ocr(probes),
        };
        Ok((screen, ocr))
    }

    fn collect_unknown_region_summaries(
        &mut self,
        _template_results: &[TemplateProbeResult],
    ) -> Result<Vec<NamedOcr>, String> {
        Ok(vec![
            NamedOcr {
                name: "dialog",
                result: self.ocr_region(FILTER_DIALOG_REGION)?,
            },
            NamedOcr {
                name: "servantList",
                result: self.ocr_region(SERVANT_LIST_REGION)?,
            },
            NamedOcr {
                name: "materialList",
                result: self.ocr_region(MATERIAL_LIST_REGION)?,
            },
            NamedOcr {
                name: "level",
                result: self.ocr_region(LEVEL_REGION)?,
            },
        ])
    }

    fn handle_home_menu_closed(&mut self) {
        self.emit("Home", "主页 MENU 未展开，点击 MENU");
        if self.tap_template_or_point("Home", PROBE_BUTTON_MENU, HOME_MENU_BUTTON) {
            thread::sleep(Duration::from_millis(700));
        }
    }

    fn handle_home_menu_open(&mut self) {
        self.emit("HomeMenu", "进入强化菜单");
        if self.tap_template_or_point("HomeMenu", PROBE_BUTTON_ENHANCEMENT, HOME_STRENGTHEN_BUTTON)
        {
            thread::sleep(Duration::from_millis(900));
        }
    }

    fn handle_enhance_menu(&mut self) {
        self.emit("EnhanceMenu", "进入从者强化");
        if self.tap_at("EnhanceMenu", ENHANCE_SERVANT_ENTRY_BUTTON) {
            thread::sleep(Duration::from_millis(900));
        }
    }

    fn handle_servant_enhance(&mut self, ocr: &OcrRegionResult) {
        let level = match self.sidecar().read_level_digits(None, LEVEL_DIGIT_REGION) {
            Ok(v) => v,
            Err(err) => {
                self.fail("ServantEnhance", format!("模板读取等级失败: {err}"));
                return;
            }
        };
        if let (true, Some(current), Some(max)) = (level.found, level.current, level.max_level) {
            self.emit("ServantEnhance", &format!("level digits: {}", level.text));
            if current < max {
                if self.exp_materials_verified {
                    self.emit(
                        "ServantEnhance",
                        &format!("当前等级 {current}/{max}，执行强化"),
                    );
                    self.pending_confirm = Some(PendingConfirm::Enhancement);
                    self.exp_materials_verified = false;
                    if self.tap_at("ServantEnhance", ENHANCE_CONFIRM_BUTTON) {
                        thread::sleep(Duration::from_millis(900));
                    }
                } else {
                    self.emit(
                        "ServantEnhance",
                        &format!("当前等级 {current}/{max}，进入素材页"),
                    );
                    if self.tap_at("ServantEnhance", MATERIAL_SELECT_BUTTON) {
                        thread::sleep(Duration::from_millis(900));
                    }
                }
                return;
            }

            if contains_keyword(&ocr.fragments, "霊基再臨") {
                self.exp_materials_verified = false;
                self.emit(
                    "ServantEnhance",
                    &format!("已满级 {current}/{max}，进入灵基再临"),
                );
                if self.tap_at("ServantEnhance", ASCENSION_ENTRY_BUTTON) {
                    thread::sleep(Duration::from_millis(900));
                }
                return;
            }

            self.transition_lifecycle(EnhancementLifecycleEvent::Finished);
            self.emit(
                "ServantEnhance",
                &format!("强化完成，当前等级 {current}/{max}"),
            );
            return;
        }

        self.emit(
            "ServantEnhance",
            &format!(
                "等级数字模板未命中({})，判定当前未选中目标从者，进入从者选择",
                level.fail_reason.as_deref().unwrap_or("unknown")
            ),
        );
        if self.tap_at("ServantEnhance", SERVANT_SELECT_BUTTON) {
            thread::sleep(Duration::from_millis(900));
        }
    }

    fn handle_servant_select(&mut self, ocr: &OcrRegionResult) {
        if !self.servant_select_density_checked {
            if !self.ensure_servant_select_max_density() {
                return;
            }
            self.servant_select_density_checked = true;
        }

        self.emit(
            "ServantSelect",
            &format!("查找从者头像 {}", self.target.name_jp),
        );
        let face_template_paths = self.target.face_template_paths.clone();
        match self.sidecar().find_enhancement_servant_grid(
            None,
            &face_template_paths,
            SERVANT_LIST_REGION,
            SERVANT_FACE_MATCH_CROP,
            Some(SERVANT_FACE_TEMPLATE_SIZE),
            0.85,
            1.2,
        ) {
            Ok(result) => {
                self.emit(
                    "ServantSelect",
                    &format!(
                        "网格 anchors={} cells={} best={:.3}",
                        result.anchors.len(),
                        result.grid_cells.len(),
                        result.score
                    ),
                );
                if result.found {
                    if self.tap_at("ServantSelect", Point::new(result.x, result.y)) {
                        thread::sleep(Duration::from_millis(900));
                    }
                    return;
                }
            }
            Err(err) => {
                self.fail("ServantSelect", format!("头像网格匹配失败: {err}"));
                return;
            }
        }

        let _ = self.tap_fragment_if_present("ServantSelect", &ocr.fragments, "Lv.順");
        thread::sleep(Duration::from_millis(250));
        let _ = self.tap_fragment_if_present("ServantSelect", &ocr.fragments, "降順");
        thread::sleep(Duration::from_millis(250));

        if self.swipe_at(
            "ServantSelect",
            SERVANT_LIST_SCROLL_FROM,
            SERVANT_LIST_SCROLL_TO,
            450,
        ) {
            thread::sleep(Duration::from_millis(800));
        }
    }

    fn ensure_servant_select_max_density(&mut self) -> bool {
        self.ensure_scale_level_3("ServantSelect")
    }

    fn handle_filter_dialog(&mut self, ocr: &OcrRegionResult) {
        self.emit("FilterDialog", "设置仅经验值素材筛选");
        let type_probe = self.probe_template(PROBE_TEXT_FILTER_SETTING_TYPE);
        if !type_probe.found {
            self.fail(
                "FilterDialog",
                format!(
                    "筛选弹窗中未能确认素材种类区域，停止以避免误消耗素材。{}",
                    type_probe.summary()
                ),
            );
            return;
        }
        for _ in 0..5 {
            if let Some(center) = find_exp_material_filter_center(&ocr.fragments) {
                if self.tap_at("FilterDialog", center) {
                    thread::sleep(Duration::from_millis(400));
                }
                let _ = self.tap_at("FilterDialog", FILTER_CONFIRM_BUTTON);
                thread::sleep(Duration::from_millis(900));
                return;
            }
            if !self.swipe_at("FilterDialog", FILTER_SCROLL_FROM, FILTER_SCROLL_TO, 500) {
                return;
            }
            thread::sleep(Duration::from_millis(700));
            let refreshed = match self.ocr_region(FILTER_DIALOG_REGION) {
                Ok(v) => v,
                Err(err) => {
                    self.fail("FilterDialog", format!("刷新筛选 OCR 失败: {err}"));
                    return;
                }
            };
            if let Some(center) = find_exp_material_filter_center(&refreshed.fragments) {
                if self.tap_at("FilterDialog", center) {
                    thread::sleep(Duration::from_millis(400));
                }
                let _ = self.tap_at("FilterDialog", FILTER_CONFIRM_BUTTON);
                thread::sleep(Duration::from_millis(900));
                return;
            }
        }

        self.fail(
            "FilterDialog",
            "未找到「サーヴァント(経験値)」筛选项，请确认素材筛选页面".to_string(),
        );
    }

    fn handle_material_select(&mut self, ocr: &OcrRegionResult) {
        if !self.exp_materials_look_safe(ocr) {
            self.emit("MaterialSelect", "当前素材页未确认是纯经验素材，打开筛选");
            if self.tap_at("MaterialSelect", FILTER_BUTTON) {
                thread::sleep(Duration::from_millis(900));
            }
            return;
        }

        self.exp_materials_verified = true;
        if !self.ensure_max_density() {
            return;
        }

        self.emit("MaterialSelect", "尝试拖选 20 个经验素材");
        let _ = self.swipe_at("MaterialSelect", MATERIAL_DRAG_FROM, MATERIAL_DRAG_TO, 1100);
        thread::sleep(Duration::from_millis(1000));

        let selected_after_drag = match self.read_selected_count() {
            Ok(v) => v,
            Err(err) => {
                self.fail("MaterialSelect", format!("读取已选数量失败: {err}"));
                return;
            }
        };

        if selected_after_drag < 20 {
            self.emit("MaterialSelect", "拖选未凑满 20，改用逐个点击回退");
            for point in MATERIAL_GRID_POINTS {
                if self.is_cancelled() {
                    return;
                }
                if !self.tap_at("MaterialSelect", point) {
                    return;
                }
                thread::sleep(Duration::from_millis(140));
                if let Ok(count) = self.read_selected_count() {
                    if count >= 20 {
                        break;
                    }
                }
            }
        }

        let final_count = match self.read_selected_count() {
            Ok(v) => v,
            Err(err) => {
                self.fail("MaterialSelect", format!("读取最终已选数量失败: {err}"));
                return;
            }
        };
        if final_count < 20 {
            self.fail(
                "MaterialSelect",
                format!("经验素材选择不足，当前仅选择了 {final_count}/20"),
            );
            return;
        }

        self.emit("MaterialSelect", "已选择 20 个经验素材，返回强化页");
        if self.tap_at("MaterialSelect", MATERIAL_CONFIRM_BUTTON) {
            thread::sleep(Duration::from_millis(900));
        }
    }

    fn handle_confirm_dialog(&mut self, ocr: &OcrRegionResult) {
        if normalize_text(&ocr.full_text).contains("★4以上") && !self.exp_materials_verified {
            self.fail(
                "ConfirmDialog",
                "出现高星素材警告，但当前未能确认素材仅为经验值".to_string(),
            );
            return;
        }

        self.emit("ConfirmDialog", "确认执行强化");
        if !self.tap_at("ConfirmDialog", DIALOG_CONFIRM_BUTTON) {
            return;
        }
        thread::sleep(Duration::from_millis(900));

        match self.pending_confirm.take() {
            Some(PendingConfirm::Enhancement) => {
                let _ = self.skip_until(
                    "EnhancementAnimation",
                    &[EnhancementScreen::ServantEnhance],
                    Duration::from_secs(30),
                );
            }
            Some(PendingConfirm::Ascension) => {
                let _ = self.skip_until(
                    "AscensionAnimation",
                    &[
                        EnhancementScreen::AscensionResult,
                        EnhancementScreen::Ascension,
                        EnhancementScreen::AscensionNotReady,
                    ],
                    Duration::from_secs(90),
                );
            }
            None => {}
        }
    }

    fn handle_ascension(&mut self) {
        self.emit("Ascension", "执行灵基再临");
        self.pending_confirm = Some(PendingConfirm::Ascension);
        if self.tap_at("Ascension", ENHANCE_CONFIRM_BUTTON) {
            thread::sleep(Duration::from_millis(900));
        }
    }

    fn handle_ascension_not_ready(&self) {
        self.fail(
            "Ascension",
            "检测到灵基再临 not_ready 状态，停止以避免无效点击；请确认材料或补充可执行状态模板"
                .to_string(),
        );
    }

    fn handle_ascension_result(&mut self) {
        self.emit("AscensionResult", "灵基再临完成，返回从者强化页");
        if self.tap_template_or_point(
            "AscensionResult",
            PROBE_BUTTON_ASCENSION_TO_SERVANT,
            ASCENSION_RESULT_RETURN_BUTTON,
        ) {
            thread::sleep(Duration::from_millis(900));
        }
    }

    fn handle_profile_update_dialog(&mut self, ocr: &OcrRegionResult) {
        self.emit("ProfileDialog", "关闭资料更新弹窗");
        if let Some(center) = find_best_fragment_center(&ocr.fragments, "閉じる") {
            if self.tap_at("ProfileDialog", center) {
                thread::sleep(Duration::from_millis(700));
            }
        } else if self.tap_at("ProfileDialog", PAGE_CENTER_TAP) {
            thread::sleep(Duration::from_millis(700));
        }
    }

    fn tap_fragment_if_present(
        &mut self,
        screen: &str,
        fragments: &[OcrFragment],
        needle: &str,
    ) -> bool {
        if let Some(center) = find_best_fragment_center(fragments, needle) {
            self.tap_at(screen, center)
        } else {
            false
        }
    }

    fn exp_materials_look_safe(&self, ocr: &OcrRegionResult) -> bool {
        let normalized = normalize_text(&ocr.full_text);
        let exp_count = normalized.matches("expup").count() + normalized.matches("exp").count();
        let banned = normalized.contains("atk")
            || normalized.contains("hp")
            || normalized.contains("フォウ")
            || normalized.contains("fou");
        exp_count >= 8 && !banned
    }

    fn ensure_max_density(&mut self) -> bool {
        self.ensure_scale_level_3("MaterialSelect")
    }

    fn ensure_scale_level_3(&mut self, screen: &str) -> bool {
        self.emit(screen, "确认一屏最多显示模式");
        let mut last = self.probe_template(PROBE_BUTTON_SCALE_LEVEL_3);
        for attempt in 0..=3 {
            match scale_level_3_decision(attempt, last.found) {
                ScaleLevel3Decision::Done => return true,
                ScaleLevel3Decision::Fail => {
                    self.fail(
                        screen,
                        format!(
                            "无法切换到一屏最多显示模式，button_scale_level_3 未命中。{}",
                            last.summary()
                        ),
                    );
                    return false;
                }
                ScaleLevel3Decision::TapAndRetry => {
                    if !self.tap_at(screen, GRID_DENSITY_BUTTON) {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(500));
                    last = self.probe_template(PROBE_BUTTON_SCALE_LEVEL_3);
                }
            }
        }
        false
    }

    fn read_selected_count(&mut self) -> Result<u32, String> {
        let ocr = self.ocr_region(MATERIAL_COUNTER_REGION)?;
        Ok(parse_selected_count(&ocr.full_text).unwrap_or(0))
    }

    fn skip_until(
        &mut self,
        label: &str,
        targets: &[EnhancementScreen],
        timeout: Duration,
    ) -> Result<(), String> {
        let start = Instant::now();
        let mut last_tap = Instant::now() - Duration::from_secs(3);
        loop {
            if self.is_cancelled() {
                return Ok(());
            }
            let (screen, ocr) = self.detect_screen()?;
            if targets.contains(&screen) {
                return Ok(());
            }
            if screen == EnhancementScreen::ProfileUpdateDialog {
                self.handle_profile_update_dialog(&ocr);
                continue;
            }
            if start.elapsed() >= timeout {
                return Err(format!("{label} 超时"));
            }
            if last_tap.elapsed() >= Duration::from_millis(1200) {
                self.emit(label, "点击屏幕跳过动画…");
                if !self.tap_at(label, PAGE_CENTER_TAP) {
                    return Err("点击跳过动画失败".to_string());
                }
                last_tap = Instant::now();
            }
            thread::sleep(Duration::from_millis(400));
        }
    }
}

impl Drop for EnhancementRunner {
    fn drop(&mut self) {
        let Some(mut sidecar) = self.sidecar.take() else {
            return;
        };
        if let Err(err) = sidecar.prepare_for_cache() {
            eprintln!("[mash-cv] prepare enhancement sidecar for cache failed: {err}");
        }
        if let Some(cache) = &self.sidecar_cache {
            let mut guard = cache.lock().unwrap();
            if guard.is_none() {
                *guard = Some(sidecar);
                return;
            }
        }
    }
}
