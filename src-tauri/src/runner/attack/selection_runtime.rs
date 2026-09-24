//! Attack-card reads, taps, and stuck-selection recovery.

use super::*;

impl Runner {
    /// Read the bottom NP gauges while the Battle screen is still visible.
    /// A one-second median-sample window avoids treating a transient dialogue
    /// overlay or visual effect as the final readiness state.
    pub(crate) fn read_noble_phantasm_gauges(
        &mut self,
        screen: &str,
        use_digit_readiness: bool,
    ) -> Option<Vec<NoblePhantasmMatch>> {
        loop {
            let started = Instant::now();
            let mut samples: Vec<Vec<NoblePhantasmMatch>> = Vec::new();

            loop {
                let sample = match if use_digit_readiness {
                    self.sidecar().find_battle_noble_phantasms(None, None)
                } else {
                    self.sidecar().find_noble_phantasms(None, None)
                } {
                    Ok(n) => n,
                    Err(err) => {
                        self.fail_action(screen, "读取宝具数字", err);
                        return None;
                    }
                };

                samples.push(sample);

                if self.is_cancelled() {
                    return None;
                }
                if np_gauge_sample_window_complete(started.elapsed(), samples.len()) {
                    break;
                }
                thread::sleep(NP_GAUGE_SAMPLE_INTERVAL);
            }

            let nps = if use_digit_readiness {
                aggregate_np_gauge_digit_samples(&samples)
            } else {
                aggregate_np_gauge_samples(&samples)
            };
            self.emit_np_recognition_diagnostics(screen, &nps);
            let read_complete = if use_digit_readiness {
                np_gauge_digit_read_complete(&nps)
            } else {
                np_gauge_read_complete(&nps)
            };
            if read_complete {
                return Some(nps);
            }
            if self.is_cancelled() {
                return None;
            }
            self.emit(screen, "宝具数字未识别完整，等待遮挡消失后重试");
            thread::sleep(ACTION_DELAY);
        }
    }

    /// Capture NP readiness before the attack button changes the screen to
    /// the command-card view. The result is consumed by `read_attack_state`.
    pub(crate) fn prepare_noble_phantasm_gauge_before_attack(&mut self) -> bool {
        if !self.current_attack_requires_np_recognition() {
            self.battle.pre_attack_nps = None;
            return true;
        }
        if !reads_np_gauge_before_attack(self.config.noble_phantasm_detection_mode) {
            return true;
        }

        self.battle.pre_attack_nps = None;
        if !self.battle.pre_attack_np_warning_emitted {
            self.emit_warn(
                "Battle",
                "当前使用攻击前宝具条识别，从者台词可能遮挡识别结果，请关闭台词",
            );
            self.battle.pre_attack_np_warning_emitted = true;
        }
        let Some(nps) = self.read_noble_phantasm_gauges("Battle", true) else {
            return false;
        };
        self.battle.pre_attack_nps = Some(nps);
        true
    }

    pub(crate) fn pick_and_tap_attack_cards(
        &mut self,
        cards: &[CommandCardMatch],
        nps: &[NoblePhantasmMatch],
        party_ids: &[Option<u32>; 3],
        party_supports: &[bool; 3],
        attack_priority_override: Option<&[AttackCard]>,
    ) {
        let mut used_card_slots: HashSet<u32> = HashSet::new();
        let mut used_np_slots: HashSet<u32> = HashSet::new();
        let mut picks: Vec<Pick> = Vec::with_capacity(3);
        let actionable_cards = actionable_command_cards(cards);

        if let Some(priority) = attack_priority_override {
            picks = pick_by_priority_with_crit(
                priority,
                &actionable_cards,
                nps,
                party_ids,
                party_supports,
                &mut used_card_slots,
                &mut used_np_slots,
                self.config.prefer_higher_critical_chance,
            );
        } else if let Some(priority) = attack_priority_for_current_scene(
            self.advanced_mode,
            self.battle.scene_config_used,
            &self.scenes,
            self.battle.current_scene_index,
            self.battle.current_turn_index,
        ) {
            picks = pick_by_priority_with_crit(
                priority,
                &actionable_cards,
                nps,
                party_ids,
                party_supports,
                &mut used_card_slots,
                &mut used_np_slots,
                self.config.prefer_higher_critical_chance,
            );
        } else {
            self.emit("Attack", "场景未变更，按默认顺序补位");
        }

        if picks.len() < 3 {
            fill_remaining(
                &mut picks,
                &actionable_cards,
                &mut used_card_slots,
                self.config.prefer_higher_critical_chance,
            );
        }
        if picks.len() < 3 {
            fill_remaining(
                &mut picks,
                cards,
                &mut used_card_slots,
                self.config.prefer_higher_critical_chance,
            );
        }

        if picks.is_empty() {
            self.emit("Attack", "未能选出任何卡，跳过");
            self.battle.scene_config_used = false;
            return;
        }

        self.tap_picks("Attack", &picks, cards, party_ids);
    }

    pub(crate) fn tap_picks(
        &mut self,
        screen: &str,
        picks: &[Pick],
        cards: &[CommandCardMatch],
        party_ids: &[Option<u32>; 3],
    ) {
        if picks.is_empty() {
            self.emit(screen, "未能选出任何卡，跳过");
            self.battle.scene_config_used = false;
            return;
        }

        let mut pending: VecDeque<Pick> = picks.iter().cloned().collect();
        let mut replacements = replacement_command_card_picks_with_crit(
            picks,
            cards,
            self.config.prefer_higher_critical_chance,
        );
        while pending.len() < 3 {
            let Some(replacement) = replacements.pop_front() else {
                self.fail_action(screen, "补足选卡", "没有其他可用指令卡".into());
                return;
            };
            pending.push_back(replacement);
        }

        let mut selected_count = 0usize;
        let mut submitted_picks = Vec::with_capacity(3);
        while selected_count < 3 {
            let Some(pick) = pending.pop_front() else {
                self.fail_action(screen, "补足选卡", "没有其他可用指令卡".into());
                return;
            };
            let is_np = matches!(&pick, Pick::Np { .. });
            let submitted_pick = pick.clone();
            let (msg, point, selected_pick) = match pick {
                Pick::Card {
                    slot,
                    point,
                    servant_id,
                    suit,
                    from_priority,
                } => {
                    let label = match from_priority.as_deref() {
                        Some(p) => format!("选择 {p} → C{}", slot + 1),
                        None => format!("补位: C{}", slot + 1),
                    };
                    let detail = format!(
                        " ({}{})",
                        suit.as_deref().unwrap_or("?"),
                        servant_id.map(|id| format!("/{id}")).unwrap_or_default(),
                    );
                    (
                        format!("{}/3 {}{}", selected_count + 1, label, detail),
                        point,
                        AttackLogSelectedPick {
                            step: selected_count + 1,
                            total: 3,
                            from_priority,
                            kind: AttackLogPickKind::Card,
                            slot,
                            suit,
                            servant_id,
                        },
                    )
                }
                Pick::Np {
                    slot,
                    point,
                    from_priority,
                } => (
                    format!(
                        "{}/3 选择 {} → NP{}",
                        selected_count + 1,
                        from_priority,
                        slot + 1,
                    ),
                    point,
                    AttackLogSelectedPick {
                        step: selected_count + 1,
                        total: 3,
                        from_priority: Some(from_priority),
                        kind: AttackLogPickKind::Np,
                        slot,
                        suit: None,
                        servant_id: party_ids.get(slot as usize).copied().flatten(),
                    },
                ),
            };
            if screen == "Attack" {
                self.emit_attack(
                    &msg,
                    AttackLogMeta {
                        front_servant_ids: *party_ids,
                        candidate_servant_ids: None,
                        command_cards: None,
                        ready_np_slots: None,
                        selected_pick: Some(selected_pick),
                    },
                );
            } else {
                self.emit(screen, &msg);
            }
            if !self.tap_pick_with_stuck_selection_test(screen, point, selected_count) {
                return;
            }
            thread::sleep(ACTION_DELAY);

            if is_np {
                let cannot_use_np = match self.sidecar().find_element_by_name(
                    None,
                    ATTACK_SCREEN,
                    CANNOT_USE_NP_CLOSE_BUTTON_ELEMENT,
                ) {
                    Ok(matched) => matched.found,
                    Err(err) => {
                        self.fail_action(screen, "检测宝具是否可用", err);
                        return;
                    }
                };
                if cannot_use_np {
                    self.emit_warn(screen, "宝具无法使用，关闭提示并改用其他指令卡");
                    if !self.tap_at(screen, CANNOT_USE_NP_CLOSE_POINT) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    let Some(replacement) = replacements.pop_front() else {
                        self.fail_action(screen, "替换无法使用的宝具", "没有其他可用指令卡".into());
                        return;
                    };
                    pending.push_back(replacement);
                    continue;
                }
            }

            submitted_picks.push(submitted_pick);
            selected_count += 1;
        }

        self.last_attack_plan = Some(AttackRetryPlan {
            picks: submitted_picks,
            cards: cards.to_vec(),
            party_ids: *party_ids,
        });
        self.battle
            .transition(BattleFlowEvent::AttackCardsSubmitted { at: Instant::now() });

        // Reset for next cycle
        self.battle.scene_config_used = false;
    }

    fn read_noble_phantasms_for_retry(&mut self) -> Option<Vec<NoblePhantasmMatch>> {
        match self.config.noble_phantasm_detection_mode {
            NoblePhantasmDetectionMode::Card => loop {
                let mut nps = match self.sidecar().find_noble_phantasms(None, None) {
                    Ok(n) => n,
                    Err(err) => {
                        self.fail_action("Attack", "重试时识别宝具卡", err);
                        return None;
                    }
                };
                self.emit_np_recognition_diagnostics("Attack", &nps);
                apply_np_detection_mode(&mut nps, NoblePhantasmDetectionMode::Card);
                if np_card_read_complete(&nps) {
                    return Some(nps);
                }
                if self.is_cancelled() {
                    return None;
                }
                self.emit("Attack", "重试时宝具卡尚未识别完整，等待卡面稳定后重试");
                thread::sleep(ACTION_DELAY);
            },
            NoblePhantasmDetectionMode::Gauge => self.read_noble_phantasm_gauges("Attack", false),
            NoblePhantasmDetectionMode::GaugeBeforeAttack => {
                let nps = self.battle.pre_attack_nps.clone();
                if nps.is_none() {
                    self.fail_action(
                        "Attack",
                        "重试时读取攻击前宝具条",
                        "未找到重新识别的宝具条结果".into(),
                    );
                }
                nps
            }
        }
    }

    /// Recover a chain whose three issued taps left the game on the Attack
    /// screen. The command-card hand is reused after returning to Battle, but
    /// NP readiness is re-read because the original result may be stale.
    pub(crate) fn recover_stuck_attack_selection(&mut self) {
        let Some(plan) = self.last_attack_plan.clone() else {
            self.emit_warn("Attack", "选卡未完成，但没有可复用的选卡记录");
            return;
        };

        self.emit_warn(
            "Attack",
            "选卡提交后仍停留在指令卡画面，返回并复用本轮选卡重试",
        );
        if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
            return;
        }
        thread::sleep(ACTION_DELAY);
        if !self.wait_for_attack_button("Battle", ATTACK_RETRY_NAVIGATION_TIMEOUT) {
            self.emit_warn("Attack", "选卡重试返回 Battle 超时，稍后继续恢复");
            return;
        }

        self.battle.transition(BattleFlowEvent::BattleActionable);
        if !self.tap_attack_button() {
            return;
        }
        if !self.wait_for_retry_attack_screen_stable(ATTACK_RETRY_NAVIGATION_TIMEOUT) {
            self.emit_warn("Attack", "选卡重试等待指令卡画面稳定超时");
            return;
        }
        self.battle
            .transition(BattleFlowEvent::AttackScreenDetected);
        thread::sleep(ATTACK_RETRY_SCREEN_SETTLE);

        let Some(nps) = self.read_noble_phantasms_for_retry() else {
            return;
        };
        let retry_picks = refresh_retry_picks_for_np_state(
            &plan.picks,
            &plan.cards,
            &nps,
            self.config.prefer_higher_critical_chance,
        );
        if retry_picks.len() < 3 {
            self.fail_action(
                "Attack",
                "重试选卡",
                "宝具不可用且没有足够的指令卡补位".into(),
            );
            return;
        }
        self.emit("Attack", "指令卡画面已稳定，已重新识别宝具状态并重试选卡");
        self.tap_picks("Attack", &retry_picks, &plan.cards, &plan.party_ids);
        if !self.battle.awaiting_attack_resolution() {
            return;
        }

        if self.confirm_retried_attack_submission(ATTACK_SUBMISSION_STUCK_TIMEOUT) {
            self.emit("Attack", "已确认重试选卡成功");
        } else {
            self.emit_warn("Attack", "重试选卡后仍未离开指令卡画面，将再次恢复");
        }
    }

    fn wait_for_retry_attack_screen_stable(&mut self, timeout: Duration) -> bool {
        let started_at = Instant::now();
        let mut consecutive_attack_polls = 0u32;
        loop {
            if self.is_cancelled() {
                return false;
            }
            match self.sidecar().detect(None) {
                Ok(Screen::Attack) => {
                    consecutive_attack_polls += 1;
                    if consecutive_attack_polls >= ATTACK_RETRY_SCREEN_STABLE_POLLS {
                        return true;
                    }
                }
                Ok(_) | Err(_) => consecutive_attack_polls = 0,
            }
            if started_at.elapsed() >= timeout {
                return false;
            }
            thread::sleep(SKILL_POLL_INTERVAL);
        }
    }

    fn confirm_retried_attack_submission(&mut self, timeout: Duration) -> bool {
        let started_at = Instant::now();
        let mut consecutive_non_attack_polls = 0u32;
        loop {
            if self.is_cancelled() {
                return false;
            }
            match self.sidecar().detect(None) {
                Ok(Screen::Attack) => consecutive_non_attack_polls = 0,
                Ok(_) => {
                    consecutive_non_attack_polls += 1;
                    if consecutive_non_attack_polls >= 2 {
                        return true;
                    }
                }
                Err(_) => consecutive_non_attack_polls = 0,
            }
            if started_at.elapsed() >= timeout {
                return false;
            }
            thread::sleep(SKILL_POLL_INTERVAL);
        }
    }
}
