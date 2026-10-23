//! Support-list scrolling, exhaustion probes, refresh flow, and legacy selection.

use super::*;

impl Runner {
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

    /// Probe both thumb tips across the complete track on one frozen frame.
    /// The list may already be between its endpoints when automation starts.
    pub(crate) fn support_scroll_bar_at_end(&mut self) -> AutomationResult<bool> {
        let snapshot = tempfile::tempdir().context("创建助战滚动条截图目录失败")?;
        let image_path = snapshot.path().join("support-scrollbar.png");
        let png = match self.sidecar().get_frame_png(0.0).recoverable()? {
            Ok(png) => png,
            Err(error) => {
                self.support_scroll_end_progress = SupportScrollEndProgress::default();
                self.emit_debug(
                    "SupportSelect",
                    &format!("滚动条截图失败，继续查找: {error}"),
                );
                return Ok(false);
            }
        };
        std::fs::write(&image_path, png).context("写入助战滚动条截图失败")?;
        let mut matches = Vec::new();
        for element in [
            SUPPORT_SCROLL_THUMB_START_ELEMENT,
            SUPPORT_SCROLL_THUMB_END_ELEMENT,
            SUPPORT_SCROLL_END_ELEMENT,
        ] {
            match self
                .sidecar()
                .find_element_by_name(Some(&image_path), SUPPORT_SELECT_SCREEN, element)
                .recoverable()?
            {
                Ok(result) => matches.push(result),
                Err(error) => {
                    self.support_scroll_end_progress = SupportScrollEndProgress::default();
                    self.emit_debug(
                        "SupportSelect",
                        &format!("滚动条识别失败，继续查找: {error}"),
                    );
                    return Ok(false);
                }
            }
        }
        let decision = self.support_scroll_end_progress.observe(
            &matches[0],
            &matches[1],
            matches[2].found,
            self.support_scroll_count,
        );
        self.emit_debug(
            "SupportSelect",
            &format!(
                "滚动条: 上端={:.3} 下端={:.3} 底部={:.3} 判定={decision:?}",
                matches[0].score, matches[1].score, matches[2].score,
            ),
        );
        Ok(matches!(
            decision,
            SupportScrollEndDecision::AtEnd | SupportScrollEndDecision::NoScrollbar
        ))
    }

    pub(crate) fn wait_for_support_refresh_available(&mut self) -> AutomationResult<bool> {
        let deadline = Instant::now() + SUPPORT_REFRESH_AVAILABLE_TIMEOUT;
        let mut emitted_wait = false;
        loop {
            if self.is_cancelled() {
                return Ok(false);
            }
            match self
                .sidecar()
                .find_element_by_name(
                    None,
                    SUPPORT_SELECT_SCREEN,
                    SUPPORT_REFRESH_AVAILABLE_ELEMENT,
                )
                .recoverable()?
            {
                Ok(m) if m.found => {
                    eprintln!("[runner] support-refresh score={:.3} -> available", m.score);
                    return Ok(true);
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
                        &e.to_string(),
                        SUPPORT_SELECT_SCREEN,
                        SUPPORT_REFRESH_AVAILABLE_ELEMENT,
                    ) =>
                {
                    eprintln!(
                        "[runner] support-refresh availability probe not configured; tapping directly"
                    );
                    return Ok(true);
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
                return Ok(false);
            }
            thread::sleep(SUPPORT_REFRESH_AVAILABLE_POLL);
        }
    }

    /// Call after this page's eligible Grand candidates have been checked.
    pub(crate) fn support_grand_page_exhausted(&self, probe: &SupportGrandPageProbe) -> bool {
        self.emit_debug("SupportSelect", &support_grand_section_probe_message(probe));
        probe.exhausted()
    }

    /// After tapping the support-list refresh button, confirm the modal when
    /// the server's cv.json defines one. Servers without a modal template
    /// fall back to the legacy fixed settle delay.
    pub(crate) fn confirm_support_refresh_dialog_if_needed(&mut self) -> AutomationResult<bool> {
        let appear_deadline = std::time::Instant::now() + SUPPORT_REFRESH_DIALOG_APPEAR_TIMEOUT;
        loop {
            if self.is_cancelled() {
                return Ok(false);
            }
            match self
                .sidecar()
                .find_element_by_name(None, SUPPORT_SELECT_SCREEN, SUPPORT_REFRESH_DIALOG_ELEMENT)
                .recoverable()?
            {
                Ok(m) if m.found => break,
                Ok(_) => {}
                Err(e)
                    if is_unknown_element_error(
                        &e.to_string(),
                        SUPPORT_SELECT_SCREEN,
                        SUPPORT_REFRESH_DIALOG_ELEMENT,
                    ) =>
                {
                    thread::sleep(SUPPORT_REFRESH_SETTLE);
                    return Ok(true);
                }
                Err(e) => {
                    eprintln!("[runner] refresh dialog probe failed (treating as absent): {e}");
                    break;
                }
            }
            if std::time::Instant::now() >= appear_deadline {
                thread::sleep(SUPPORT_REFRESH_SETTLE);
                return Ok(true);
            }
            thread::sleep(SUPPORT_REFRESH_DIALOG_POLL);
        }

        thread::sleep(SUPPORT_REFRESH_DIALOG_CONFIRM_SETTLE);
        if self.is_cancelled() {
            return Ok(false);
        }

        self.emit("SupportSelect", "助战刷新需要确认，点击确定");
        if !self.tap_at("SupportSelect", SUPPORT_REFRESH_CONFIRM_BUTTON) {
            return Ok(false);
        }

        let dismiss_deadline = std::time::Instant::now() + SUPPORT_REFRESH_DIALOG_DISMISS_TIMEOUT;
        loop {
            if self.is_cancelled() {
                return Ok(false);
            }
            match self
                .sidecar()
                .find_element_by_name(None, SUPPORT_SELECT_SCREEN, SUPPORT_REFRESH_DIALOG_ELEMENT)
                .recoverable()?
            {
                Ok(m) if !m.found => {
                    thread::sleep(SUPPORT_REFRESH_DIALOG_POLL);
                    return Ok(true);
                }
                Ok(_) => {}
                Err(e)
                    if is_unknown_element_error(
                        &e.to_string(),
                        SUPPORT_SELECT_SCREEN,
                        SUPPORT_REFRESH_DIALOG_ELEMENT,
                    ) =>
                {
                    thread::sleep(SUPPORT_REFRESH_SETTLE);
                    return Ok(true);
                }
                Err(e) => {
                    eprintln!("[runner] refresh dialog disappearance probe failed: {e}");
                    thread::sleep(SUPPORT_REFRESH_SETTLE);
                    return Ok(true);
                }
            }
            if std::time::Instant::now() >= dismiss_deadline {
                self.emit("SupportSelect", "等待助战刷新确认关闭超时");
                return Ok(false);
            }
            thread::sleep(SUPPORT_REFRESH_DIALOG_POLL);
        }
    }

    /// Legacy "tap the top of the list" path used when the project hasn't
    /// pinned a support servant. Mirrors the pre-OCR behaviour so existing
    /// projects don't regress; new setups should pin a servant via the
    /// team-builder support slot to engage the OCR-based flow above.
    pub(crate) fn legacy_pick_first_support(&mut self) -> AutomationResult<()> {
        if let Some(name) = self.config.support_servant_name.clone() {
            let region = NormRect {
                x: 0.0,
                y: 0.15,
                w: 1.0,
                h: 0.75,
            };
            if let Ok(Some(pos)) = self
                .sidecar()
                .find_element(None, &name, region, 0.8)
                .recoverable()?
            {
                self.emit("SupportSelect", &format!("找到助战从者: {name}"));
                if !self.tap_at("SupportSelect", pos) {
                    return Ok(());
                }
                self.support_selected = true;
                self.support_scroll_count = 0;
                thread::sleep(ACTION_DELAY);
                return Ok(());
            }
            // Template miss → fall through to scroll/refresh below.
            if self.support_scroll_count < self.config.max_support_scrolls {
                if !self.swipe_at(
                    "SupportSelect",
                    Point::new(0.50, 0.70),
                    Point::new(0.50, 0.30),
                    300,
                ) {
                    return Ok(());
                }
                self.support_scroll_count += 1;
                thread::sleep(ACTION_DELAY);
            } else {
                if !self.tap_at("SupportSelect", Point::new(0.92, 0.08)) {
                    return Ok(());
                }
                self.support_scroll_count = 0;
                thread::sleep(Duration::from_secs(2));
            }
            return Ok(());
        }

        self.emit("SupportSelect", "选择第一个助战从者");
        if !self.tap_at("SupportSelect", Point::new(0.50, 0.35)) {
            return Ok(());
        }
        self.support_selected = true;
        thread::sleep(ACTION_DELAY);
        Ok(())
    }
}
