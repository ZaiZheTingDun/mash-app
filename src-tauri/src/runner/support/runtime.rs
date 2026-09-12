//! Device-facing support selection orchestration.
//!
//! Matching, filtering, and region policies remain in the parent module; this
//! module owns Sidecar probes, list navigation, taps, and refresh lifecycle.

use super::*;

impl Runner {
    pub(crate) fn handle_support_select(&mut self) {
        // No servant pinned → fall back to "tap the top of the list" so
        // existing setups that never picked a support still work.
        let Some(servant_id) = self.config.support_servant_id else {
            self.legacy_pick_first_support();
            return;
        };

        // Lazy-load (name, np_names, class_name) once per run. The shared
        // static cache in `lib.rs` makes this cheap, but caching on the
        // runner avoids even hashing it at every poll.
        if self.support_meta.is_none() {
            match load_servant_metadata_for_variant(
                &self.app_handle,
                servant_id,
                self.server,
                self.config.support_servant_variant_key.as_deref(),
            ) {
                Ok(meta) => self.support_meta = Some(meta),
                Err(e) => {
                    self.fail_action(
                        "SupportSelect",
                        "加载助战元数据",
                        format!("servant {servant_id}: {e}"),
                    );
                    return;
                }
            }
        }
        let meta = self.support_meta.clone().unwrap();

        // Filter the list to the servant's class before scanning. We avoid
        // searching from "all" + "mix" because they interleave duplicates
        // and lengthen every OCR pass; the class tab restricts the list to
        // exactly the rows we care about.
        if !self.support_class_tab_done {
            match support_class_filter_action(
                self.server,
                &meta.class_name,
                self.config.enable_extra_class_filter,
                self.support_extra_class_filter_configured,
            ) {
                Some(SupportClassFilterAction::Tap(tab)) => {
                    self.emit(
                        "SupportSelect",
                        &format!("切换职阶筛选 -> {}", meta.class_name),
                    );
                    if !self.tap_at("SupportSelect", tab) {
                        return;
                    }
                    self.support_class_tab_done = true;
                    thread::sleep(SUPPORT_CLASS_TAB_SETTLE);
                    return;
                }
                Some(SupportClassFilterAction::CnExtra(extra)) => {
                    self.emit(
                        "SupportSelect",
                        &format!("设置 EXTRA 具体职阶筛选 -> {}", extra.label),
                    );
                    if !self.configure_cn_extra_class_filter(extra) {
                        return;
                    }
                    self.support_extra_class_filter_configured = true;
                    self.support_class_tab_done = true;
                    thread::sleep(SUPPORT_EXTRA_FILTER_RESULT_SETTLE);
                    return;
                }
                None => {
                    // Unknown class → mark done so we don't loop, and let
                    // the OCR pass run against whatever tab is active.
                    eprintln!(
                        "[runner] no class-tab mapping for className='{}', skipping filter",
                        meta.class_name,
                    );
                    self.support_class_tab_done = true;
                }
            }
        }

        // OCR the current (class-filtered) screen and look for a row
        // whose name + NP both fuzzy-match the pinned servant.
        let include_support_details =
            support_level_filtering_enabled(self.server) && self.has_support_level_requirements();
        let support_full_list_ocr_fallback = self.config.support_full_list_ocr_fallback;
        let result = match self.sidecar().find_supports(
            None,
            &meta.name,
            &meta.names,
            &meta.excluded_names,
            &meta.np_names,
            meta.require_np_match,
            include_support_details,
            support_full_list_ocr_fallback,
        ) {
            Ok(r) => r,
            Err(e) => {
                self.release_support_ocr();
                self.fail_action("SupportSelect", "OCR 助战识别", e);
                return;
            }
        };

        // `supports` is already sorted top-down by the sidecar. Default
        // pick = first match, but optional CE / level filters can reject
        // rows before we tap.
        let ce_filter_enabled = self.support_ce_filter_enabled();
        let mut waiting_for_skill_panel = false;
        let mut ce_filter_reasons: Vec<String> = Vec::new();
        let mut ce_filter_debug_summaries: Vec<String> = Vec::new();
        // Distinct mismatch reasons across the visible candidates, in
        // first-seen order. Same servant from multiple friends often
        // means the same gap (e.g. "持有技能 2 ≥ 10（实际 8）"); dedup
        // keeps the log readable.
        let mut level_filter_reasons: Vec<String> = Vec::new();
        let mut skill_wait_diagnostic: Option<String> = None;
        let mut chosen_index: Option<usize> = None;
        for (index, row) in result.supports.iter().enumerate() {
            if let Some(mismatch) = self.support_row_ce_mismatch(row) {
                if !ce_filter_reasons.contains(&mismatch.reason) {
                    ce_filter_reasons.push(mismatch.reason);
                }
                if let Some(summary) = mismatch.debug_summary {
                    if !ce_filter_debug_summaries.contains(&summary) {
                        ce_filter_debug_summaries.push(summary);
                    }
                }
                continue;
            }
            let previous_candidate_key = self.support_level_progress.candidate_key.clone();
            let current_candidate_key = support_level_candidate_key(row);
            match support_row_matches_level_requirements_with_progress(
                self.server,
                &self.config,
                row,
                &mut self.support_level_progress,
            ) {
                SupportLevelFilter::Pass => {
                    chosen_index = Some(index);
                    break;
                }
                SupportLevelFilter::WaitingForPanel => {
                    let diagnostic = support_level_wait_diagnostic(
                        row,
                        previous_candidate_key.as_deref(),
                        &current_candidate_key,
                    );
                    if self.support_level_progress.panel_toggle_taps
                        < SUPPORT_SKILL_PANEL_MAX_TOGGLE_TAPS
                    {
                        waiting_for_skill_panel = true;
                        skill_wait_diagnostic = Some(diagnostic);
                        break;
                    }
                    self.emit(
                        "SupportSelect",
                        &format!(
                            "切换面板已达上限，跳过当前助战，继续检查同屏候选…；识别 {diagnostic}"
                        ),
                    );
                    self.support_level_progress = SupportLevelPanelProgress::default();
                }
                SupportLevelFilter::Fail(reason) => {
                    if !level_filter_reasons.contains(&reason) {
                        level_filter_reasons.push(reason);
                    }
                }
            }
        }
        let chosen = chosen_index.and_then(|index| result.supports.get(index));

        if let Some(row) = chosen {
            self.release_support_ocr();
            self.emit("SupportSelect", &support_found_summary(&meta.name, row));
            self.emit_debug(
                "SupportSelect",
                &support_found_debug_detail(&meta.name, row),
            );
            if !self.tap_at("SupportSelect", support_row_tap_point(row)) {
                return;
            }
            self.support_selected = true;
            self.support_scroll_count = 0;
            self.support_refresh_count = 0;
            self.support_grand_section_seen = false;
            self.support_grand_section_misses = 0;
            self.support_class_tab_done = false;
            self.support_level_progress = SupportLevelPanelProgress::default();
            thread::sleep(ACTION_DELAY);
            return;
        }

        // OCR found rows but the pinned CE didn't match any of them —
        // emit a distinct message before falling through to the
        // scroll/refresh branch so the user knows it's a CE filter miss
        // (vs a name miss).
        if ce_filter_enabled && !result.supports.is_empty() {
            let reason = if ce_filter_reasons.is_empty() {
                "礼装不匹配".to_string()
            } else {
                ce_filter_reasons.join("；")
            };
            self.emit("SupportSelect", &format!("找到从者但{reason}，继续滚动…"));
            for summary in ce_filter_debug_summaries {
                self.emit_debug("SupportSelect", &summary);
            }
        }
        if waiting_for_skill_panel {
            // The candidate's name + NP match but we still need the
            // *other* skill panel to confirm its level requirements.
            // We can't rely on the game auto-flipping panels (the user
            // may have the toggle locked on 固定持有 or 固定追加), so
            // actively tap "技能显示切换" until both panels have been
            // observed. The per-row cap is enforced inside the scan loop
            // so an exhausted top candidate can fall through to another
            // visible candidate before we scroll/refresh.
            self.support_level_progress.panel_toggle_taps += 1;
            self.emit(
                "SupportSelect",
                &format!(
                    "找到从者，主动点击技能显示切换 ({}/{}){}",
                    self.support_level_progress.panel_toggle_taps,
                    SUPPORT_SKILL_PANEL_MAX_TOGGLE_TAPS,
                    skill_wait_diagnostic
                        .as_deref()
                        .map(|diag| format!("；识别 {diag}"))
                        .unwrap_or_default(),
                ),
            );
            if !self.tap_at("SupportSelect", SUPPORT_SKILL_PANEL_TOGGLE_BUTTON) {
                return;
            }
            thread::sleep(SUPPORT_SKILL_PANEL_TOGGLE_SETTLE);
            return;
        }
        if !level_filter_reasons.is_empty() {
            self.emit(
                "SupportSelect",
                &format!(
                    "找到从者但等级不匹配（{}），继续滚动…",
                    level_filter_reasons.join("；"),
                ),
            );
        }

        // No match in the visible viewport. Normal support scans use the
        // scroll-bar tail indicator as the source of truth. Grand support
        // scans can stop earlier: Grand rows are listed before ordinary
        // rows, and the per-anchor "冠位从者" ribbon probe (run by the
        // sidecar inside `find_supports` and surfaced via
        // `diagnostics.is_grand_section_visible`) goes false once the
        // visible page has scrolled past the Grand section.
        let grand_section_exhausted = self.config.support_grand_mode
            && self.update_support_grand_section_exhausted(
                result.diagnostics.is_grand_section_visible,
            );
        if !grand_section_exhausted && !self.support_scroll_bar_at_end() {
            // Surface the Grand-section signal in the normal log, so the
            // operator can see whether this scroll continues a Grand scan
            // or a normal support-list scan.
            let grand_visible_in_grand_mode = self.config.support_grand_mode
                && result.diagnostics.is_grand_section_visible == Some(true);
            self.emit(
                "SupportSelect",
                &support_not_found_scroll_message(
                    &meta.name,
                    grand_visible_in_grand_mode,
                    self.support_scroll_count + 1,
                ),
            );
            if !self.scroll_support_list(&result.diagnostics.confirm_button_anchors) {
                return;
            }
            self.support_scroll_count += 1;
            self.support_level_progress = SupportLevelPanelProgress::default();
            thread::sleep(SUPPORT_SCROLL_SETTLE);
        } else if self.support_refresh_count < SUPPORT_MAX_REFRESHES {
            let reason = if grand_section_exhausted {
                "冠位助战已扫完"
            } else {
                "已到底部"
            };
            self.emit(
                "SupportSelect",
                &format!(
                    "{reason}，刷新助战列表 ({}/{})",
                    self.support_refresh_count + 1,
                    SUPPORT_MAX_REFRESHES,
                ),
            );
            if !self.wait_for_support_refresh_available() {
                return;
            }
            if !self.tap_at("SupportSelect", SUPPORT_REFRESH_BUTTON) {
                return;
            }
            self.support_scroll_count = 0;
            self.support_refresh_count += 1;
            self.support_grand_section_seen = false;
            self.support_grand_section_misses = 0;
            self.support_level_progress = SupportLevelPanelProgress::default();
            // Refresh occasionally snaps the class filter back to "all";
            // re-tap the class tab on the next poll to be safe.
            self.support_class_tab_done = false;
            if !self.confirm_support_refresh_dialog_if_needed() {
                return;
            }
        } else {
            self.release_support_ocr();
            self.fail_action(
                "SupportSelect",
                "查找助战",
                format!("刷新 {} 次仍未找到 {}", SUPPORT_MAX_REFRESHES, meta.name),
            );
        }
    }

    pub(crate) fn release_support_ocr(&mut self) {
        if let Err(error) = self.sidecar().release_ocr() {
            eprintln!("[mash-cv] release support-search OCR worker failed: {error}");
        }
    }

    fn configure_cn_extra_class_filter(&mut self, extra: ExtraClassFilter) -> bool {
        loop {
            if self.is_cancelled() {
                return false;
            }
            if !self.press_at(
                "SupportSelect",
                SUPPORT_TAB_EXTRA,
                SUPPORT_EXTRA_FILTER_LONG_PRESS_MS,
            ) {
                return false;
            }
            let wait = self.wait_for_support_extra_filter_dialog_once(true);
            if wait == SupportExtraFilterDialogWait::Matched {
                break;
            }
            if !should_retry_support_extra_filter_dialog(wait) {
                return false;
            }
            self.emit(
                "SupportSelect",
                "EXTRA 职阶筛选弹窗未出现，重新长按 EXTRA 页签",
            );
        }
        thread::sleep(SUPPORT_EXTRA_FILTER_ACTION_SETTLE);

        if !self.tap_at("SupportSelect", SUPPORT_EXTRA_FILTER_RESET_BUTTON) {
            return false;
        }
        thread::sleep(SUPPORT_EXTRA_FILTER_ACTION_SETTLE);
        if !self.tap_at("SupportSelect", extra.point) {
            return false;
        }
        thread::sleep(SUPPORT_EXTRA_FILTER_ACTION_SETTLE);
        // `adb input tap` is effectively a zero-duration press. On this
        // modal it can dismiss on DOWN and leak the UP into the support row
        // underneath, selecting a servant before OCR runs. A 100-ms
        // stationary press matches a human tap and is consumed by the modal.
        if !self.press_at(
            "SupportSelect",
            SUPPORT_EXTRA_FILTER_CONFIRM_BUTTON,
            SUPPORT_EXTRA_FILTER_CONFIRM_PRESS_MS,
        ) {
            return false;
        }
        self.wait_for_support_extra_filter_dialog(false)
    }

    fn wait_for_support_extra_filter_dialog(&mut self, expected_visible: bool) -> bool {
        match self.wait_for_support_extra_filter_dialog_once(expected_visible) {
            SupportExtraFilterDialogWait::Matched => true,
            SupportExtraFilterDialogWait::Aborted => false,
            SupportExtraFilterDialogWait::TimedOut => {
                let state = if expected_visible { "打开" } else { "关闭" };
                self.fail_action(
                    "SupportSelect",
                    &format!("等待 EXTRA 职阶筛选弹窗{state}"),
                    "超时".into(),
                );
                false
            }
        }
    }

    fn wait_for_support_extra_filter_dialog_once(
        &mut self,
        expected_visible: bool,
    ) -> SupportExtraFilterDialogWait {
        let deadline = Instant::now() + SUPPORT_EXTRA_FILTER_DIALOG_TIMEOUT;
        loop {
            if self.is_cancelled() {
                return SupportExtraFilterDialogWait::Aborted;
            }
            match self.sidecar().find_element_by_name(
                None,
                SUPPORT_SELECT_SCREEN,
                SUPPORT_EXTRA_FILTER_DIALOG_ELEMENT,
            ) {
                Ok(matched) if matched.found == expected_visible => {
                    return SupportExtraFilterDialogWait::Matched;
                }
                Ok(_) => {}
                Err(err) => {
                    self.fail_action("SupportSelect", "识别 EXTRA 职阶筛选弹窗", err);
                    return SupportExtraFilterDialogWait::Aborted;
                }
            }
            if Instant::now() >= deadline {
                return SupportExtraFilterDialogWait::TimedOut;
            }
            thread::sleep(SUPPORT_EXTRA_FILTER_DIALOG_POLL);
        }
    }

    /// Resolve all ordinary-support CE templates. The ordered multi-select
    /// list takes precedence; a legacy single id is used as fallback.
    /// Cached on first call so repeated polls don't restat the filesystem.
    pub(crate) fn resolve_support_ce_templates(&mut self) -> Vec<PathBuf> {
        if let Some(cached) = &self.support_ce_templates {
            return cached.clone();
        }
        let resolved = if self.config.support_grand_mode {
            Vec::new()
        } else {
            ordinary_support_ce_ids(&self.config)
                .into_iter()
                .filter_map(|ce_id| self.resolve_ce_template_path(ce_id))
                .collect()
        };
        self.support_ce_templates = Some(resolved.clone());
        resolved
    }

    pub(crate) fn resolve_support_grand_ce_templates(&mut self) -> [Vec<PathBuf>; 3] {
        if let Some(cached) = &self.support_grand_ce_templates {
            return cached.clone();
        }
        let resolved = std::array::from_fn(|index| {
            grand_support_ce_ids(&self.config, index)
                .into_iter()
                .filter_map(|ce_id| self.resolve_ce_template_path(ce_id))
                .collect()
        });
        self.support_grand_ce_templates = Some(resolved.clone());
        resolved
    }

    pub(crate) fn resolve_ce_template_path(&self, ce_id: u32) -> Option<PathBuf> {
        let dir = self.ce_assets_dir.as_ref()?;
        let path = dir.join(ce_id.to_string()).join("card_ce.png");
        if path.is_file() {
            Some(path)
        } else {
            eprintln!(
                "[runner] support CE template missing: {} (skipping CE filter)",
                path.display()
            );
            None
        }
    }

    pub(crate) fn support_ce_filter_enabled(&self) -> bool {
        if self.config.support_grand_mode {
            (0..3).any(|index| !grand_support_ce_ids(&self.config, index).is_empty())
        } else {
            !ordinary_support_ce_ids(&self.config).is_empty()
        }
    }

    /// Compute the absolute search window for a row's CE icon by
    /// applying `SUPPORT_CE_OFFSET_IN_ROW` (a row-local rect) to the
    /// row's full bbox. Thin method wrapper around the pure free helper
    /// [`ce_search_region`] (kept free so unit tests can exercise the
    /// math without constructing a full `SupportRowMatch`).
    pub(crate) fn support_ce_search_region(row: &SupportRowMatch) -> NormRect {
        ce_search_region(row.row_region)
    }

    pub(crate) fn support_grand_ce_search_region(
        row: &SupportRowMatch,
        slot: usize,
    ) -> Option<NormRect> {
        grand_ce_search_region(row, slot)
    }

    pub(crate) fn support_row_region_ce_mismatch(
        &mut self,
        region: NormRect,
        template_path: &Path,
        label: &str,
        mut options: SupportCeVerificationOptions,
    ) -> Option<SupportCeMismatch> {
        let support_ce_threshold = self.config.support_ce_threshold;
        options.full_gate_threshold = self.config.support_ce_full_gate_threshold;
        options.mlb_icon_threshold = self.config.support_mlb_icon_threshold;
        options.bond_icon_threshold = self.config.support_bond_icon_threshold;
        match self.sidecar().verify_support_ce(
            None,
            region,
            template_path,
            support_ce_threshold,
            options,
        ) {
            Ok(result) => {
                let effective_threshold = if result.threshold > 0.0 {
                    result.threshold
                } else {
                    support_ce_threshold
                };
                eprintln!(
                    "[runner] {label} verify: score={:.3} threshold={:.2} -> {}",
                    result.score,
                    effective_threshold,
                    if result.passed { "PASS" } else { "skip" },
                );
                if !result.artwork_checks.is_empty() {
                    eprintln!(
                        "[runner] {label} variants: {}",
                        format_ce_artwork_checks(&result.artwork_checks),
                    );
                }
                for check in &result.icon_checks {
                    eprintln!(
                        "[runner] {label} {} icon: score={:.3} threshold={:.2} -> {}",
                        check.kind,
                        check.score,
                        check.threshold,
                        if check.passed { "PASS" } else { "skip" },
                    );
                }
                mismatch_from_ce_result(label, &result, effective_threshold)
            }
            Err(e) => {
                eprintln!("[runner] support CE verify failed (treating as skip): {e}");
                Some(SupportCeMismatch::reason_only(format!("{label} 校验失败")))
            }
        }
    }

    pub(crate) fn support_row_ce_mismatch(
        &mut self,
        row: &SupportRowMatch,
    ) -> Option<SupportCeMismatch> {
        if self.config.support_grand_mode {
            let templates = self.resolve_support_grand_ce_templates();
            if templates.iter().any(|items| !items.is_empty()) && row.score_anchor.is_none() {
                return Some(SupportCeMismatch::reason_only("确认按钮未完整显示"));
            }
            for (index, slot_templates) in templates.iter().enumerate() {
                if slot_templates.is_empty() {
                    continue;
                }
                let Some(region) = Self::support_grand_ce_search_region(row, index) else {
                    return Some(SupportCeMismatch::reason_only(format!(
                        "冠位礼装 {} 区域无效",
                        index + 1
                    )));
                };
                let mut final_mismatch = None;
                for (candidate_index, template) in slot_templates.iter().enumerate() {
                    let label = if slot_templates.len() == 1 {
                        format!("冠位礼装 {}", index + 1)
                    } else {
                        format!("冠位礼装 {} 候选 {}", index + 1, candidate_index + 1)
                    };
                    match self.support_row_region_ce_mismatch(
                        region,
                        template,
                        &label,
                        SupportCeVerificationOptions {
                            mlb_required: self.config.support_grand_craft_essence_mlb_required
                                [index],
                            grand_bond_ce_mode: if index == 1 {
                                match self.config.support_grand_bond_ce_mode {
                                    SupportGrandBondCeMode::Any => None,
                                    SupportGrandBondCeMode::Bond => Some("bond".to_string()),
                                    SupportGrandBondCeMode::BondNp => Some("bondNp".to_string()),
                                }
                            } else {
                                None
                            },
                            ..Default::default()
                        },
                    ) {
                        None => {
                            final_mismatch = None;
                            break;
                        }
                        Some(mismatch) => final_mismatch = Some(mismatch),
                    }
                }
                if let Some(reason) = final_mismatch {
                    return Some(reason);
                }
            }
            None
        } else {
            let templates = self.resolve_support_ce_templates();
            if templates.is_empty() {
                return None;
            }
            let region = Self::support_ce_search_region(row);
            let mut final_mismatch = None;
            for (index, template) in templates.iter().enumerate() {
                let label = if templates.len() == 1 {
                    "礼装".to_string()
                } else {
                    format!("候选礼装 {}", index + 1)
                };
                match self.support_row_region_ce_mismatch(
                    region,
                    template,
                    &label,
                    SupportCeVerificationOptions {
                        mlb_required: self.config.support_craft_essence_mlb_required,
                        grand_bond_ce_mode: None,
                        ..Default::default()
                    },
                ) {
                    None => return None,
                    Some(mismatch) => final_mismatch = Some(mismatch),
                }
            }
            final_mismatch.map(|mismatch| SupportCeMismatch {
                reason: if mismatch.reason.contains("完整匹配不足") {
                    let suffix = mismatch
                        .reason
                        .split_once("完整匹配不足")
                        .map(|(_, suffix)| suffix)
                        .unwrap_or_default();
                    format!("礼装完整匹配不足{suffix}")
                } else {
                    "礼装不匹配".to_string()
                },
                debug_summary: mismatch.debug_summary,
            })
        }
    }

    pub(crate) fn has_support_level_requirements(&self) -> bool {
        self.config.support_servant_level_min.is_some()
            || self.config.support_noble_phantasm_level_min.is_some()
            || self.config.support_star_map_score_min.is_some()
            || (self.config.support_grand_mode
                && self.config.support_grand_star_map_score_min.is_some())
            || self
                .config
                .support_skill_level_mins
                .iter()
                .any(Option::is_some)
            || self
                .config
                .support_append_skill_level_mins
                .iter()
                .any(Option::is_some)
    }

    /// Scroll the support list by an adaptive distance so the last
    /// visible confirm-button anchor lands near the first-row confirm
    /// position. See `scroll_support_list_delta` for the math; the short
    /// version is `delta = last_button.y - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y`.
    /// This avoids guessing about clipped rows below the viewport.
    ///
    /// When no anchors are visible (rare — usually means the template
    /// detector glitched on this frame) we fall back to a single fixed
    /// delta so the runner still makes progress.
    ///
    /// Emits a local-only debug log with the detected anchor positions
    /// and the chosen delta for development triage.
    pub(crate) fn scroll_support_list(&mut self, confirm_button_anchors: &[NormRect]) -> bool {
        let delta = scroll_support_list_delta(confirm_button_anchors);
        let from_y = SUPPORT_SCROLL_FROM_Y;
        let to_y = (from_y - delta).max(0.05);
        let swipe_ms = scroll_support_list_duration_ms(delta);
        let scroll_msg = format_scroll_debug(
            confirm_button_anchors,
            delta,
            from_y,
            to_y,
            swipe_ms,
            SUPPORT_SCROLL_SETTLE_MS,
        );
        // Tag the debug line with the active touch backend's name so
        // an operator can tell at a glance which backend produced the
        // gesture they're triaging — useful when we add more
        // backends behind the `TouchBackend` trait again.
        self.emit_local_debug(
            "SupportSelect",
            &format!("{scroll_msg} [{}]", self.touch.name()),
        );
        self.swipe_with_settle_at(
            "SupportSelect",
            Point::new(0.50, from_y),
            Point::new(0.50, to_y),
            swipe_ms,
            SUPPORT_SCROLL_SETTLE_MS,
        )
    }

    /// Return true when the scroll-bar-end indicator is visible in the
    /// bottom-right corner of the support list, meaning the user has
    /// scrolled all the way down. If the list is still at the top and the
    /// start indicator is absent, treat the list as non-scrollable and
    /// therefore already exhausted. Logs the actual match score every poll
    /// so the threshold can be tuned from real numbers; transient sidecar
    /// errors degrade to `false` so a CV blip just means "keep scrolling"
    /// instead of triggering a refresh loop.
    pub(crate) fn support_scroll_bar_at_end(&mut self) -> bool {
        match self.sidecar().find_element_by_name(
            None,
            SUPPORT_SELECT_SCREEN,
            SUPPORT_SCROLL_END_ELEMENT,
        ) {
            Ok(m) if m.found => {
                eprintln!(
                    "[runner] scroll-bar-end score={:.3} -> {}",
                    m.score, "AT-BOTTOM",
                );
                return true;
            }
            Ok(m) => {
                eprintln!(
                    "[runner] scroll-bar-end score={:.3} -> not-at-bottom",
                    m.score,
                );
            }
            Err(e) => {
                eprintln!("[runner] scroll-bar-end check failed (treating as not-at-bottom): {e}");
            }
        }

        if self.support_scroll_count > 0 {
            return false;
        }

        match self.sidecar().find_element_by_name(
            None,
            SUPPORT_SELECT_SCREEN,
            SUPPORT_SCROLL_START_ELEMENT,
        ) {
            Ok(m) if m.found => {
                eprintln!(
                    "[runner] scroll-bar-start score={:.3} -> scrollable",
                    m.score,
                );
                false
            }
            Ok(m) => {
                eprintln!(
                    "[runner] scroll-bar-start score={:.3} -> no-scrollbar",
                    m.score,
                );
                true
            }
            Err(e) => {
                eprintln!("[runner] scroll-bar-start check failed (treating as scrollable): {e}");
                false
            }
        }
    }

    pub(crate) fn wait_for_support_refresh_available(&mut self) -> bool {
        let deadline = Instant::now() + SUPPORT_REFRESH_AVAILABLE_TIMEOUT;
        let mut emitted_wait = false;
        loop {
            if self.is_cancelled() {
                return false;
            }
            match self.sidecar().find_element_by_name(
                None,
                SUPPORT_SELECT_SCREEN,
                SUPPORT_REFRESH_AVAILABLE_ELEMENT,
            ) {
                Ok(m) if m.found => {
                    eprintln!("[runner] support-refresh score={:.3} -> available", m.score);
                    return true;
                }
                Ok(m) => {
                    eprintln!("[runner] support-refresh score={:.3} -> cooldown", m.score);
                    if !emitted_wait {
                        self.emit("SupportSelect", "等待助战刷新按钮可用");
                        emitted_wait = true;
                    }
                }
                Err(e)
                    if is_unknown_element_error(
                        &e,
                        SUPPORT_SELECT_SCREEN,
                        SUPPORT_REFRESH_AVAILABLE_ELEMENT,
                    ) =>
                {
                    eprintln!(
                        "[runner] support-refresh availability probe not configured; tapping directly"
                    );
                    return true;
                }
                Err(e) => {
                    eprintln!("[runner] support-refresh availability probe failed: {e}");
                    if !emitted_wait {
                        self.emit("SupportSelect", "等待助战刷新按钮可用");
                        emitted_wait = true;
                    }
                }
            }
            if Instant::now() >= deadline {
                self.fail_action("SupportSelect", "等待助战刷新", "刷新按钮仍不可用".into());
                return false;
            }
            thread::sleep(SUPPORT_REFRESH_AVAILABLE_POLL);
        }
    }

    /// In Grand support mode, given the sidecar's per-frame "is at
    /// least one Grand row visible?" probe (from
    /// `SupportDiagnostics::is_grand_section_visible`), update the
    /// rolling miss counter and return whether the Grand section has
    /// been exhausted. `None` from the sidecar means the active
    /// server bundle doesn't ship the ribbon template, so callers
    /// fall back to scroll-bar-end via the helper's existing logic.
    pub(crate) fn update_support_grand_section_exhausted(&mut self, visible: Option<bool>) -> bool {
        eprintln!(
            "[runner] grand-servant ribbon visible={:?} (seen={}, misses={})",
            visible, self.support_grand_section_seen, self.support_grand_section_misses,
        );
        support_grand_section_exhausted_after_probe(
            visible,
            &mut self.support_grand_section_seen,
            &mut self.support_grand_section_misses,
        )
    }

    /// After tapping the support-list refresh button, confirm the modal when
    /// the server's cv.json defines one. Servers without a modal template
    /// fall back to the legacy fixed settle delay.
    pub(crate) fn confirm_support_refresh_dialog_if_needed(&mut self) -> bool {
        let appear_deadline = std::time::Instant::now() + SUPPORT_REFRESH_DIALOG_APPEAR_TIMEOUT;
        loop {
            if self.is_cancelled() {
                return false;
            }
            match self.sidecar().find_element_by_name(
                None,
                SUPPORT_SELECT_SCREEN,
                SUPPORT_REFRESH_DIALOG_ELEMENT,
            ) {
                Ok(m) if m.found => break,
                Ok(_) => {}
                Err(e)
                    if is_unknown_element_error(
                        &e,
                        SUPPORT_SELECT_SCREEN,
                        SUPPORT_REFRESH_DIALOG_ELEMENT,
                    ) =>
                {
                    thread::sleep(SUPPORT_REFRESH_SETTLE);
                    return true;
                }
                Err(e) => {
                    eprintln!("[runner] refresh dialog probe failed (treating as absent): {e}");
                    break;
                }
            }
            if std::time::Instant::now() >= appear_deadline {
                thread::sleep(SUPPORT_REFRESH_SETTLE);
                return true;
            }
            thread::sleep(SUPPORT_REFRESH_DIALOG_POLL);
        }

        thread::sleep(SUPPORT_REFRESH_DIALOG_CONFIRM_SETTLE);
        if self.is_cancelled() {
            return false;
        }

        self.emit("SupportSelect", "助战刷新需要确认，点击确定");
        if !self.tap_at("SupportSelect", SUPPORT_REFRESH_CONFIRM_BUTTON) {
            return false;
        }

        let dismiss_deadline = std::time::Instant::now() + SUPPORT_REFRESH_DIALOG_DISMISS_TIMEOUT;
        loop {
            if self.is_cancelled() {
                return false;
            }
            match self.sidecar().find_element_by_name(
                None,
                SUPPORT_SELECT_SCREEN,
                SUPPORT_REFRESH_DIALOG_ELEMENT,
            ) {
                Ok(m) if !m.found => {
                    thread::sleep(SUPPORT_REFRESH_DIALOG_POLL);
                    return true;
                }
                Ok(_) => {}
                Err(e)
                    if is_unknown_element_error(
                        &e,
                        SUPPORT_SELECT_SCREEN,
                        SUPPORT_REFRESH_DIALOG_ELEMENT,
                    ) =>
                {
                    thread::sleep(SUPPORT_REFRESH_SETTLE);
                    return true;
                }
                Err(e) => {
                    eprintln!("[runner] refresh dialog disappearance probe failed: {e}");
                    thread::sleep(SUPPORT_REFRESH_SETTLE);
                    return true;
                }
            }
            if std::time::Instant::now() >= dismiss_deadline {
                self.emit("SupportSelect", "等待助战刷新确认关闭超时");
                return false;
            }
            thread::sleep(SUPPORT_REFRESH_DIALOG_POLL);
        }
    }

    /// Legacy "tap the top of the list" path used when the project hasn't
    /// pinned a support servant. Mirrors the pre-OCR behaviour so existing
    /// projects don't regress; new setups should pin a servant via the
    /// team-builder support slot to engage the OCR-based flow above.
    pub(crate) fn legacy_pick_first_support(&mut self) {
        if let Some(name) = self.config.support_servant_name.clone() {
            let region = NormRect {
                x: 0.0,
                y: 0.15,
                w: 1.0,
                h: 0.75,
            };
            if let Ok(Some(pos)) = self.sidecar().find_element(None, &name, region, 0.8) {
                self.emit("SupportSelect", &format!("找到助战从者: {name}"));
                if !self.tap_at("SupportSelect", pos) {
                    return;
                }
                self.support_selected = true;
                self.support_scroll_count = 0;
                thread::sleep(ACTION_DELAY);
                return;
            }
            // Template miss → fall through to scroll/refresh below.
            if self.support_scroll_count < self.config.max_support_scrolls {
                if !self.swipe_at(
                    "SupportSelect",
                    Point::new(0.50, 0.70),
                    Point::new(0.50, 0.30),
                    300,
                ) {
                    return;
                }
                self.support_scroll_count += 1;
                thread::sleep(ACTION_DELAY);
            } else {
                if !self.tap_at("SupportSelect", Point::new(0.92, 0.08)) {
                    return;
                }
                self.support_scroll_count = 0;
                thread::sleep(Duration::from_secs(2));
            }
            return;
        }

        self.emit("SupportSelect", "选择第一个助战从者");
        if !self.tap_at("SupportSelect", Point::new(0.50, 0.35)) {
            return;
        }
        self.support_selected = true;
        thread::sleep(ACTION_DELAY);
    }
}
