//! Shared runner runtime helpers.
//!
//! These methods provide sidecar access, status emission, cancellation checks,
//! and touch/wait primitives used by the domain modules. Keeping them together
//! makes the domain modules depend on one small runtime surface instead of each
//! owning its own ADB or sidecar plumbing.

use super::*;

impl Runner {
    // -- helpers -------------------------------------------------------------

    pub(crate) fn transition_lifecycle(
        &self,
        event: RunnerLifecycleEvent,
    ) -> RunnerLifecycleTransition {
        let mut state = self.state.lock().unwrap();
        let transition = runner_lifecycle_transition(state.clone(), event);
        if transition.accepted {
            *state = transition.next.clone();
        }
        transition
    }

    pub(crate) fn emit(&self, screen: &str, message: &str) {
        self.emit_with_level(screen, message, LogLevel::Info);
    }

    pub(crate) fn emit_warn(&self, screen: &str, message: &str) {
        self.emit_with_level(screen, message, LogLevel::Warn);
    }

    /// Like [`emit`] but at `LogLevel::Debug`. Use for technical
    /// diagnostics that the user doesn't normally want to see — they
    /// stay hidden behind the operation-log "显示调试" toggle in the
    /// status bar. Keep these messages compact (a single line) since
    /// the panel doesn't wrap long entries gracefully.
    pub(crate) fn emit_debug(&self, screen: &str, message: &str) {
        self.emit_with_level(screen, message, LogLevel::Debug);
    }

    /// Local-only diagnostics for details that are useful during
    /// development but too noisy for packaged builds.
    pub(crate) fn emit_local_debug(&self, screen: &str, message: &str) {
        #[cfg(debug_assertions)]
        self.emit_with_level(screen, message, LogLevel::LocalDebug);

        #[cfg(not(debug_assertions))]
        let _ = (screen, message);
    }

    pub(crate) fn emit_with_level(&self, screen: &str, message: &str, level: LogLevel) {
        self.emit_with_level_and_meta(screen, message, level, None, None);
    }

    pub(crate) fn emit_attack(&self, message: &str, attack: AttackLogMeta) {
        self.emit_with_level_and_meta("Attack", message, LogLevel::Info, Some(attack), None);
    }

    pub(crate) fn emit_action(&self, message: &str, action: ActionLogMeta) {
        self.emit_with_level_and_meta("Battle", message, LogLevel::Info, None, Some(action));
    }

    pub(crate) fn emit_with_level_and_meta(
        &self,
        screen: &str,
        message: &str,
        level: LogLevel,
        attack: Option<AttackLogMeta>,
        action: Option<ActionLogMeta>,
    ) {
        let (state_str, status) = {
            let s = self.state.lock().unwrap();
            (format!("{:?}", *s), s.status())
        };
        let _ = self.app_handle.emit(
            "automation-status",
            AutomationEvent {
                state: state_str,
                status,
                current_screen: screen.into(),
                message: message.into(),
                level,
                attack,
                action,
            },
        );
    }

    pub(crate) fn sidecar(&mut self) -> &mut SidecarClient {
        self.sidecar.as_mut().expect("runner sidecar missing")
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub(crate) fn should_stop_after_current(&self) -> bool {
        self.stop_after_current.load(Ordering::Relaxed)
    }

    pub(crate) fn fail_action(&self, screen: &str, action: &str, err: String) {
        let message = format!("{action}失败: {err}");
        self.transition_lifecycle(RunnerLifecycleEvent::Failed {
            message: message.clone(),
        });
        self.emit(screen, &message);
    }

    pub(crate) fn tap_at(&mut self, screen: &str, point: Point) -> bool {
        let (px, py) = point.to_physical(self.screen_w, self.screen_h);
        let (jx, jy) = jitter_offset();
        // Saturate at the screen edges so a near-edge button still
        // registers even if the jitter would push it off-screen.
        let tap_x = (px as i32 + jx).clamp(0, self.screen_w.saturating_sub(1) as i32) as u32;
        let tap_y = (py as i32 + jy).clamp(0, self.screen_h.saturating_sub(1) as i32) as u32;
        match self.touch.tap(tap_x, tap_y) {
            Ok(()) => true,
            Err(err) => {
                self.fail_action(screen, "点击", err);
                false
            }
        }
    }

    pub(crate) fn tap_attack_button(&mut self) -> bool {
        if !self.tap_at("Battle", ATTACK_BUTTON) {
            return false;
        }
        self.battle
            .transition(BattleFlowEvent::AttackButtonTapped { at: Instant::now() });
        true
    }

    pub(crate) fn swipe_at(
        &mut self,
        screen: &str,
        from: Point,
        to: Point,
        duration_ms: u32,
    ) -> bool {
        let from_px = from.to_physical(self.screen_w, self.screen_h);
        let to_px = to.to_physical(self.screen_w, self.screen_h);
        match self.touch.swipe(from_px, to_px, duration_ms) {
            Ok(()) => true,
            Err(err) => {
                self.fail_action(screen, "滑动", err);
                false
            }
        }
    }

    /// "Press, drag, hold, release" swipe via the active `TouchBackend`.
    /// Use this for any swipe whose precise stopping position matters
    /// — the settle window prevents Android's fling momentum from
    /// continuing the scroll past the lift-off coordinate.
    pub(crate) fn swipe_with_settle_at(
        &mut self,
        screen: &str,
        from: Point,
        to: Point,
        swipe_ms: u32,
        settle_ms: u32,
    ) -> bool {
        let from_px = from.to_physical(self.screen_w, self.screen_h);
        let to_px = to.to_physical(self.screen_w, self.screen_h);
        match self
            .touch
            .swipe_with_settle(from_px, to_px, swipe_ms, settle_ms)
        {
            Ok(()) => true,
            Err(err) => {
                self.fail_action(screen, "滑动", err);
                false
            }
        }
    }

    /// Tap `point` repeatedly, every `interval`, until either
    /// `sidecar.detect()` returns a screen different from `from_screen`,
    /// `timeout` elapses, or the user cancels the run.
    ///
    /// The bond and EXP result pages in particular have a ~3–5 s
    /// per-servant level-up / bond animation that absorbs the very first
    /// tap silently — the page stays mounted until the animation
    /// completes, so the legacy "one tap per main-loop poll" cadence
    /// (~one tap every 800 ms) wastes most of those taps and leaves the
    /// runner sitting on the result page much longer than necessary.
    /// Tapping inside the handler at a tighter cadence and bailing the
    /// instant the screen actually changes drops the average bond-screen
    /// dwell from ~6 s to under 2 s on common quests.
    ///
    /// Returns ``true`` when the screen changed away from `from_screen`,
    /// ``false`` on timeout, cancellation, or ADB tap failure (which is
    /// already reported via `fail_action` from `tap_at`).
    pub(crate) fn tap_until_screen_changes(
        &mut self,
        screen_label: &str,
        from_screen: Screen,
        point: Point,
        interval: Duration,
        timeout: Duration,
    ) -> bool {
        let start = std::time::Instant::now();
        let mut taps: u32 = 0;
        loop {
            if self.is_cancelled() {
                return false;
            }
            if !self.tap_at(screen_label, point) {
                return false;
            }
            taps += 1;
            thread::sleep(interval);
            // detect() failures are best-effort here — treat them as
            // "screen unchanged" so we keep tapping rather than bailing.
            // A persistent CV error will surface on the main loop's
            // next iteration via the same call path.
            let detected = self.sidecar().detect(None).unwrap_or(from_screen);
            if detected != from_screen {
                return true;
            }
            if start.elapsed() >= timeout {
                self.emit(
                    screen_label,
                    &format!("等待画面切换超时 (已点击 {taps} 次)"),
                );
                return false;
            }
        }
    }

    pub(crate) fn wait_for_element_visible(
        &mut self,
        screen: &str,
        element: &str,
        timeout: Duration,
        status_text: &str,
        timeout_text: &str,
    ) -> bool {
        self.wait_for_element_state(screen, element, true, timeout, status_text, timeout_text)
    }

    pub(crate) fn wait_for_element_hidden(
        &mut self,
        screen: &str,
        element: &str,
        timeout: Duration,
        status_text: &str,
        timeout_text: &str,
    ) -> bool {
        self.wait_for_element_state(screen, element, false, timeout, status_text, timeout_text)
    }

    pub(crate) fn wait_for_element_state(
        &mut self,
        screen: &str,
        element: &str,
        expected_found: bool,
        timeout: Duration,
        status_text: &str,
        timeout_text: &str,
    ) -> bool {
        let start = std::time::Instant::now();
        let mut tick: u32 = 0;
        loop {
            if self.is_cancelled() {
                return false;
            }
            let found = match self
                .sidecar()
                .find_element_by_name(None, BATTLE_SCREEN, element)
            {
                Ok(matched) => Some(matched.found),
                Err(err) => {
                    eprintln!(
                        "[runner] find_element_by_name({BATTLE_SCREEN}.{element}) failed: {err}"
                    );
                    None
                }
            };
            if found == Some(expected_found) {
                return true;
            }
            if start.elapsed() >= timeout {
                self.emit_warn(screen, timeout_text);
                return false;
            }
            tick += 1;
            if tick % 4 == 1 {
                self.emit_debug(screen, status_text);
            }
            thread::sleep(SKILL_POLL_INTERVAL);
        }
    }

    /// Block until the attack-button probe in `cv.json` matches, polling
    /// every `SKILL_POLL_INTERVAL`. Used after firing a skill so
    /// the next tap doesn't land during the cut-in / animation while the
    /// button is hidden.
    ///
    /// Returns ``true`` when the button is detected, ``false`` on timeout
    /// or cancellation. Emits status updates so the user can see the wait.
    pub(crate) fn wait_for_attack_button(&mut self, screen: &str, timeout: Duration) -> bool {
        self.wait_for_element_visible(
            screen,
            ATTACK_BUTTON_ELEMENT,
            timeout,
            "等待技能动画结束…",
            "等待攻击按钮超时",
        )
    }
}
