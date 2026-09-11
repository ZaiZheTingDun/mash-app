//! Main battle automation loop and high-level screen routing.
//!
//! This module keeps the polling loop and Battle screen transition logic together.
//! Domain-specific actions are delegated to support, prebattle, battle result, AP,
//! party, and attack modules so screen routing remains the only responsibility here.

use super::*;

pub(super) fn unknown_screen_wait_message(
    server: Server,
    unknown_count: u32,
    timeout: u32,
) -> String {
    let server_label = match server {
        Server::Cn => "国服",
        Server::Jp => "日服",
    };
    format!("等待识别画面[{server_label}]... ({unknown_count}/{timeout})")
}

pub(super) fn support_search_screen_exited(previous: Screen, current: Screen) -> bool {
    previous == Screen::SupportSelect
        && current != Screen::SupportSelect
        && current != Screen::Unknown
}

impl Runner {
    // -- main loop -----------------------------------------------------------

    pub fn run(mut self) {
        self.transition_lifecycle(RunnerLifecycleEvent::WorkerStarted);
        if let Some(recorder) = &self.run_recorder {
            recorder.mark_running();
        }
        self.emit_run_progress();
        self.emit("", "自动化已启动");

        let mut unknown_count: u32 = 0;
        let mut last_detected_screen = Screen::Unknown;

        loop {
            self.checkpoint_run_statistics_if_due();
            if self.is_cancelled() {
                self.transition_lifecycle(RunnerLifecycleEvent::StopRequested);
                self.emit("", "自动化已停止");
                return;
            }

            let screen = match self.sidecar().detect(None) {
                Ok(s) => s,
                Err(e) => {
                    self.transition_lifecycle(RunnerLifecycleEvent::Failed { message: e.clone() });
                    self.emit("", &format!("画面识别失败: {e}"));
                    return;
                }
            };

            if support_search_screen_exited(last_detected_screen, screen) {
                self.release_support_ocr();
            }

            self.resolve_pending_ap_recovery(screen);

            if screen != Screen::BattleResultContinue {
                self.battle_result_continue_handled = false;
            }
            if screen != Screen::BattleResultLoot {
                self.battle_result_loot_handled = false;
            }
            if screen != Screen::BattleResultFriendRequest {
                self.battle_result_friend_request_handled = false;
            }
            if screen != Screen::BattleResultBond {
                self.battle_result_bond_handled = false;
            }

            match screen {
                Screen::TeamConfirm => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_team_confirm();
                }
                Screen::TeamChange => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_team_change();
                }
                Screen::SupportSelect => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_support_select();
                }
                Screen::ServantSelect => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_servant_select();
                }
                Screen::Battle => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle();
                }
                Screen::Attack => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    if self.battle.awaiting_attack_resolution() {
                        match attack_submission_wait_gate(
                            self.battle.attack_submission_started_at(),
                            Instant::now(),
                            ATTACK_SUBMISSION_STUCK_TIMEOUT,
                        ) {
                            AttackSubmissionWaitGate::TimedOut => {
                                self.recover_stuck_attack_selection();
                            }
                            AttackSubmissionWaitGate::Waiting
                            | AttackSubmissionWaitGate::NotWaiting => {
                                self.emit("Attack", "已提交本轮选卡，等待攻击动画");
                            }
                        }
                    } else {
                        self.battle
                            .transition(BattleFlowEvent::AttackScreenDetected);
                        self.handle_attack();
                    }
                }
                Screen::BattleResultBond => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_bond();
                }
                Screen::BattleResultExp => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_exp();
                }
                Screen::BattleResultLoot => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_loot();
                }
                Screen::BattleResultFriendRequest => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_friend_request();
                }
                Screen::BattleResultContinue => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_continue();
                }
                Screen::APRecovery => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_ap_recovery();
                }
                Screen::Unknown => {
                    unknown_count += 1;
                    let timeout = self.config.unknown_screen_timeout_count;
                    if unknown_count >= timeout {
                        self.transition_lifecycle(RunnerLifecycleEvent::Failed {
                            message: "无法识别当前画面".into(),
                        });
                        if self.config.auto_capture_unknown_screen_timeout {
                            match self.capture_unknown_screen_timeout_screenshot() {
                                Ok(path) => self.emit(
                                    "Unknown",
                                    &format!("无法识别画面截图已保存: {}", path.display()),
                                ),
                                Err(err) => self.emit_warn(
                                    "Unknown",
                                    &format!("无法识别画面截图保存失败: {err}"),
                                ),
                            }
                        }
                        self.emit("Unknown", "无法识别当前画面，已超时停止");
                        return;
                    }
                    self.emit(
                        "Unknown",
                        &unknown_screen_wait_message(self.server, unknown_count, timeout),
                    );
                    if is_battle_result_screen(last_detected_screen) {
                        self.emit_debug("Unknown", "结算页可能被弹窗遮挡，尝试点击跳过区域");
                        if self.tap_at("Unknown", BATTLE_RESULT_POPUP_SKIP) {
                            thread::sleep(ACTION_DELAY);
                        }
                    }
                }
            }

            if matches!(*self.state.lock().unwrap(), RunnerState::Error { .. }) {
                return;
            }

            if matches!(*self.state.lock().unwrap(), RunnerState::Finished) {
                self.emit("", "自动化已完成");
                return;
            }

            thread::sleep(POLL_INTERVAL);
        }
    }

    // -- battle screen handlers ----------------------------------------------

    fn handle_battle(&mut self) {
        match attack_screen_wait_gate(
            self.battle.attack_screen_wait_started_at(),
            Instant::now(),
            ATTACK_SCREEN_WAIT_TIMEOUT,
        ) {
            AttackScreenWaitGate::Waiting => {
                self.emit("Battle", "已点击攻击按钮，等待指令卡画面…");
                return;
            }
            AttackScreenWaitGate::TimedOut => {
                self.battle
                    .transition(BattleFlowEvent::AttackScreenWaitTimedOut);
                self.emit_warn("Battle", "等待指令卡画面超时，恢复 Battle 处理");
            }
            AttackScreenWaitGate::NotWaiting => {}
        }

        // Check if the attack button is present (our turn to act)
        let attack_present = self
            .sidecar()
            .find_element_by_name(None, BATTLE_SCREEN, ATTACK_BUTTON_ELEMENT)
            .map(|m| m.found)
            .unwrap_or(false);

        if !attack_present {
            self.emit("Battle", "等待战斗动作…");
            return;
        }

        let attack_returned_after_submit = self.battle.awaiting_attack_resolution();
        if !attack_returned_after_submit {
            self.battle.transition(BattleFlowEvent::BattleActionable);
        }

        // Read the current battle scene (m of n) from the BATTLE label HUD.
        let screen_scene = self
            .sidecar()
            .read_battle_scene(None, BATTLE_SCENE_REGION)
            .unwrap_or(None);
        match post_attack_hud_read_gate(
            self.advanced_mode,
            attack_returned_after_submit,
            screen_scene,
            self.battle.post_attack_hud_wait_started_at(),
            Instant::now(),
            POST_ATTACK_HUD_READ_TIMEOUT,
        ) {
            PostAttackHudReadGate::Ready => {
                if attack_returned_after_submit {
                    self.battle
                        .transition(BattleFlowEvent::PostAttackHudResolved);
                }
            }
            PostAttackHudReadGate::Waiting { started_at } => {
                self.battle
                    .transition(BattleFlowEvent::PostAttackHudWaitStarted { at: started_at });
                self.emit("Battle", "等待读取 Battle HUD…");
                return;
            }
            PostAttackHudReadGate::TimedOut => {
                self.battle
                    .transition(BattleFlowEvent::PostAttackHudResolved);
                self.emit("Battle", "Battle HUD 读取超时，沿用当前 Turn 推进逻辑");
            }
        }
        let scene_m = screen_scene.map(|(m, _)| m);

        // Decide what to do based on (prior scene, fresh read). See
        // `tick_scene_state` for the full state-transition rules; the
        // key invariant is that a failed CV read (`scene_m == None`)
        // never advances the index, never re-locks `last_screen_scene`,
        // and never re-fires skills for an already-executed scene.
        let tick = tick_scene_state(
            self.battle.last_screen_scene,
            self.battle.current_scene_index,
            self.battle.executed_scene_index,
            scene_m,
        );
        let previous_scene_index = self.battle.current_scene_index;
        self.battle.current_scene_index = tick.current_scene_index;
        self.battle.last_screen_scene = tick.last_screen_scene;
        let scene_changed = self.battle.current_scene_index != previous_scene_index;
        if !self.advanced_mode {
            if scene_changed {
                self.battle.current_turn_index = 0;
            } else if attack_returned_after_submit {
                self.battle.current_turn_index = self.battle.current_turn_index.saturating_add(1);
            }
        }

        if self.advanced_mode && tick.needs_exec {
            self.battle.scene_config_used = false;
            let scene_str = match screen_scene {
                Some((m, n)) => format!("{m}/{n}"),
                None => "?".into(),
            };
            self.emit(
                "Battle",
                &format!(
                    "执行第 {} 组指令 (画面场景: {})",
                    self.battle.current_scene_index + 1,
                    scene_str,
                ),
            );
            self.battle.scene_config_used = self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .is_some();
            self.battle.executed_scene_index = Some(self.battle.current_scene_index);
        } else if self.advanced_mode {
            self.emit("Battle", "场景未变更，直接攻击");
        } else {
            self.battle.scene_config_used = false;
            let turn_key = (
                self.battle.current_scene_index,
                self.battle.current_turn_index,
            );
            if let Some((turn_cfg, over_configured_turns)) = normal_turn_for_current_state(
                &self.scenes,
                self.battle.current_scene_index,
                self.battle.current_turn_index,
            ) {
                let scene_str = match screen_scene {
                    Some((m, n)) => format!("{m}/{n}"),
                    None => "?".into(),
                };
                if over_configured_turns {
                    self.emit(
                        "Battle",
                        &format!(
                            "Battle {} Turn {} 超出配置，沿用最后 Turn 的攻击配置 (画面场景: {})",
                            self.battle.current_scene_index + 1,
                            self.battle.current_turn_index + 1,
                            scene_str,
                        ),
                    );
                } else if self.battle.executed_turn_key != Some(turn_key) {
                    self.emit(
                        "Battle",
                        &format!(
                            "执行 Battle {} Turn {} 指令 (画面场景: {})",
                            self.battle.current_scene_index + 1,
                            self.battle.current_turn_index + 1,
                            scene_str,
                        ),
                    );
                    let resolved_turn = self.resolve_normal_turn_for_current_members(&turn_cfg);
                    if !self.execute_turn_skills(&resolved_turn) {
                        return;
                    }
                    self.battle.executed_turn_key = Some(turn_key);
                    self.battle.scene_config_used = true;
                } else {
                    self.emit("Battle", "Turn 未变更，直接攻击");
                    self.battle.scene_config_used = true;
                }
            } else {
                self.emit(
                    "Battle",
                    &format!(
                        "无第 {} 组 Battle 配置，直接攻击",
                        self.battle.current_scene_index + 1
                    ),
                );
            }
        }

        if !self.advanced_mode {
            if let Some((turn_cfg, over_configured_turns)) = normal_turn_for_current_state(
                &self.scenes,
                self.battle.current_scene_index,
                self.battle.current_turn_index,
            ) {
                if !over_configured_turns {
                    self.select_enemy_target(turn_cfg.enemy_target.as_deref());
                }
            }
        }

        if self.advanced_mode && attack_returned_after_submit {
            if let Some(scene) = self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .cloned()
            {
                if !self.execute_next_grand_turn_skills(&scene) {
                    return;
                }
            }
        }

        if self.advanced_mode {
            if let Some(scene) = self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .cloned()
            {
                let grand_servants = self.grand_servant_runtime_configs();
                if grand_startup_can_run_before_attack(&scene, &grand_servants) {
                    if !self.prepare_grand_startup_before_attack(&scene) {
                        return;
                    }
                }
            }
        }

        if self.advanced_mode {
            let target = advanced_enemy_target_for_current_scene(
                &self.advanced_scenes,
                self.battle.current_scene_index,
                tick.needs_exec,
            )
            .map(str::to_owned);
            self.select_enemy_target(target.as_deref());
        }

        // Click the attack button
        self.emit("Battle", "点击攻击按钮");
        if !self.tap_attack_button() {
            return;
        }
        thread::sleep(ACTION_DELAY);
    }
}
