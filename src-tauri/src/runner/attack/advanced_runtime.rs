//! Advanced-mode attack rule execution and fallback handling.

use super::*;

impl Runner {
    pub(crate) fn handle_advanced_attack(
        &mut self,
        cards: Vec<CommandCardMatch>,
        nps: Vec<NoblePhantasmMatch>,
        party_ids: [Option<u32>; 3],
        party_supports: [bool; 3],
    ) {
        let grand_servants = self.grand_servant_runtime_configs();
        let Some(scene) = self
            .advanced_scenes
            .get(self.battle.current_scene_index)
            .cloned()
        else {
            if !grand_servants.is_empty() {
                self.emit("Attack", "无高级指令配置，按冠位自动策略攻击");
                let scene = AdvancedBattleScene {
                    id: "__grand_auto_default__".into(),
                    enemy_target: None,
                    main_output: None,
                    grand_auto_order_change: None,
                    command_conditions: Vec::new(),
                    control_actions: Vec::new(),
                    turns: Vec::new(),
                    startup_actions: Vec::new(),
                    rules: Vec::new(),
                };
                let picks = choose_advanced_auto_picks_with_crit(
                    &scene,
                    &cards,
                    &nps,
                    &party_ids,
                    &party_supports,
                    &grand_servants,
                    &self.config.grand_card_strategy,
                    self.config.grand_class,
                    self.config.prefer_higher_critical_chance,
                );
                self.tap_picks("Attack", &picks, &cards, &party_ids);
                return;
            }
            self.emit("Attack", "无高级指令配置，按默认顺序补位");
            self.pick_and_tap_attack_cards(&cards, &nps, &party_ids, &party_supports, None);
            return;
        };

        if scene.rules.is_empty() || uses_advanced_strategy_flow(&scene) {
            let scene_index = self.battle.current_scene_index;
            let executed_control_count = *self
                .battle
                .advanced_control_indices
                .get(&scene_index)
                .unwrap_or(&0);
            let (current_party_ids, current_party_supports) =
                if self.battle.advanced_startup_done.contains(&scene_index) {
                    let startup_control_count = *self
                        .battle
                        .advanced_startup_control_indices
                        .get(&scene_index)
                        .unwrap_or(&executed_control_count);
                    (
                        self.advanced_party_ids_after_startup_flow(
                            &scene,
                            executed_control_count,
                            startup_control_count,
                        ),
                        self.advanced_party_supports_after_startup_flow(
                            &scene,
                            executed_control_count,
                            startup_control_count,
                        ),
                    )
                } else {
                    (
                        self.advanced_party_ids_after_control(&scene, executed_control_count),
                        self.advanced_party_supports_after_control(&scene, executed_control_count),
                    )
                };
            let (cards, nps, party_ids, party_supports) = if current_party_ids != party_ids
                || current_party_supports != party_supports
            {
                let Some((cards, nps)) = self.read_attack_state(&current_party_ids, true, false)
                else {
                    return;
                };
                (cards, nps, current_party_ids, current_party_supports)
            } else {
                (cards, nps, party_ids, party_supports)
            };

            if !self.battle.advanced_startup_done.contains(&scene_index) {
                if scene.grand_auto_order_change == Some(true) {
                    let auto_order_change = grand_auto_order_change_action(
                        &cards,
                        &party_ids,
                        &party_supports,
                        &grand_servants,
                        self.config.grand_class,
                    );
                    let original_members = self.build_full_party_members();
                    let mut members = original_members.clone();
                    let mut startup_actions = Vec::new();
                    if let Some(action) = auto_order_change.clone() {
                        self.emit("Attack", "启动条件：自动将后排冠位从者换至前排");
                        let ids = party_member_ids(&members);
                        if action_frontline_available(&ids, &action) {
                            apply_party_member_lineup_change(&mut members, &action);
                            startup_actions.push(action.clone());
                            self.battle
                                .advanced_auto_order_changes
                                .insert(scene_index, action);
                        } else {
                            self.emit("Battle", "跳过自动换位：目标不在可交换位置");
                        }
                    } else {
                        self.emit(
                            "Attack",
                            "启动条件：主冠位不需要或无法自动换位，直接执行 Turn 1 技能",
                        );
                    }
                    let next_control_count = if executed_control_count < scene.control_actions.len()
                    {
                        executed_control_count + 1
                    } else {
                        executed_control_count
                    };
                    if executed_control_count < scene.control_actions.len() {
                        self.emit(
                            "Attack",
                            &format!("Turn 1 执行本回合控制行动 {next_control_count}"),
                        );
                    }
                    let first_turn_actions = advanced_turn_actions(&scene, 0).unwrap_or_default();
                    let pending_actions = scene
                        .control_actions
                        .iter()
                        .skip(executed_control_count)
                        .take(next_control_count.saturating_sub(executed_control_count))
                        .chain(first_turn_actions.iter())
                        .cloned();
                    for action in pending_actions {
                        let Some(resolved_action) = resolve_action_to_current_member_positions(
                            &members,
                            &original_members,
                            &action,
                        ) else {
                            self.emit_action(
                                "跳过行动：目标不在当前可用位置",
                                skipped_action_log_meta(&action),
                            );
                            continue;
                        };
                        let ids = party_member_ids(&members);
                        if !action_frontline_available(&ids, &resolved_action) {
                            self.emit_action(
                                "跳过行动：目标不在前排",
                                skipped_action_log_meta(&action),
                            );
                            continue;
                        }
                        apply_party_member_lineup_change(&mut members, &resolved_action);
                        startup_actions.push(resolved_action);
                    }
                    let startup_party_members = frontline_party_members(&members);
                    let (startup_party_ids, startup_party_supports) =
                        frontline_party_ids_and_supports(&startup_party_members);
                    self.battle
                        .advanced_control_indices
                        .insert(scene_index, next_control_count);
                    self.battle
                        .advanced_startup_control_indices
                        .insert(scene_index, next_control_count);
                    self.battle.advanced_startup_done.insert(scene_index);
                    self.battle.advanced_turn_indices.insert(scene_index, 1);
                    if !startup_actions.is_empty() {
                        if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                        if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                            self.fail_action(
                                "Battle",
                                "返回 Battle 执行启动行动",
                                "等待攻击按钮超时".into(),
                            );
                            return;
                        }
                        let prep_turn = BattleTurn {
                            id: scene.id.clone(),
                            preparation_actions: startup_actions,
                            servant_actions: Vec::new(),
                            equipment_actions: Vec::new(),
                            command_spell_actions: Vec::new(),
                            enemy_target: None,
                            attack_priority: Vec::new(),
                            attack_mode: AttackMode::Normal,
                            critical_strategy: CriticalAttackStrategy::default(),
                            advanced_card_strategy: AdvancedCardStrategy::default(),
                        };
                        if !self.execute_turn_skills(&prep_turn) {
                            return;
                        }

                        self.emit("Battle", "Turn 1 技能完成，进入自动战斗");
                        if !self.tap_attack_button() {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                        let Some((next_cards, next_nps)) =
                            self.read_attack_state(&startup_party_ids, true, false)
                        else {
                            return;
                        };
                        let picks = choose_advanced_auto_picks_with_crit(
                            &scene,
                            &next_cards,
                            &next_nps,
                            &startup_party_ids,
                            &startup_party_supports,
                            &grand_servants,
                            &self.config.grand_card_strategy,
                            self.config.grand_class,
                            self.config.prefer_higher_critical_chance,
                        );
                        self.tap_picks("Attack", &picks, &next_cards, &startup_party_ids);
                        return;
                    }

                    let picks = choose_advanced_auto_picks_with_crit(
                        &scene,
                        &cards,
                        &nps,
                        &startup_party_ids,
                        &startup_party_supports,
                        &grand_servants,
                        &self.config.grand_card_strategy,
                        self.config.grand_class,
                        self.config.prefer_higher_critical_chance,
                    );
                    self.tap_picks("Attack", &picks, &cards, &startup_party_ids);
                    return;
                }

                if !advanced_startup_conditions_match(&scene, &cards, &party_ids, &party_supports) {
                    let control_index = executed_control_count;
                    if let Some(control_action) = scene.control_actions.get(control_index).cloned()
                    {
                        self.emit(
                            "Attack",
                            &format!("启动条件未满足，执行控制行动 {}", control_index + 1),
                        );
                        if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                        if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                            self.fail_action(
                                "Battle",
                                "返回 Battle 执行控制行动",
                                "等待攻击按钮超时".into(),
                            );
                            return;
                        }
                        let control_turn = BattleTurn {
                            id: format!("{}_control_{}", scene.id, control_index + 1),
                            preparation_actions: vec![control_action],
                            servant_actions: Vec::new(),
                            equipment_actions: Vec::new(),
                            command_spell_actions: Vec::new(),
                            enemy_target: None,
                            attack_priority: Vec::new(),
                            attack_mode: AttackMode::Normal,
                            critical_strategy: CriticalAttackStrategy::default(),
                            advanced_card_strategy: AdvancedCardStrategy::default(),
                        };
                        if !self.execute_turn_skills(&control_turn) {
                            return;
                        }
                        self.battle
                            .advanced_control_indices
                            .insert(scene_index, control_index + 1);

                        self.emit("Battle", "控制行动完成，返回指令卡攻击");
                        if !self.tap_attack_button() {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                        let control_party_ids =
                            self.advanced_party_ids_after_control(&scene, control_index + 1);
                        let control_party_supports =
                            self.advanced_party_supports_after_control(&scene, control_index + 1);
                        let Some((next_cards, _next_nps)) =
                            self.read_attack_state(&control_party_ids, true, false)
                        else {
                            return;
                        };
                        let picks = choose_advanced_auto_picks_with_crit(
                            &scene,
                            &next_cards,
                            &[],
                            &control_party_ids,
                            &control_party_supports,
                            &grand_servants,
                            &self.config.grand_card_strategy,
                            self.config.grand_class,
                            self.config.prefer_higher_critical_chance,
                        );
                        self.tap_picks("Attack", &picks, &next_cards, &control_party_ids);
                        return;
                    }

                    self.emit("Attack", "启动条件未满足，按自动优先级攻击且不释放宝具");
                    let picks = choose_advanced_auto_picks_with_crit(
                        &scene,
                        &cards,
                        &[],
                        &party_ids,
                        &party_supports,
                        &grand_servants,
                        &self.config.grand_card_strategy,
                        self.config.grand_class,
                        self.config.prefer_higher_critical_chance,
                    );
                    self.tap_picks("Attack", &picks, &cards, &party_ids);
                    return;
                }

                self.emit("Attack", "启动条件满足，执行 Turn 1 技能");
                let next_control_count = if executed_control_count < scene.control_actions.len() {
                    executed_control_count + 1
                } else {
                    executed_control_count
                };
                if executed_control_count < scene.control_actions.len() {
                    self.emit(
                        "Attack",
                        &format!("Turn 1 执行本回合控制行动 {next_control_count}"),
                    );
                }
                let first_turn_actions = advanced_turn_actions(&scene, 0).unwrap_or_default();
                let pending_actions = scene
                    .control_actions
                    .iter()
                    .skip(executed_control_count)
                    .take(next_control_count.saturating_sub(executed_control_count))
                    .chain(first_turn_actions.iter())
                    .cloned();
                let (startup_actions, startup_party_members) = self
                    .advanced_actions_and_members_after(
                        scene
                            .control_actions
                            .iter()
                            .take(executed_control_count)
                            .cloned(),
                        pending_actions,
                    );
                let (startup_party_ids, startup_party_supports) =
                    frontline_party_ids_and_supports(&startup_party_members);
                self.battle
                    .advanced_control_indices
                    .insert(scene_index, next_control_count);
                self.battle
                    .advanced_startup_control_indices
                    .insert(scene_index, next_control_count);
                self.battle.advanced_startup_done.insert(scene_index);
                self.battle.advanced_turn_indices.insert(scene_index, 1);
                if !startup_actions.is_empty() {
                    if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                        self.fail_action(
                            "Battle",
                            "返回 Battle 执行启动行动",
                            "等待攻击按钮超时".into(),
                        );
                        return;
                    }
                    let prep_turn = BattleTurn {
                        id: scene.id.clone(),
                        preparation_actions: startup_actions,
                        servant_actions: Vec::new(),
                        equipment_actions: Vec::new(),
                        command_spell_actions: Vec::new(),
                        enemy_target: None,
                        attack_priority: Vec::new(),
                        attack_mode: AttackMode::Normal,
                        critical_strategy: CriticalAttackStrategy::default(),
                        advanced_card_strategy: AdvancedCardStrategy::default(),
                    };
                    if !self.execute_turn_skills(&prep_turn) {
                        return;
                    }

                    self.emit("Battle", "Turn 1 技能完成，进入自动战斗");
                    if !self.tap_attack_button() {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    let Some((next_cards, next_nps)) =
                        self.read_attack_state(&startup_party_ids, true, false)
                    else {
                        return;
                    };
                    let picks = choose_advanced_auto_picks_with_crit(
                        &scene,
                        &next_cards,
                        &next_nps,
                        &startup_party_ids,
                        &startup_party_supports,
                        &grand_servants,
                        &self.config.grand_card_strategy,
                        self.config.grand_class,
                        self.config.prefer_higher_critical_chance,
                    );
                    self.tap_picks("Attack", &picks, &next_cards, &startup_party_ids);
                    return;
                }

                let picks = choose_advanced_auto_picks_with_crit(
                    &scene,
                    &cards,
                    &nps,
                    &startup_party_ids,
                    &startup_party_supports,
                    &grand_servants,
                    &self.config.grand_card_strategy,
                    self.config.grand_class,
                    self.config.prefer_higher_critical_chance,
                );
                self.tap_picks("Attack", &picks, &cards, &startup_party_ids);
                return;
            }

            let executed_control_count = *self
                .battle
                .advanced_control_indices
                .get(&self.battle.current_scene_index)
                .unwrap_or(&0);
            let startup_control_count = *self
                .battle
                .advanced_startup_control_indices
                .get(&self.battle.current_scene_index)
                .unwrap_or(&executed_control_count);
            if executed_control_count < scene.control_actions.len() {
                let active_actions = advanced_startup_flow_actions(
                    &scene,
                    executed_control_count,
                    startup_control_count,
                    *self
                        .battle
                        .advanced_turn_indices
                        .get(&self.battle.current_scene_index)
                        .unwrap_or(&0),
                    self.battle
                        .advanced_auto_order_changes
                        .get(&self.battle.current_scene_index),
                );
                let (control_actions, control_party_members) = self
                    .advanced_actions_and_members_after(
                        active_actions.into_iter(),
                        scene
                            .control_actions
                            .iter()
                            .skip(executed_control_count)
                            .take(1)
                            .cloned(),
                    );
                let (control_party_ids, control_party_supports) =
                    frontline_party_ids_and_supports(&control_party_members);
                let next_control_count = executed_control_count + 1;
                self.battle
                    .advanced_control_indices
                    .insert(self.battle.current_scene_index, next_control_count);

                if !control_actions.is_empty() {
                    self.emit(
                        "Attack",
                        &format!("自动战斗执行本回合控制行动 {next_control_count}"),
                    );
                    if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                        self.fail_action(
                            "Battle",
                            "返回 Battle 执行控制行动",
                            "等待攻击按钮超时".into(),
                        );
                        return;
                    }
                    let control_turn = BattleTurn {
                        id: format!("{}_auto_control_{}", scene.id, next_control_count),
                        preparation_actions: control_actions,
                        servant_actions: Vec::new(),
                        equipment_actions: Vec::new(),
                        command_spell_actions: Vec::new(),
                        enemy_target: None,
                        attack_priority: Vec::new(),
                        attack_mode: AttackMode::Normal,
                        critical_strategy: CriticalAttackStrategy::default(),
                        advanced_card_strategy: AdvancedCardStrategy::default(),
                    };
                    if !self.execute_turn_skills(&control_turn) {
                        return;
                    }

                    self.emit("Battle", "控制行动完成，返回指令卡攻击");
                    if !self.tap_attack_button() {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    let Some((next_cards, next_nps)) =
                        self.read_attack_state(&control_party_ids, true, false)
                    else {
                        return;
                    };
                    let picks = choose_advanced_auto_picks_with_crit(
                        &scene,
                        &next_cards,
                        &next_nps,
                        &control_party_ids,
                        &control_party_supports,
                        &grand_servants,
                        &self.config.grand_card_strategy,
                        self.config.grand_class,
                        self.config.prefer_higher_critical_chance,
                    );
                    self.tap_picks("Attack", &picks, &next_cards, &control_party_ids);
                    return;
                }

                let picks = choose_advanced_auto_picks_with_crit(
                    &scene,
                    &cards,
                    &nps,
                    &control_party_ids,
                    &control_party_supports,
                    &grand_servants,
                    &self.config.grand_card_strategy,
                    self.config.grand_class,
                    self.config.prefer_higher_critical_chance,
                );
                self.tap_picks("Attack", &picks, &cards, &control_party_ids);
                return;
            }
            let active_party_ids = self.advanced_party_ids_after_startup_flow(
                &scene,
                executed_control_count,
                startup_control_count,
            );
            let active_party_supports = self.advanced_party_supports_after_startup_flow(
                &scene,
                executed_control_count,
                startup_control_count,
            );
            let picks = choose_advanced_auto_picks_with_crit(
                &scene,
                &cards,
                &nps,
                &active_party_ids,
                &active_party_supports,
                &grand_servants,
                &self.config.grand_card_strategy,
                self.config.grand_class,
                self.config.prefer_higher_critical_chance,
            );
            self.tap_picks("Attack", &picks, &cards, &active_party_ids);
            return;
        }

        let mut cards = cards;
        let mut nps = nps;
        let mut next_rule_index = 0usize;
        let mut guard = 0usize;
        while next_rule_index < scene.rules.len() && guard <= scene.rules.len() {
            guard += 1;
            let Some((rule_index, rule)) = scene
                .rules
                .iter()
                .enumerate()
                .skip(next_rule_index)
                .find(|(_, rule)| {
                    advanced_rule_matches(rule, &cards, &nps, &party_ids, &party_supports)
                })
                .map(|(index, rule)| (index, rule.clone()))
            else {
                break;
            };

            let prep_actions: Vec<Action> = rule
                .actions
                .iter()
                .filter_map(|action| action.as_preparation_action())
                .collect();
            let attack_priority: Vec<AttackCard> = rule
                .actions
                .iter()
                .filter_map(|action| action.as_attack_card())
                .collect();

            if !prep_actions.is_empty() {
                self.emit(
                    "Attack",
                    &format!("高级规则 {} 命中，返回执行准备行动", rule_index + 1),
                );
                if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                    return;
                }
                thread::sleep(ACTION_DELAY);
                if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                    self.fail_action(
                        "Battle",
                        "返回 Battle 执行高级规则准备行动",
                        "等待攻击按钮超时".into(),
                    );
                    return;
                }
                let prep_turn = BattleTurn {
                    id: rule.id.clone(),
                    preparation_actions: prep_actions,
                    servant_actions: Vec::new(),
                    equipment_actions: Vec::new(),
                    command_spell_actions: Vec::new(),
                    enemy_target: None,
                    attack_priority: Vec::new(),
                    attack_mode: AttackMode::Normal,
                    critical_strategy: CriticalAttackStrategy::default(),
                    advanced_card_strategy: AdvancedCardStrategy::default(),
                };
                if !self.execute_turn_skills(&prep_turn) {
                    return;
                }

                self.emit("Battle", "高级规则准备行动完成，重新进入指令卡");
                if !self.tap_attack_button() {
                    return;
                }
                thread::sleep(ACTION_DELAY);
                let Some((next_cards, next_nps)) = self.read_attack_state(&party_ids, true, false)
                else {
                    return;
                };
                cards = next_cards;
                nps = next_nps;
                if !attack_priority.is_empty() {
                    self.emit(
                        "Attack",
                        &format!("高级规则 {} 准备后执行攻击", rule_index + 1),
                    );
                    self.pick_and_tap_attack_cards(
                        &cards,
                        &nps,
                        &party_ids,
                        &party_supports,
                        Some(&attack_priority),
                    );
                    return;
                }
                next_rule_index = rule_index + 1;
                continue;
            }

            if !attack_priority.is_empty() {
                self.emit(
                    "Attack",
                    &format!("高级规则 {} 命中，执行攻击", rule_index + 1),
                );
                self.pick_and_tap_attack_cards(
                    &cards,
                    &nps,
                    &party_ids,
                    &party_supports,
                    Some(&attack_priority),
                );
                return;
            }

            next_rule_index = rule_index + 1;
        }

        self.emit("Attack", "无高级规则命中攻击，按默认顺序补位");
        self.pick_and_tap_attack_cards(&cards, &nps, &party_ids, &party_supports, None);
    }
}
