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
