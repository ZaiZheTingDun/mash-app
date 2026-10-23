//! Device-facing support selection orchestration.
//!
//! Matching, filtering, and region policies remain in the parent module; this
//! module owns Sidecar probes, list navigation, taps, and refresh lifecycle.

use super::*;

impl Runner {
    pub(crate) fn handle_support_select(&mut self) -> AutomationResult<()> {
        // No servant pinned → preserve the legacy top-row pick unless a CE
        // is configured, in which case the CE alone identifies the row.
        let Some(servant_id) = self.config.support_servant_id else {
            if self.support_ce_filter_enabled() {
                self.handle_support_ce_only_select()?;
            } else {
                self.legacy_pick_first_support()?;
            }
            return Ok(());
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
                    return Ok(());
                }
            }
        }
        let meta = self.support_meta.clone().unwrap();

        // Apply the current search stage before OCR. Both mode scans the
        // corresponding class first, then the leftmost ALL slot before refresh.
        if !self.support_class_search.tab_done {
            match self.support_class_search.filter_action(
                self.server,
                Some(&meta.class_name),
                self.config.support_class_filter_mode,
                self.config.enable_extra_class_filter,
                self.support_extra_class_filter_configured,
            ) {
                Some(SupportClassFilterAction::Tap(tab)) => {
                    self.emit(
                        "SupportSelect",
                        &format!(
                            "切换职阶筛选 -> {}",
                            if tab.x == SUPPORT_TAB_ALL.x {
                                "全职阶"
                            } else {
                                &meta.class_name
                            },
                        ),
                    );
                    if !self.tap_at("SupportSelect", tab) {
                        return Ok(());
                    }
                    self.support_class_search.tab_done = true;
                    thread::sleep(SUPPORT_CLASS_TAB_SETTLE);
                    return Ok(());
                }
                Some(SupportClassFilterAction::CnExtra(extra)) => {
                    self.emit(
                        "SupportSelect",
                        &format!("设置 EXTRA 具体职阶筛选 -> {}", extra.label),
                    );
                    if !self.configure_cn_extra_class_filter(extra)? {
                        return Ok(());
                    }
                    self.support_extra_class_filter_configured = true;
                    self.support_class_search.tab_done = true;
                    thread::sleep(SUPPORT_EXTRA_FILTER_RESULT_SETTLE);
                    return Ok(());
                }
                None => {
                    // Unknown class → mark done so we don't loop, and let
                    // the OCR pass run against whatever tab is active.
                    eprintln!(
                        "[runner] no class-tab mapping for className='{}', skipping filter",
                        meta.class_name,
                    );
                    self.support_class_search.tab_done = true;
                }
            }
        }

        // OCR the current (class-filtered) screen and look for a row
        // whose name + NP both fuzzy-match the pinned servant.
        let include_support_details =
            support_level_filtering_enabled(self.server) && self.has_support_level_requirements();
        let support_full_list_ocr_fallback = self.config.support_full_list_ocr_fallback;
        let result = match self
            .sidecar()
            .find_supports(
                None,
                &meta.name,
                &meta.names,
                &meta.excluded_names,
                &meta.np_names,
                meta.require_np_match,
                include_support_details,
                support_full_list_ocr_fallback,
                false,
            )
            .recoverable()?
        {
            Ok(r) => r,
            Err(e) => {
                self.release_support_ocr()?;
                self.fail_action("SupportSelect", "OCR 助战识别", e.to_string());
                return Ok(());
            }
        };

        // `supports` is already sorted top-down by the sidecar. Default
        // pick = first match, but optional CE / level filters can reject
        // rows before we tap.
        let grand_page = SupportGrandPageProbe::from_diagnostics(&result.diagnostics);
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
        let mut had_eligible_candidate = false;
        for (index, row) in result.supports.iter().enumerate() {
            if !grand_page.allows_candidate(self.config.support_grand_mode, row) {
                continue;
            }
            had_eligible_candidate = true;
            if let Some(mismatch) = self.support_row_ce_mismatch(row)? {
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
            self.release_support_ocr()?;
            self.emit("SupportSelect", &support_found_summary(&meta.name, row));
            self.emit_debug(
                "SupportSelect",
                &support_found_debug_detail(&meta.name, row),
            );
            if !self.tap_at("SupportSelect", support_row_tap_point(row)) {
                return Ok(());
            }
            self.support_selected = true;
            self.support_scroll_count = 0;
            self.support_refresh_count = 0;
            self.support_class_search = SupportClassSearchProgress::default();
            self.support_level_progress = SupportLevelPanelProgress::default();
            thread::sleep(ACTION_DELAY);
            return Ok(());
        }

        // OCR found rows but the pinned CE didn't match any of them —
        // emit a distinct message before falling through to the
        // scroll/refresh branch so the user knows it's a CE filter miss
        // (vs a name miss).
        if ce_filter_enabled && had_eligible_candidate {
            let reason = if ce_filter_reasons.is_empty() {
                "礼装不匹配".to_string()
            } else {
                ce_filter_reasons.join("；")
            };
            self.emit("SupportSelect", &format!("找到从者但{reason}，继续查找…"));
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
                return Ok(());
            }
            thread::sleep(SUPPORT_SKILL_PANEL_TOGGLE_SETTLE);
            return Ok(());
        }
        if !level_filter_reasons.is_empty() {
            self.emit(
                "SupportSelect",
                &format!(
                    "找到从者但等级不匹配（{}），继续查找…",
                    level_filter_reasons.join("；"),
                ),
            );
        }

        // Finish this page's Grand candidates before acting on an ordinary
        // row boundary. A mixed page ends the scan without another swipe.
        let grand_section_exhausted =
            self.config.support_grand_mode && self.support_grand_page_exhausted(&grand_page);
        // Grand exhaustion must not skip the scrollbar diagnostic probe.
        Ok(
            if !support_list_exhausted(grand_section_exhausted, || {
                self.support_scroll_bar_at_end()
            })? {
                self.emit(
                    "SupportSelect",
                    &support_not_found_scroll_message(
                        &meta.name,
                        self.config.support_grand_mode,
                        self.support_scroll_count + 1,
                    ),
                );
                if !self.scroll_support_list(&result.diagnostics.confirm_button_anchors) {
                    return Ok(());
                }
                self.support_scroll_count += 1;
                self.support_level_progress = SupportLevelPanelProgress::default();
                thread::sleep(SUPPORT_SCROLL_SETTLE);
            } else if self
                .support_class_search
                .advance_to_all_classes(self.config.support_class_filter_mode)
            {
                self.emit(
                    "SupportSelect",
                    "对应职阶未找到匹配助战，切换全职阶继续查找…",
                );
                self.support_scroll_count = 0;
                self.support_level_progress = SupportLevelPanelProgress::default();
            } else if self.support_refresh_count < SUPPORT_MAX_REFRESHES {
                let reason = if grand_section_exhausted {
                    "当前页已出现普通助战，冠位区域已扫完"
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
                if !self.wait_for_support_refresh_available()? {
                    return Ok(());
                }
                if !self.tap_at("SupportSelect", SUPPORT_REFRESH_BUTTON) {
                    return Ok(());
                }
                self.support_scroll_count = 0;
                self.support_refresh_count += 1;
                self.support_level_progress = SupportLevelPanelProgress::default();
                // Refresh occasionally snaps the class filter back to "all";
                // re-tap the class tab on the next poll to be safe.
                self.support_class_search = SupportClassSearchProgress::default();
                if !self.confirm_support_refresh_dialog_if_needed()? {
                    return Ok(());
                }
            } else {
                self.release_support_ocr()?;
                self.fail_action(
                    "SupportSelect",
                    "查找助战",
                    format!("刷新 {} 次仍未找到 {}", SUPPORT_MAX_REFRESHES, meta.name),
                );
            },
        )
    }

    /// Select a support by its equipped Craft Essence when no servant is
    /// pinned. The sidecar returns the visible row anchors without applying
    /// name matching, then the ordinary/grand CE verifier chooses a row.
    pub(crate) fn handle_support_ce_only_select(&mut self) -> AutomationResult<()> {
        let missing_template = if self.config.support_grand_mode {
            let expected = (0..3).find(|&index| {
                !grand_support_ce_ids(&self.config, index).is_empty()
                    && self.resolve_support_grand_ce_templates()[index].is_empty()
            });
            expected.map(|index| format!("冠位礼装 {} 模板不可用", index + 1))
        } else if !ordinary_support_ce_ids(&self.config).is_empty()
            && self.resolve_support_ce_templates().is_empty()
        {
            Some("已配置礼装的识别模板不可用".to_string())
        } else {
            None
        };
        if let Some(reason) = missing_template {
            self.fail_action("SupportSelect", "按礼装查找助战", reason);
            return Ok(());
        }

        let support_full_list_ocr_fallback = self.config.support_full_list_ocr_fallback;
        let result = match self
            .sidecar()
            .find_supports(
                None,
                "",
                &[],
                &[],
                &[],
                false,
                false,
                support_full_list_ocr_fallback,
                true,
            )
            .recoverable()?
        {
            Ok(result) => result,
            Err(error) => {
                self.release_support_ocr()?;
                self.fail_action("SupportSelect", "OCR 助战识别", error.to_string());
                return Ok(());
            }
        };

        let grand_page = SupportGrandPageProbe::from_diagnostics(&result.diagnostics);
        let mut mismatch_reasons = Vec::new();
        let mut mismatch_debug_summaries = Vec::new();
        for (index, row) in result.supports.iter().enumerate() {
            if !grand_page.allows_candidate(self.config.support_grand_mode, row) {
                continue;
            }
            if let Some(mismatch) = self.support_row_ce_mismatch(row)? {
                if !mismatch_reasons.contains(&mismatch.reason) {
                    mismatch_reasons.push(mismatch.reason);
                }
                if let Some(summary) = mismatch.debug_summary {
                    if !mismatch_debug_summaries.contains(&summary) {
                        mismatch_debug_summaries.push(summary);
                    }
                }
                continue;
            }

            self.release_support_ocr()?;
            let label = if row.name_text.is_empty() {
                format!("第 {} 行", index + 1)
            } else {
                row.name_text.clone()
            };
            let mlb_required = if self.config.support_grand_mode {
                (0..3).any(|slot| {
                    !grand_support_ce_ids(&self.config, slot).is_empty()
                        && self.config.support_grand_craft_essence_mlb_required[slot]
                })
            } else {
                self.config.support_craft_essence_mlb_required
            };
            let mlb_summary = if mlb_required {
                "满破检查通过"
            } else {
                "未启用满破要求"
            };
            self.emit(
                "SupportSelect",
                &format!("找到符合礼装条件的助战: {label}（{mlb_summary}）"),
            );
            self.emit_debug("SupportSelect", &format!("礼装匹配助战 {label}"));
            if !self.tap_at("SupportSelect", support_row_tap_point(row)) {
                return Ok(());
            }
            self.support_selected = true;
            self.support_scroll_count = 0;
            self.support_refresh_count = 0;
            thread::sleep(ACTION_DELAY);
            return Ok(());
        }

        if !mismatch_reasons.is_empty() {
            self.emit(
                "SupportSelect",
                &format!(
                    "当前助战礼装不满足要求（{}），继续查找…",
                    mismatch_reasons.join("；")
                ),
            );
            for summary in mismatch_debug_summaries {
                self.emit_debug("SupportSelect", &summary);
            }
        }

        let grand_section_exhausted =
            self.config.support_grand_mode && self.support_grand_page_exhausted(&grand_page);
        // Grand exhaustion must not skip the scrollbar diagnostic probe.
        Ok(
            if !support_list_exhausted(grand_section_exhausted, || {
                self.support_scroll_bar_at_end()
            })? {
                self.emit(
                    "SupportSelect",
                    &format!(
                        "当前{}助战没有匹配礼装，继续滚动… ({})",
                        if self.config.support_grand_mode {
                            "冠位"
                        } else {
                            "普通"
                        },
                        self.support_scroll_count + 1
                    ),
                );
                if !self.scroll_support_list(&result.diagnostics.confirm_button_anchors) {
                    return Ok(());
                }
                self.support_scroll_count += 1;
                thread::sleep(SUPPORT_SCROLL_SETTLE);
            } else if self.support_refresh_count < SUPPORT_MAX_REFRESHES {
                let reason = if grand_section_exhausted {
                    "当前页已出现普通助战，冠位区域已扫完"
                } else {
                    "已到底部"
                };
                self.emit(
                    "SupportSelect",
                    &format!(
                        "{reason}，刷新助战列表 ({}/{})",
                        self.support_refresh_count + 1,
                        SUPPORT_MAX_REFRESHES
                    ),
                );
                if !self.wait_for_support_refresh_available()? {
                    return Ok(());
                }
                if !self.tap_at("SupportSelect", SUPPORT_REFRESH_BUTTON) {
                    return Ok(());
                }
                self.support_scroll_count = 0;
                self.support_refresh_count += 1;
                if !self.confirm_support_refresh_dialog_if_needed()? {
                    return Ok(());
                }
            } else {
                self.release_support_ocr()?;
                self.fail_action(
                    "SupportSelect",
                    "查找助战",
                    format!("刷新 {} 次仍未找到匹配礼装的助战", SUPPORT_MAX_REFRESHES),
                );
            },
        )
    }

    pub(crate) fn release_support_ocr(&mut self) -> AutomationResult<()> {
        Ok(
            if let Err(error) = self.sidecar().release_ocr().recoverable()? {
                eprintln!("[mash-cv] release support-search OCR worker failed: {error}");
            },
        )
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
}
