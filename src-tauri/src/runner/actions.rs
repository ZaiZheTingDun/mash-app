//! Battle action execution helpers.
//!
//! This module owns servant skills, master skills, command spells, order
//! change execution, and target-position resolution for battle actions.

use super::*;

impl Runner {
    pub(crate) fn execute_turn_skills(&mut self, turn: &BattleTurn) -> bool {
        for action in turn_preparation_actions(turn) {
            match action {
                Action::Servant {
                    servant,
                    servant_id,
                    skill,
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

                    self.emit_action(
                        "从者技能",
                        ActionLogMeta::ServantSkill {
                            servant_id: *servant_id,
                            skill_index,
                            target_servant_id: *target_servant_id,
                        },
                    );
                    if !self.tap_at("Battle", pos) {
                        return self.fail_skill_execution(&action_label, "点击技能按钮失败");
                    }
                    thread::sleep(ACTION_DELAY);

                    if let Some(target_pos) = skill_target_position(target.as_deref()) {
                        self.emit_debug("Battle", "等待目标选择框出现");
                        if !self.wait_for_element_visible(
                            "Battle",
                            SKILL_TARGET_CLOSE_BUTTON_ELEMENT,
                            SKILL_WAIT_TIMEOUT,
                            "等待目标选择框出现…",
                            "等待目标选择框出现超时",
                        ) {
                            return self
                                .fail_skill_execution(&action_label, "等待目标选择框出现超时");
                        }

                        if !self.tap_at("Battle", target_pos) {
                            return self.fail_skill_execution(&action_label, "点击技能目标失败");
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
                    if !self.tap_at("Battle", pos) {
                        return self.fail_skill_execution(&action_label, "点击御主技能失败");
                    }
                    thread::sleep(ACTION_DELAY);

                    if let Some(change) = order_change {
                        if !self.execute_order_change(change) {
                            return self
                                .fail_skill_execution(&action_label, "Order Change 执行失败");
                        }
                    } else if let Some(target_pos) = skill_target_position(target.as_deref()) {
                        self.emit_debug("Battle", "等待目标选择框出现");
                        if !self.wait_for_element_visible(
                            "Battle",
                            SKILL_TARGET_CLOSE_BUTTON_ELEMENT,
                            SKILL_WAIT_TIMEOUT,
                            "等待目标选择框出现…",
                            "等待目标选择框出现超时",
                        ) {
                            return self
                                .fail_skill_execution(&action_label, "等待目标选择框出现超时");
                        }

                        if !self.tap_at("Battle", target_pos) {
                            return self.fail_skill_execution(&action_label, "点击技能目标失败");
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
                    }

                    if order_change.is_none() {
                        self.skip_after_skill();
                    }

                    if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                        return self.fail_skill_execution(&action_label, "等待攻击按钮超时");
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
            }
        }
        true
    }

    fn fail_skill_execution(&self, action_label: &str, reason: &str) -> bool {
        let message = format!("{action_label} 执行失败: {reason}");
        self.set_state(RunnerState::Error {
            message: message.clone(),
        });
        self.emit_warn("Battle", &message);
        false
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
        let Some(back_pos) = order_change_slot_position(change.back.as_deref(), 3..6) else {
            self.emit_action(
                "Order Change 后排目标无效，已跳过",
                ActionLogMeta::SkippedAction {
                    servant_id: change.back_servant_id,
                },
            );
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
        if !self.tap_at("Battle", front_pos) {
            return false;
        }
        thread::sleep(ACTION_DELAY);

        if !self.tap_at("Battle", back_pos) {
            return false;
        }
        thread::sleep(ACTION_DELAY);

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

    /// Tap the in-game "skip animation" button so the cut-in / buff
    /// effect plays at full speed. The button is in the top-right corner
    /// and is safe to tap whether or not an animation is currently
    /// playing -- on a normal Battle frame this region is the turn-counter
    /// pill which has no interactive effect.
    pub(crate) fn skip_after_skill(&mut self) {
        let _ = self.tap_at("Battle", SKIP_ANIMATION_BUTTON);
        thread::sleep(ACTION_DELAY);
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
}

pub(crate) fn skill_position(servant: Option<&str>, skill: Option<&str>) -> Option<Point> {
    let si = parse_index(servant?, "servant_")?;
    let ki = parse_index(skill?, "skill_")?;
    SERVANT_SKILLS.get(si).and_then(|row| row.get(ki)).copied()
}

pub(crate) fn equipment_skill_position(skill: Option<&str>) -> Option<Point> {
    let ki = parse_index(skill?, "skill_")?;
    EQUIPMENT_SKILLS.get(ki).copied()
}

/// Map a Command Spell name to its index in `COMMAND_SPELL_OPTIONS`.
/// Returns `None` for unknown / missing values so the runner can skip
/// the action gracefully (mirrors how `skill_position` returns `None`
/// for malformed servant/skill strings).
pub(crate) fn command_spell_index(spell: Option<&str>) -> Option<usize> {
    match spell? {
        "np_release" => Some(0),
        "restore" => Some(1),
        _ => None,
    }
}

pub(crate) fn turn_preparation_actions(turn: &BattleTurn) -> std::slice::Iter<'_, Action> {
    turn.preparation_actions.iter()
}

/// Skill targets are always allies (servant_1, servant_2, servant_3).
pub(crate) fn skill_target_position(target: Option<&str>) -> Option<Point> {
    let t = target?;
    let si = parse_index(t, "servant_")?;
    SKILL_TARGETS.get(si).copied()
}

pub(crate) fn order_change_slot_position(
    target: Option<&str>,
    allowed: std::ops::Range<usize>,
) -> Option<Point> {
    let t = target?;
    let si = parse_index(t, "servant_")?;
    if !allowed.contains(&si) {
        return None;
    }
    ORDER_CHANGE_SLOTS.get(si).copied()
}

/// Enemy targets (enemy_1..enemy_6).
pub(crate) fn enemy_target_position(target: Option<&str>) -> Option<Point> {
    let t = target?;
    let ei = parse_index(t, "enemy_")?;
    ENEMY_TARGETS.get(ei).copied()
}

pub(crate) fn servant_skill_failure_label(
    servant: Option<&str>,
    servant_id: Option<u32>,
    skill_index: u32,
) -> String {
    match (servant, servant_id) {
        (Some(slot), Some(id)) => format!("从者 {slot} (#{id}) 技能 {}", skill_index + 1),
        (Some(slot), None) => format!("从者 {slot} 技能 {}", skill_index + 1),
        (None, Some(id)) => format!("从者 #{id} 技能 {}", skill_index + 1),
        (None, None) => format!("从者技能 {}", skill_index + 1),
    }
}

pub(crate) fn equipment_skill_failure_label(skill_index: u32, order_change: bool) -> String {
    if order_change {
        format!("御主技能 {} / Order Change", skill_index + 1)
    } else {
        format!("御主技能 {}", skill_index + 1)
    }
}

pub(crate) fn command_spell_failure_label(spell: &str) -> String {
    match spell {
        "np_release" => "令咒 开放宝具".into(),
        "restore" => "令咒 回复".into(),
        _ => format!("令咒 {spell}"),
    }
}
