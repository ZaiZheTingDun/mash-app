//! Battle action execution helpers.
//!
//! This module owns servant skills, master skills, command spells, order
//! change execution, and target-position resolution for battle actions.

use super::*;

const SKILL_ACTIVATION_NOT_OBSERVED_MESSAGE: &str =
    "技能点击未观察到状态变化（请检查该技能目标选择是否正确，无目标请选择无目标）";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillUseDialogState {
    Confirm,
    AlreadyUsed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillPostTapOutcome {
    Confirmed,
    AlreadyUsed,
    ProceedWithoutDialog,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SkillPostTapExpectation {
    ActivationStart,
    TargetPicker,
    OrderChange,
    SelectionDialog(SelectionDialogKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionDialogKind {
    AddInfo,
    TreasureDevice,
    SelfTreasureDevice,
}

impl Runner {
    pub(crate) fn execute_turn_skills(&mut self, turn: &BattleTurn) -> bool {
        for action in turn_preparation_actions(turn) {
            match action {
                Action::Servant {
                    servant,
                    servant_id,
                    skill,
                    skill_selection,
                    target,
                    target_servant_id,
                    ..
                } => {
                    let Some(pos) = skill_position(servant.as_deref(), skill.as_deref()) else {
                        continue;
                    };
                    let Some(skill_index) = skill
                        .as_deref()
                        .and_then(|value| parse_index(value, "skill_"))
                        .map(|index| index as u32)
                    else {
                        continue;
                    };
                    let action_label =
                        servant_skill_failure_label(servant.as_deref(), *servant_id, skill_index);
                    let target_pos = skill_target_position(target.as_deref());
                    let expectation =
                        skill_post_tap_expectation(skill_selection.as_ref(), target_pos, false);

                    self.emit_action(
                        "从者技能",
                        ActionLogMeta::ServantSkill {
                            servant_id: *servant_id,
                            skill_index,
                            target_servant_id: *target_servant_id,
                        },
                    );
                    let mut triggered = false;
                    let mut skipped = false;
                    for attempt in 0..=1 {
                        if attempt > 0 {
                            self.emit_warn("Battle", "技能点击未确认生效，重试一次");
                        }
                        self.emit_skill_tap_debug("点击从者技能", pos);
                        if !self.tap_at("Battle", pos) {
                            return self.fail_skill_execution(&action_label, "点击技能按钮失败");
                        }
                        thread::sleep(SKILL_TAP_DELAY);
                        let Some(outcome) = self.handle_skill_post_tap(&action_label, expectation)
                        else {
                            if attempt == 0 && self.config.verify_skill_activation {
                                continue;
                            }
                            return self.fail_skill_execution(
                                &action_label,
                                SKILL_ACTIVATION_NOT_OBSERVED_MESSAGE,
                            );
                        };
                        if matches!(outcome, SkillPostTapOutcome::AlreadyUsed) {
                            skipped = true;
                            triggered = true;
                            break;
                        }

                        if let Some(selection) = skill_selection {
                            if !self.execute_skill_selection(selection, &action_label) {
                                return false;
                            }
                        }

                        if let Some(target_pos) = target_pos {
                            self.emit_debug("Battle", "等待目标选择框出现");
                            if !self.wait_for_element_visible(
                                "Battle",
                                SKILL_TARGET_CLOSE_BUTTON_ELEMENT,
                                SKILL_WAIT_TIMEOUT,
                                "等待目标选择框出现…",
                                "等待目标选择框出现超时",
                            ) {
                                if attempt == 0 && self.config.verify_skill_activation {
                                    continue;
                                }
                                return self
                                    .fail_skill_execution(&action_label, "等待目标选择框出现超时");
                            }

                            self.emit_skill_tap_debug("点击技能目标", target_pos);
                            if !self.tap_at("Battle", target_pos) {
                                return self
                                    .fail_skill_execution(&action_label, "点击技能目标失败");
                            }
                            self.emit_debug("Battle", "等待目标选择框关闭");
                            if !self.wait_for_element_hidden(
                                "Battle",
                                SKILL_TARGET_CLOSE_BUTTON_ELEMENT,
                                SKILL_WAIT_TIMEOUT,
                                "等待目标选择框关闭…",
                                "等待目标选择框关闭超时",
                            ) {
                                return self
                                    .fail_skill_execution(&action_label, "等待目标选择框关闭超时");
                            }
                        } else if !self.wait_for_skill_activation_start() {
                            if attempt == 0 {
                                continue;
                            }
                            return self.fail_skill_execution(
                                &action_label,
                                SKILL_ACTIVATION_NOT_OBSERVED_MESSAGE,
                            );
                        }
                        triggered = true;
                        break;
                    }
                    if !triggered {
                        return self.fail_skill_execution(
                            &action_label,
                            SKILL_ACTIVATION_NOT_OBSERVED_MESSAGE,
                        );
                    }
                    if skipped {
                        continue;
                    }

                    self.skip_after_skill();

                    // Skill animation (cut-in, buff effect, etc.) hides the
                    // attack button. Wait for it to reappear before firing
                    // the next skill, otherwise rapid taps land on nothing
                    // or, worse, on whatever overlay is currently shown.
                    if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                        return self.fail_skill_execution(&action_label, "等待攻击按钮超时");
                    }
                }
                Action::Equipment {
                    skill,
                    target,
                    target_servant_id,
                    order_change,
                    ..
                } => {
                    let Some(pos) = equipment_skill_position(skill.as_deref()) else {
                        continue;
                    };
                    let Some(skill_index) = skill
                        .as_deref()
                        .and_then(|value| parse_index(value, "skill_"))
                        .map(|index| index as u32)
                    else {
                        continue;
                    };
                    let action_label =
                        equipment_skill_failure_label(skill_index, order_change.is_some());

                    self.emit("Battle", "打开御主技能面板");
                    self.emit_skill_tap_debug("点击御主技能面板", EQUIPMENT_BUTTON);
                    if !self.tap_at("Battle", EQUIPMENT_BUTTON) {
                        return self.fail_skill_execution(&action_label, "打开御主技能面板失败");
                    }
                    thread::sleep(ACTION_DELAY);

                    self.emit_action(
                        "御主技能",
                        ActionLogMeta::EquipmentSkill {
                            skill_index,
                            target_servant_id: *target_servant_id,
                        },
                    );
                    if let Some(change) = order_change {
                        self.emit_skill_tap_debug("点击御主技能", pos);
                        if !self.tap_at("Battle", pos) {
                            return self.fail_skill_execution(&action_label, "点击御主技能失败");
                        }
                        thread::sleep(SKILL_TAP_DELAY);
                        let Some(outcome) = self.handle_skill_post_tap(
                            &action_label,
                            SkillPostTapExpectation::OrderChange,
                        ) else {
                            return self.fail_skill_execution(
                                &action_label,
                                SKILL_ACTIVATION_NOT_OBSERVED_MESSAGE,
                            );
                        };
                        if matches!(outcome, SkillPostTapOutcome::AlreadyUsed) {
                            continue;
                        }
                        if !self.execute_order_change(change) {
                            return self
                                .fail_skill_execution(&action_label, "Order Change 执行失败");
                        }
                        if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                            return self.fail_skill_execution(&action_label, "等待攻击按钮超时");
                        }
                        thread::sleep(ORDER_CHANGE_EXTRA_SETTLE);
                    } else {
                        let target_pos = skill_target_position(target.as_deref());
                        let expectation = skill_post_tap_expectation(None, target_pos, false);
                        let mut triggered = false;
                        let mut skipped = false;
                        for attempt in 0..=1 {
                            if attempt > 0 {
                                self.emit_warn("Battle", "技能点击未确认生效，重试一次");
                            }
                            self.emit_skill_tap_debug("点击御主技能", pos);
                            if !self.tap_at("Battle", pos) {
                                return self
                                    .fail_skill_execution(&action_label, "点击御主技能失败");
                            }
                            thread::sleep(SKILL_TAP_DELAY);
                            let Some(outcome) =
                                self.handle_skill_post_tap(&action_label, expectation)
                            else {
                                if attempt == 0 && self.config.verify_skill_activation {
                                    continue;
                                }
                                return self.fail_skill_execution(
                                    &action_label,
                                    SKILL_ACTIVATION_NOT_OBSERVED_MESSAGE,
                                );
                            };
                            if matches!(outcome, SkillPostTapOutcome::AlreadyUsed) {
                                skipped = true;
                                triggered = true;
                                break;
                            }

                            if let Some(target_pos) = target_pos {
                                self.emit_debug("Battle", "等待目标选择框出现");
                                if !self.wait_for_element_visible(
                                    "Battle",
                                    SKILL_TARGET_CLOSE_BUTTON_ELEMENT,
                                    SKILL_WAIT_TIMEOUT,
                                    "等待目标选择框出现…",
                                    "等待目标选择框出现超时",
                                ) {
                                    if attempt == 0 && self.config.verify_skill_activation {
                                        continue;
                                    }
                                    return self.fail_skill_execution(
                                        &action_label,
                                        "等待目标选择框出现超时",
                                    );
                                }

                                self.emit_skill_tap_debug("点击技能目标", target_pos);
                                if !self.tap_at("Battle", target_pos) {
                                    return self
                                        .fail_skill_execution(&action_label, "点击技能目标失败");
                                }
                                self.emit_debug("Battle", "等待目标选择框关闭");
                                if !self.wait_for_element_hidden(
                                    "Battle",
                                    SKILL_TARGET_CLOSE_BUTTON_ELEMENT,
                                    SKILL_WAIT_TIMEOUT,
                                    "等待目标选择框关闭…",
                                    "等待目标选择框关闭超时",
                                ) {
                                    return self.fail_skill_execution(
                                        &action_label,
                                        "等待目标选择框关闭超时",
                                    );
                                }
                            } else if !self.wait_for_skill_activation_start() {
                                if attempt == 0 {
                                    continue;
                                }
                                return self.fail_skill_execution(
                                    &action_label,
                                    SKILL_ACTIVATION_NOT_OBSERVED_MESSAGE,
                                );
                            }
                            triggered = true;
                            break;
                        }
                        if !triggered {
                            return self.fail_skill_execution(
                                &action_label,
                                SKILL_ACTIVATION_NOT_OBSERVED_MESSAGE,
                            );
                        }
                        if skipped {
                            continue;
                        }
                    }

                    if order_change.is_none() {
                        self.skip_after_skill();
                        if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                            return self.fail_skill_execution(&action_label, "等待攻击按钮超时");
                        }
                    }
                }
                // Command Spell (令咒) walks four full-screen modals:
                // button → spell row → 决定 confirm → ally target picker,
                // settling between each step because each tap pops or
                // pushes a modal.
                Action::CommandSpell {
                    spell,
                    target,
                    target_servant_id,
                    ..
                } => {
                    let Some(option_idx) = command_spell_index(spell.as_deref()) else {
                        continue;
                    };
                    let Some(target_pos) = skill_target_position(target.as_deref()) else {
                        continue;
                    };
                    let action_label =
                        command_spell_failure_label(spell.as_deref().unwrap_or_default());

                    self.emit_action(
                        "使用令咒",
                        ActionLogMeta::CommandSpell {
                            spell: spell.clone().unwrap_or_default(),
                            target_servant_id: *target_servant_id,
                        },
                    );
                    if !self.tap_at("Battle", COMMAND_SPELL_BUTTON) {
                        return self.fail_skill_execution(&action_label, "点击令咒按钮失败");
                    }
                    thread::sleep(COMMAND_SPELL_DIALOG_SETTLE);

                    if !self.tap_at("Battle", COMMAND_SPELL_OPTIONS[option_idx]) {
                        return self.fail_skill_execution(&action_label, "选择令咒行动失败");
                    }
                    thread::sleep(COMMAND_SPELL_DIALOG_SETTLE);

                    self.emit("Battle", "确认令咒");
                    if !self.tap_at("Battle", COMMAND_SPELL_CONFIRM) {
                        return self.fail_skill_execution(&action_label, "确认令咒失败");
                    }
                    thread::sleep(COMMAND_SPELL_DIALOG_SETTLE);

                    self.emit_debug("Battle", "等待令咒目标选择框出现");
                    if !self.wait_for_element_visible(
                        "Battle",
                        COMMAND_SPELL_CLOSE_BUTTON_ELEMENT,
                        SKILL_WAIT_TIMEOUT,
                        "等待令咒目标选择框出现…",
                        "等待令咒目标选择框出现超时",
                    ) {
                        return self
                            .fail_skill_execution(&action_label, "等待令咒目标选择框出现超时");
                    }

                    if !self.tap_at("Battle", target_pos) {
                        return self.fail_skill_execution(&action_label, "点击令咒目标失败");
                    }
                    self.emit_debug("Battle", "等待令咒目标选择框关闭");
                    if !self.wait_for_element_hidden(
                        "Battle",
                        COMMAND_SPELL_CLOSE_BUTTON_ELEMENT,
                        SKILL_WAIT_TIMEOUT,
                        "等待令咒目标选择框关闭…",
                        "等待令咒目标选择框关闭超时",
                    ) {
                        return self
                            .fail_skill_execution(&action_label, "等待令咒目标选择框关闭超时");
                    }

                    self.skip_after_skill();

                    if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                        return self.fail_skill_execution(&action_label, "等待攻击按钮超时");
                    }
                }
                Action::EnemyTarget { target, .. } => {
                    self.select_enemy_target(target.as_deref());
                }
            }
        }
        true
    }

    fn fail_skill_execution(&self, action_label: &str, reason: &str) -> bool {
        let message = format!("{action_label} 执行失败: {reason}");
        self.transition_lifecycle(RunnerLifecycleEvent::Failed {
            message: message.clone(),
        });
        self.emit_warn("Battle", &message);
        false
    }

    fn emit_skill_tap_debug(&self, label: &str, point: Point) {
        self.emit_debug(
            "Battle",
            &format_skill_tap_debug(label, point, self.screen_w, self.screen_h),
        );
    }

    fn wait_for_skill_activation_start(&mut self) -> bool {
        if !self.config.verify_skill_activation {
            return true;
        }
        self.emit_debug("Battle", "等待战斗菜单隐藏…");
        self.wait_for_element_hidden(
            "Battle",
            BATTLE_ACTION_MENU_ELEMENT,
            SKILL_ACTIVATION_START_TIMEOUT,
            "等待战斗菜单隐藏…",
            SKILL_ACTIVATION_NOT_OBSERVED_MESSAGE,
        )
    }

    fn handle_skill_post_tap(
        &mut self,
        action_label: &str,
        expectation: SkillPostTapExpectation,
    ) -> Option<SkillPostTapOutcome> {
        let start = Instant::now();
        let mut tick: u32 = 0;
        let mut logged_template_error = false;
        loop {
            if self.is_cancelled() {
                return None;
            }

            let probe_started = Instant::now();
            let probe_result = self.probe_skill_use_dialog();
            let probe_elapsed_ms = probe_started.elapsed().as_millis();
            match probe_result {
                Ok((probe, image_path)) => {
                    if let Some(state) = classify_skill_use_dialog_probe(&probe) {
                        self.emit_local_debug(
                            "Battle",
                            &format!(
                                "技能确认 probe: score={:.3}, meanLuma={:.1}, image={}",
                                probe.score,
                                probe.mean_luma,
                                display_skill_use_probe_image_path(image_path.as_ref())
                            ),
                        );
                        match state {
                            SkillUseDialogState::AlreadyUsed => {
                                self.emit(
                                    "Battle",
                                    &format!(
                                        "{action_label}: 技能确认弹窗显示该技能已使用，已跳过"
                                    ),
                                );
                                let _ = self
                                    .tap_skip_animation_button("技能确认弹窗已使用，跳过该技能");
                                thread::sleep(ACTION_DELAY);
                                return Some(SkillPostTapOutcome::AlreadyUsed);
                            }
                            SkillUseDialogState::Confirm => {
                                self.emit_debug("Battle", "检测到技能确认弹窗，已点击确认");
                                if !self.tap_at("Battle", SKILL_USE_CONFIRM_POINT) {
                                    return None;
                                }
                                thread::sleep(ACTION_DELAY);
                                return Some(SkillPostTapOutcome::Confirmed);
                            }
                        }
                    } else if let Some(err) = probe.error.as_deref() {
                        if !logged_template_error {
                            self.emit_debug(
                                "Battle",
                                &format!("技能确认弹窗模板不可用，回退到原有流程: {err}"),
                            );
                            logged_template_error = true;
                        }
                    }
                }
                Err(err) => {
                    if !logged_template_error {
                        self.emit_debug(
                            "Battle",
                            &format!("技能确认弹窗探测失败，回退到原有流程: {err}"),
                        );
                        logged_template_error = true;
                    }
                }
            }

            let expectation_started = Instant::now();
            let expectation_reached = self.skill_post_tap_expectation_reached(expectation);
            let expectation_elapsed_ms = expectation_started.elapsed().as_millis();
            if expectation_reached {
                return Some(SkillPostTapOutcome::ProceedWithoutDialog);
            }

            if start.elapsed() >= SKILL_ACTIVATION_START_TIMEOUT {
                self.emit_local_debug(
                    "Battle",
                    &format!(
                        "技能点击后未观察到状态变化: elapsed={}ms, polls={}, lastProbe={}ms, lastExpectation={}ms",
                        start.elapsed().as_millis(),
                        tick + 1,
                        probe_elapsed_ms,
                        expectation_elapsed_ms
                    ),
                );
                return None;
            }
            tick += 1;
            if tick % 4 == 1 {
                self.emit_debug("Battle", "等待技能点击后的弹窗或后续状态…");
            }
            thread::sleep(SKILL_POLL_INTERVAL);
        }
    }

    fn probe_skill_use_dialog(&mut self) -> Result<(SkillUseDialogProbe, Option<PathBuf>), String> {
        if self.config.auto_capture_skill_use_probe {
            let image_path = self.capture_skill_use_probe_frame()?;
            let probe = self.sidecar().probe_skill_use_dialog(
                Some(&image_path),
                SKILL_USE_DIALOG_TEMPLATE,
                SKILL_USE_DIALOG_REGION,
                SKILL_USE_DIALOG_THRESHOLD,
                SKILL_USE_CONFIRM_REGION,
            )?;
            Ok((probe, Some(image_path)))
        } else {
            let probe = self.sidecar().probe_skill_use_dialog(
                None,
                SKILL_USE_DIALOG_TEMPLATE,
                SKILL_USE_DIALOG_REGION,
                SKILL_USE_DIALOG_THRESHOLD,
                SKILL_USE_CONFIRM_REGION,
            )?;
            Ok((probe, None))
        }
    }

    fn skill_post_tap_expectation_reached(&mut self, expectation: SkillPostTapExpectation) -> bool {
        match expectation {
            SkillPostTapExpectation::ActivationStart => self
                .sidecar()
                .find_element_by_name(None, BATTLE_SCREEN, BATTLE_ACTION_MENU_ELEMENT)
                .map(|matched| !matched.found)
                .unwrap_or(false),
            SkillPostTapExpectation::TargetPicker => self
                .sidecar()
                .find_element_by_name(None, BATTLE_SCREEN, SKILL_TARGET_CLOSE_BUTTON_ELEMENT)
                .map(|matched| matched.found)
                .unwrap_or(false),
            SkillPostTapExpectation::OrderChange => self
                .sidecar()
                .find_element_by_name(None, BATTLE_SCREEN, ORDER_CHANGE_CLOSE_BUTTON_ELEMENT)
                .map(|matched| matched.found)
                .unwrap_or(false),
            SkillPostTapExpectation::SelectionDialog(kind) => {
                let region = skill_selection_close_region(kind);
                self.sidecar()
                    .find_element(None, SKILL_SELECTION_CLOSE_BUTTON_TEMPLATE, region, 0.8)
                    .map(|matched| matched.is_some())
                    .unwrap_or(false)
            }
        }
    }

    pub(crate) fn execute_order_change(&mut self, change: &crate::OrderChangeSelection) -> bool {
        let Some(front_pos) = order_change_slot_position(change.front.as_deref(), 0..3) else {
            self.emit_action(
                "Order Change 前排目标无效，已跳过",
                ActionLogMeta::SkippedAction {
                    servant_id: change.front_servant_id,
                },
            );
            return true;
        };
        let Some(front_probe_pos) = order_change_selection_position(change.front.as_deref(), 0..3)
        else {
            return true;
        };
        let Some(back_pos) = order_change_slot_position(change.back.as_deref(), 3..6) else {
            self.emit_action(
                "Order Change 后排目标无效，已跳过",
                ActionLogMeta::SkippedAction {
                    servant_id: change.back_servant_id,
                },
            );
            return true;
        };
        let Some(back_probe_pos) = order_change_selection_position(change.back.as_deref(), 3..6)
        else {
            return true;
        };

        self.emit_debug("Battle", "等待换人框出现");
        if !self.wait_for_element_visible(
            "Battle",
            ORDER_CHANGE_CLOSE_BUTTON_ELEMENT,
            SKILL_WAIT_TIMEOUT,
            "等待换人框出现…",
            "等待换人框出现超时",
        ) {
            return false;
        }

        self.emit_action(
            "Order Change",
            ActionLogMeta::OrderChange {
                front_servant_id: change.front_servant_id,
                back_servant_id: change.back_servant_id,
            },
        );
        if !self.select_order_change_slot_until_confirmed("前排", front_pos, front_probe_pos) {
            return false;
        }
        if !self.select_order_change_slot_until_confirmed("后排", back_pos, back_probe_pos) {
            return false;
        }

        self.emit("Battle", "Order Change 点击进行更替");
        if !self.tap_at("Battle", ORDER_CHANGE_CONFIRM) {
            return false;
        }
        self.emit_debug("Battle", "等待换人框关闭");
        if !self.wait_for_element_hidden(
            "Battle",
            ORDER_CHANGE_CLOSE_BUTTON_ELEMENT,
            SKILL_WAIT_TIMEOUT,
            "等待换人框关闭…",
            "等待换人框关闭超时",
        ) {
            return false;
        }
        true
    }

    /// Click one Order Change slot, then verify its SELECT marker before any
    /// possible retry. Re-clicking a selected slot would toggle it off, so a
    /// successful probe always returns immediately and prevents another tap.
    fn select_order_change_slot_until_confirmed(
        &mut self,
        line: &str,
        tap_pos: Point,
        probe_pos: Point,
    ) -> bool {
        let start = Instant::now();
        loop {
            if self.is_cancelled() {
                return false;
            }
            self.emit_skill_tap_debug("点击换人槽位", tap_pos);
            if !self.tap_at("Battle", tap_pos) {
                return false;
            }
            thread::sleep(ORDER_CHANGE_SELECTION_CONFIRM_DELAY);

            let server = self.server;
            match self.sidecar().probe_order_change_selection(
                None,
                probe_pos.x,
                probe_pos.y,
                server,
            ) {
                Ok(probe) => {
                    self.emit_local_debug(
                        "Battle",
                        &format!(
                            "Order Change {line}选择 probe: selected={}, brightCount={}, sampleLumas={:?}",
                            probe.selected, probe.bright_count, probe.sample_lumas
                        ),
                    );
                    if probe.selected {
                        return true;
                    }
                }
                Err(err) => {
                    self.emit_local_debug(
                        "Battle",
                        &format!("Order Change {line}选择 probe 失败: {err}"),
                    );
                }
            }

            if start.elapsed() >= ORDER_CHANGE_SELECTION_TIMEOUT {
                self.emit_warn(
                    "Battle",
                    &format!("Order Change {line}选择未确认，已超过确认时间"),
                );
                return false;
            }
            self.emit_warn(
                "Battle",
                &format!("Order Change {line}选择未确认，重新点击"),
            );
        }
    }

    /// Tap the in-game "skip animation" button so the cut-in / buff
    /// effect plays at full speed. The button is in the top-right corner
    /// and is safe to tap whether or not an animation is currently
    /// playing -- on a normal Battle frame this region is the turn-counter
    /// pill which has no interactive effect.
    pub(crate) fn skip_after_skill(&mut self) {
        if self.handle_pending_skill_use_dialog_before_skip() {
            return;
        }
        let _ = self.tap_skip_animation_button("普通技能后跳过");
        thread::sleep(ACTION_DELAY);
    }

    fn handle_pending_skill_use_dialog_before_skip(&mut self) -> bool {
        if self.is_cancelled() {
            return false;
        }

        match self.probe_skill_use_dialog() {
            Ok((probe, image_path)) => match classify_skill_use_dialog_probe(&probe) {
                Some(SkillUseDialogState::AlreadyUsed) => {
                    self.emit_local_debug(
                        "Battle",
                        &format!(
                            "技能确认 probe: score={:.3}, meanLuma={:.1}, image={}",
                            probe.score,
                            probe.mean_luma,
                            display_skill_use_probe_image_path(image_path.as_ref())
                        ),
                    );
                    self.emit("Battle", "检测到技能确认弹窗已使用状态，先执行战斗跳过");
                    let _ = self.tap_skip_animation_button("技能确认弹窗已使用，执行战斗跳过");
                    thread::sleep(ACTION_DELAY);
                    true
                }
                Some(SkillUseDialogState::Confirm) => {
                    self.emit_local_debug(
                        "Battle",
                        &format!(
                            "技能确认 probe: score={:.3}, meanLuma={:.1}, image={}",
                            probe.score,
                            probe.mean_luma,
                            display_skill_use_probe_image_path(image_path.as_ref())
                        ),
                    );
                    self.emit_debug("Battle", "通用跳过前检测到技能确认弹窗，先点确认");
                    if self.tap_at("Battle", SKILL_USE_CONFIRM_POINT) {
                        thread::sleep(ACTION_DELAY);
                    }
                    false
                }
                None => {
                    if let Some(err) = probe.error.as_deref() {
                        self.emit_debug(
                            "Battle",
                            &format!("通用跳过前技能确认弹窗模板不可用，继续原流程: {err}"),
                        );
                    }
                    false
                }
            },
            Err(err) => {
                self.emit_debug(
                    "Battle",
                    &format!("通用跳过前技能确认弹窗探测失败，继续原流程: {err}"),
                );
                false
            }
        }
    }

    fn tap_skip_animation_button(&mut self, reason: &str) -> bool {
        self.emit_local_debug("Battle", &format!("点击战斗跳过区: {reason}"));
        self.tap_at("Battle", SKIP_ANIMATION_BUTTON)
    }

    fn capture_skill_use_probe_frame(&mut self) -> Result<PathBuf, String> {
        let dir = crate::app_data_dir(&self.app_handle)
            .join("debug")
            .join("skill-use-probes");
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("创建技能确认 probe 截图目录失败: {err}"))?;
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| format!("获取技能确认 probe 时间戳失败: {err}"))?
            .as_millis();
        let path = dir.join(format!("skill-use-probe-{timestamp_ms}.jpg"));
        let jpeg = self
            .sidecar()
            .get_frame_jpeg(0.0)
            .map_err(|err| format!("获取技能确认 probe 视频帧失败: {err}"))?;
        std::fs::write(&path, jpeg).map_err(|err| format!("写入技能确认 probe 截图失败: {err}"))?;
        Ok(path)
    }

    pub(crate) fn select_enemy_target(&mut self, target: Option<&str>) {
        let Some(point) = enemy_target_position(target) else {
            return;
        };
        self.emit(
            "Battle",
            &format!("选择敌方目标: {}", target.unwrap_or("?")),
        );
        if !self.tap_at("Battle", point) {
            return;
        }
        thread::sleep(ACTION_DELAY);
        self.skip_after_skill();
    }

    fn execute_skill_selection(
        &mut self,
        selection: &crate::SkillSelection,
        action_label: &str,
    ) -> bool {
        let Some(point) = skill_selection_option_position(selection) else {
            return self
                .fail_skill_execution(action_label, "技能二次选择配置无效或暂不支持该选项数量");
        };
        self.emit(
            "Battle",
            &format!(
                "选择技能选项: {}",
                selection.label.as_deref().unwrap_or("未命名选项")
            ),
        );
        self.emit_skill_tap_debug("点击技能二次选择", point);
        if !self.tap_at("Battle", point) {
            return self.fail_skill_execution(action_label, "点击技能二次选择失败");
        }
        thread::sleep(ACTION_DELAY);
        true
    }
}

mod helpers;
pub(crate) use helpers::*;
