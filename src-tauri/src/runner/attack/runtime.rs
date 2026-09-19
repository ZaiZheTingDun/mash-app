//! Device-facing attack flow orchestration.
//!
//! Selection and condition policies remain in the parent module; this module
//! owns screen reads, taps, retries, and battle-state transitions.

use super::*;

impl Runner {
    pub(crate) fn capture_battle_before_attack_screenshot(&mut self) -> Result<PathBuf, String> {
        let dir = battle_before_attack_screenshot_dir_in_root(
            &crate::app_data_dir(&self.app_handle),
            self.server,
        );
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("创建点击攻击前截图目录失败: {err}"))?;
        let path = dir.join(battle_before_attack_screenshot_filename(
            std::time::SystemTime::now(),
            self.server,
            self.completed_mission_runs,
            self.battle.current_scene_index,
            self.battle.current_turn_index,
        ));
        let png = self
            .sidecar()
            .get_frame_png(0.0)
            .map_err(|err| format!("获取点击攻击前视频帧失败: {err}"))?;
        std::fs::write(&path, png).map_err(|err| format!("写入点击攻击前截图失败: {err}"))?;
        Ok(path)
    }

    fn current_attack_is_critical(&self) -> bool {
        !self.advanced_mode
            && normal_turn_for_current_scene(
                &self.scenes,
                self.battle.current_scene_index,
                self.battle.current_turn_index,
            )
            .is_some_and(|turn| turn.attack_mode == AttackMode::Critical)
    }

    fn capture_unrecognized_critical_chance_screenshot(&mut self) -> Result<PathBuf, String> {
        let dir = unrecognized_critical_chance_screenshot_dir_in_root(&crate::app_data_dir(
            &self.app_handle,
        ));
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("创建暴击率识别截图目录失败: {err}"))?;
        let path = dir.join(unrecognized_critical_chance_screenshot_filename(
            std::time::SystemTime::now(),
            self.completed_mission_runs,
        ));
        let jpeg = self
            .sidecar()
            .get_frame_jpeg(0.0)
            .map_err(|err| format!("获取暴击率识别视频帧失败: {err}"))?;
        std::fs::write(&path, jpeg).map_err(|err| format!("写入暴击率识别截图失败: {err}"))?;
        Ok(path)
    }

    pub(super) fn current_attack_requires_np_recognition(&self) -> bool {
        if self.advanced_mode {
            return self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .map(|scene| {
                    advanced_scene_requires_np_recognition(
                        scene,
                        !self.config.grand_servants.is_empty(),
                        &self.config.grand_card_strategy,
                    )
                })
                .unwrap_or(!self.config.grand_servants.is_empty());
        }

        normal_turn_for_current_scene(
            &self.scenes,
            self.battle.current_scene_index,
            self.battle.current_turn_index,
        )
        .is_some_and(battle_turn_requires_np_recognition)
    }

    pub(crate) fn execute_next_grand_turn_skills(&mut self, scene: &AdvancedBattleScene) -> bool {
        let scene_index = self.battle.current_scene_index;
        if !self.battle.advanced_startup_done.contains(&scene_index) {
            return true;
        }
        let completed_turn_count = *self
            .battle
            .advanced_turn_indices
            .get(&scene_index)
            .unwrap_or(&0);
        let Some(turn_actions) = advanced_turn_actions(scene, completed_turn_count) else {
            return true;
        };
        let executed_control_count = *self
            .battle
            .advanced_control_indices
            .get(&scene_index)
            .unwrap_or(&0);
        let startup_control_count = *self
            .battle
            .advanced_startup_control_indices
            .get(&scene_index)
            .unwrap_or(&executed_control_count);
        let active_actions = advanced_startup_flow_actions(
            scene,
            executed_control_count,
            startup_control_count,
            completed_turn_count,
            self.battle.advanced_auto_order_changes.get(&scene_index),
        );
        let (resolved_actions, _) = self.advanced_actions_and_members_after(
            active_actions.into_iter(),
            turn_actions.iter().cloned(),
        );
        let turn_number = completed_turn_count + 1;

        if !resolved_actions.is_empty() {
            self.emit("Battle", &format!("执行 Turn {turn_number} 技能"));
            let turn = BattleTurn {
                id: format!("{}_turn_{turn_number}", scene.id),
                preparation_actions: resolved_actions,
                servant_actions: Vec::new(),
                equipment_actions: Vec::new(),
                command_spell_actions: Vec::new(),
                enemy_target: None,
                attack_priority: Vec::new(),
                attack_mode: AttackMode::Normal,
                critical_strategy: CriticalAttackStrategy::default(),
                advanced_card_strategy: AdvancedCardStrategy::default(),
            };
            if !self.execute_turn_skills(&turn) {
                return false;
            }
        }

        self.battle
            .advanced_turn_indices
            .insert(scene_index, turn_number);
        true
    }

    pub(crate) fn prepare_grand_startup_before_attack(
        &mut self,
        scene: &AdvancedBattleScene,
    ) -> bool {
        let scene_index = self.battle.current_scene_index;
        if self.battle.advanced_startup_done.contains(&scene_index) {
            return true;
        }
        let executed_control_count = *self
            .battle
            .advanced_control_indices
            .get(&scene_index)
            .unwrap_or(&0);
        let next_control_count = if executed_control_count < scene.control_actions.len() {
            executed_control_count + 1
        } else {
            executed_control_count
        };
        if executed_control_count < scene.control_actions.len() {
            self.emit(
                "Battle",
                &format!("无启动条件，执行本回合控制行动 {next_control_count}"),
            );
        } else {
            self.emit("Battle", "无启动条件，直接执行 Turn 1 技能");
        }
        let first_turn_actions = advanced_turn_actions(scene, 0).unwrap_or_default();
        let pending_actions = scene
            .control_actions
            .iter()
            .skip(executed_control_count)
            .take(next_control_count.saturating_sub(executed_control_count))
            .chain(first_turn_actions.iter())
            .cloned();
        let (startup_actions, _) = self.advanced_actions_and_members_after(
            scene
                .control_actions
                .iter()
                .take(executed_control_count)
                .cloned(),
            pending_actions,
        );
        self.battle
            .advanced_control_indices
            .insert(scene_index, next_control_count);
        self.battle
            .advanced_startup_control_indices
            .insert(scene_index, next_control_count);
        self.battle.advanced_startup_done.insert(scene_index);
        self.battle.advanced_turn_indices.insert(scene_index, 1);

        if startup_actions.is_empty() {
            return true;
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
            return false;
        }
        self.emit("Battle", "Turn 1 技能完成，进入自动战斗");
        true
    }

    pub(crate) fn handle_attack(&mut self) {
        if !self.ensure_battle_speed_level_two() {
            return;
        }

        if self.advanced_mode {
            let party_ids = self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .filter(|scene| scene.rules.is_empty() || uses_advanced_strategy_flow(scene))
                .map(|scene| self.advanced_current_party_ids(scene))
                .unwrap_or_else(|| self.build_party_ids());
            let party_supports = self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .filter(|scene| scene.rules.is_empty() || uses_advanced_strategy_flow(scene))
                .map(|scene| {
                    let scene_index = self.battle.current_scene_index;
                    let executed_control_count = *self
                        .battle
                        .advanced_control_indices
                        .get(&scene_index)
                        .unwrap_or(&0);
                    if self.battle.advanced_startup_done.contains(&scene_index) {
                        let startup_control_count = *self
                            .battle
                            .advanced_startup_control_indices
                            .get(&scene_index)
                            .unwrap_or(&executed_control_count);
                        self.advanced_party_supports_after_startup_flow(
                            scene,
                            executed_control_count,
                            startup_control_count,
                        )
                    } else {
                        self.advanced_party_supports_after_control(scene, executed_control_count)
                    }
                })
                .unwrap_or_else(|| {
                    let (_, supports) = frontline_party_ids_and_supports(&frontline_party_members(
                        &self.build_full_party_members(),
                    ));
                    supports
                });
            let Some((cards, nps)) = self.read_attack_state(&party_ids, true, false) else {
                return;
            };
            self.handle_advanced_attack(cards, nps, party_ids, party_supports);
            return;
        }

        let party_members = self.normal_current_party_members();
        let party_ids = std::array::from_fn(|index| {
            party_members[index]
                .as_ref()
                .map(|member| member.servant_id)
        });
        let party_supports = std::array::from_fn(|index| {
            party_members[index]
                .as_ref()
                .map(|member| member.is_support)
                .unwrap_or(false)
        });
        let current_turn = normal_turn_for_current_scene(
            &self.scenes,
            self.battle.current_scene_index,
            self.battle.current_turn_index,
        )
        .cloned();
        let recognize_command_cards = normal_scenes_need_command_card_recognition(&self.scenes)
            || self.config.prefer_higher_critical_chance;
        let tolerate_missing_owners = current_turn
            .as_ref()
            .is_some_and(|turn| turn.attack_mode == AttackMode::Critical)
            || self.config.prefer_higher_critical_chance;
        let Some((cards, nps)) =
            self.read_attack_state(&party_ids, recognize_command_cards, tolerate_missing_owners)
        else {
            return;
        };
        match current_turn
            .as_ref()
            .map(|turn| turn.attack_mode)
            .unwrap_or_default()
        {
            AttackMode::Normal => {
                self.emit("Attack", "普通模式：按配置的攻击优先级选卡");
                self.pick_and_tap_attack_cards(&cards, &nps, &party_ids, &party_supports, None);
            }
            AttackMode::Critical => {
                let strategy = current_turn
                    .as_ref()
                    .map(|turn| &turn.critical_strategy)
                    .cloned()
                    .unwrap_or_default();
                let (picks, summary) = choose_critical_picks(&cards, &party_members, &strategy);
                let selection = match summary.bonus {
                    Some(CriticalBonusType::MightyChain) => "精湛连携 +20% 暴击率",
                    Some(CriticalBonusType::QuickChain) => "迅击连携 +20% 暴击率",
                    Some(CriticalBonusType::QuickFirst) => "首张迅击 +20% 暴击率",
                    None => match summary.chain {
                        Some(CriticalChainType::Buster) => "力击连携",
                        Some(CriticalChainType::Arts) => "技击连携",
                        _ => "无额外暴击率加成",
                    },
                };
                self.emit(
                    "Attack",
                    &format!(
                        "暴击模式：{selection}，成员顺序 {}",
                        if summary.owner_labels.is_empty() {
                            "待补位".into()
                        } else {
                            summary.owner_labels.join(" → ")
                        }
                    ),
                );
                if summary.relaxed_alternation {
                    self.emit_warn(
                        "Attack",
                        &format!(
                            "暴击模式因{}，放宽相邻成员限制并补足选卡",
                            summary.relaxed_reason.unwrap_or("无法确认可交错的三张卡")
                        ),
                    );
                }
                self.tap_picks("Attack", &picks, &cards, &party_ids);
            }
            AttackMode::Advanced => {
                let strategy = current_turn
                    .as_ref()
                    .map(|turn| &turn.advanced_card_strategy)
                    .cloned()
                    .unwrap_or_default();
                let (picks, matched_rule) = choose_ordinary_advanced_picks_with_crit(
                    &cards,
                    &nps,
                    &party_members,
                    &strategy,
                    self.config.prefer_higher_critical_chance,
                );
                if let Some(rule) = matched_rule {
                    self.emit("Attack", &format!("高级模式：命中 {rule}"));
                } else {
                    self.emit("Attack", "高级模式：无规则命中，按可行动卡从左至右补位");
                }
                self.tap_picks("Attack", &picks, &cards, &party_ids);
            }
        }
    }

    fn ensure_battle_speed_level_two(&mut self) -> bool {
        let mut switch_requested = false;
        loop {
            if self.is_cancelled() {
                return false;
            }
            let speed_two = match self.sidecar().find_element_by_name(
                None,
                ATTACK_SCREEN,
                ATTACK_SCREEN_SPEED_2_ELEMENT,
            ) {
                Ok(matched) => matched.found,
                Err(err) => {
                    self.fail_action("Attack", "检测战斗速度", err);
                    return false;
                }
            };
            if speed_two {
                return true;
            }
            let speed_one = match self.sidecar().find_element_by_name(
                None,
                ATTACK_SCREEN,
                ATTACK_SCREEN_SPEED_1_ELEMENT,
            ) {
                Ok(matched) => matched.found,
                Err(err) => {
                    self.fail_action("Attack", "检测战斗速度", err);
                    return false;
                }
            };
            if speed_one && !switch_requested {
                self.emit("Attack", "检测到战斗速度为一级，现在切换为二级。");
                if !self.tap_at("Attack", ATTACK_SCREEN_SPEED_BUTTON) {
                    return false;
                }
                switch_requested = true;
            }
            thread::sleep(SKILL_POLL_INTERVAL);
        }
    }

    pub(crate) fn advanced_current_party_ids(
        &self,
        scene: &AdvancedBattleScene,
    ) -> [Option<u32>; 3] {
        let scene_index = self.battle.current_scene_index;
        let executed_control_count = *self
            .battle
            .advanced_control_indices
            .get(&scene_index)
            .unwrap_or(&0);
        if self.battle.advanced_startup_done.contains(&scene_index) {
            let startup_control_count = *self
                .battle
                .advanced_startup_control_indices
                .get(&scene_index)
                .unwrap_or(&executed_control_count);
            self.advanced_party_ids_after_startup_flow(
                scene,
                executed_control_count,
                startup_control_count,
            )
        } else {
            self.advanced_party_ids_after_control(scene, executed_control_count)
        }
    }

    pub(crate) fn read_attack_state(
        &mut self,
        party_ids: &[Option<u32>; 3],
        recognize_command_cards: bool,
        tolerate_missing_owners: bool,
    ) -> Option<(Vec<CommandCardMatch>, Vec<NoblePhantasmMatch>)> {
        let tolerate_missing_owners =
            tolerate_missing_owners || self.config.prefer_higher_critical_chance;
        let recognize_critical_chance = should_log_command_card_crit_chances(
            self.config.prefer_higher_critical_chance,
            self.current_attack_is_critical(),
        );
        let recognition_settle_delay =
            command_card_recognition_settle_delay(recognize_critical_chance);
        if !recognition_settle_delay.is_zero() {
            self.emit("Attack", "等待 1 秒后识别指令卡暴击率");
            thread::sleep(recognition_settle_delay);
        }

        // Start with the expected front line for speed. If owner recognition
        // repeatedly fails, a servant probably died and a back-line member
        // moved forward, so broaden the template candidates to the full team.
        let full_candidate_ids = command_card_candidate_ids(&self.build_full_party_ids());
        let configured_frontline_count = party_ids.iter().flatten().count();
        let mut candidate_ids = if self.battle.command_card_owner_fallback_to_full_party {
            full_candidate_ids.clone()
        } else {
            command_card_candidate_ids(party_ids)
        };

        let cards = if recognize_command_cards {
            self.emit_attack(
                &format!("指令卡候选从者: {:?}", candidate_ids),
                AttackLogMeta {
                    front_servant_ids: *party_ids,
                    candidate_servant_ids: Some(candidate_ids.clone()),
                    command_cards: None,
                    ready_np_slots: None,
                    selected_pick: None,
                },
            );

            if self.assets_dir.is_none() {
                self.emit("Attack", "未找到从者资源目录，将无法按从者匹配指令卡");
            }

            let assets_dir = self.assets_dir.clone();
            loop {
                let used_full_party_candidates =
                    self.battle.command_card_owner_fallback_to_full_party;
                let cards = match self.sidecar().find_command_cards(
                    None,
                    None,
                    &candidate_ids,
                    assets_dir.as_deref(),
                ) {
                    Ok(c) => c,
                    Err(err) => {
                        self.fail_action("Attack", "识别指令卡", err);
                        return None;
                    }
                };
                if !command_cards_visible(&cards) {
                    if self.is_cancelled() {
                        return None;
                    }
                    self.emit("Attack", "指令卡尚未完全出现，等待卡面稳定后重试");
                    thread::sleep(ACTION_DELAY);
                    continue;
                }
                if !should_retry_command_card_owner_detection(
                    &cards,
                    &candidate_ids,
                    configured_frontline_count,
                ) {
                    if configured_frontline_count < 3
                        && cards
                            .iter()
                            .any(|card| !card.is_stunned && card.servant_id.is_none())
                    {
                        self.emit_warn(
                            "Attack",
                            "当前从者配置不满三位，跳过强制识别所有指令卡归属。",
                        );
                    }
                    if !self.battle.command_card_owner_fallback_to_full_party {
                        self.battle.command_card_owner_failure_count = 0;
                    }
                    break cards;
                }
                if tolerate_missing_owners && used_full_party_candidates {
                    self.emit_warn(
                        "Attack",
                        "警告：全队候选仍有指令卡成员未识别，将按未识别成员的原卡位处理",
                    );
                    break cards;
                }
                if !self.battle.command_card_owner_fallback_to_full_party {
                    self.battle.command_card_owner_failure_count += 1;
                }
                if self.battle.command_card_owner_failure_count
                    >= COMMAND_CARD_FRONTLINE_OWNER_FAILURE_LIMIT
                    && !self.battle.command_card_owner_fallback_to_full_party
                {
                    self.battle.command_card_owner_fallback_to_full_party = true;
                    candidate_ids = full_candidate_ids.clone();
                    self.emit_warn(
                        "Attack",
                        "警告：指令卡归属已连续 3 次识别失败，本场战斗后续将改为检测全队六人",
                    );
                    self.emit_attack(
                        "指令卡归属连续识别失败，改用全队六人候选",
                        AttackLogMeta {
                            front_servant_ids: *party_ids,
                            candidate_servant_ids: Some(candidate_ids.clone()),
                            command_cards: None,
                            ready_np_slots: None,
                            selected_pick: None,
                        },
                    );
                }
                if self.is_cancelled() {
                    return None;
                }
                self.emit("Attack", "指令卡从者未识别，等待卡面稳定后重试");
                thread::sleep(ACTION_DELAY);
            }
        } else {
            let cards = loop {
                let cards = match self.sidecar().find_command_cards(None, None, &[], None) {
                    Ok(c) => c,
                    Err(err) => {
                        self.fail_action("Attack", "等待指令卡出现", err);
                        return None;
                    }
                };
                if command_cards_visible(&cards) {
                    break cards;
                }
                if self.is_cancelled() {
                    return None;
                }
                self.emit("Attack", "指令卡尚未完全出现，等待卡面稳定后重试");
                thread::sleep(ACTION_DELAY);
            };
            self.emit("Attack", "未配置普通指令卡，跳过指令卡归属识别");
            cards
        };
        if recognize_critical_chance {
            self.emit(
                "Attack",
                &format!("指令卡暴击率：{}", format_command_card_crit_chances(&cards)),
            );
        }
        if should_capture_unrecognized_critical_chance(
            self.config.auto_capture_unrecognized_critical_chance,
            recognize_critical_chance,
            &cards,
        ) {
            match self.capture_unrecognized_critical_chance_screenshot() {
                Ok(path) => self.emit(
                    "Attack",
                    &format!("无法识别暴击率截图已保存: {}", path.display()),
                ),
                Err(err) => self.emit_warn(
                    "Attack",
                    &format!("无法识别暴击率截图保存失败，继续选卡: {err}"),
                ),
            }
        }
        let np_detection_mode = self.config.noble_phantasm_detection_mode;
        let recognize_noble_phantasms = self.current_attack_requires_np_recognition();
        let nps = if !recognize_noble_phantasms {
            self.battle.pre_attack_nps = None;
            Vec::new()
        } else {
            match np_detection_mode {
                NoblePhantasmDetectionMode::Card => loop {
                    let mut nps = match self.sidecar().find_noble_phantasms(None, None) {
                        Ok(n) => n,
                        Err(err) => {
                            self.fail_action("Attack", "识别宝具卡", err);
                            return None;
                        }
                    };
                    apply_np_detection_mode(&mut nps, np_detection_mode);

                    if np_card_read_complete(&nps) {
                        break nps;
                    }
                    if self.is_cancelled() {
                        return None;
                    }
                    self.emit("Attack", "宝具卡尚未识别完整，等待卡面稳定后重试");
                    thread::sleep(ACTION_DELAY);
                },
                NoblePhantasmDetectionMode::Gauge => {
                    let Some(nps) = self.read_noble_phantasm_gauges("Attack", false) else {
                        return None;
                    };
                    nps
                }
                NoblePhantasmDetectionMode::GaugeBeforeAttack => {
                    let Some(nps) = self.battle.pre_attack_nps.clone() else {
                        self.fail_action(
                            "Attack",
                            "读取攻击前宝具条",
                            "未找到攻击前缓存的宝具条识别结果".into(),
                        );
                        return None;
                    };
                    nps
                }
            }
        };

        if recognize_command_cards {
            let card_summary: Vec<String> = cards
                .iter()
                .map(|c| {
                    let owner = c
                        .servant_id
                        .and_then(|id| {
                            Some(format!("{id}{}", if c.is_support { "[支]" } else { "" }))
                        })
                        .unwrap_or_else(|| "未识别".into());
                    format!(
                        "C{}={}/{}",
                        c.slot + 1,
                        c.suit.as_deref().unwrap_or("?"),
                        owner,
                    )
                })
                .collect();
            self.emit_attack(
                &format!("指令卡: {}", card_summary.join(" ")),
                AttackLogMeta {
                    front_servant_ids: *party_ids,
                    candidate_servant_ids: None,
                    command_cards: Some(command_cards_log_meta(&cards)),
                    ready_np_slots: None,
                    selected_pick: None,
                },
            );
        }
        if recognize_noble_phantasms {
            let ready: Vec<String> = nps
                .iter()
                .filter(|n| n.ready)
                .map(|n| format!("NP{}", n.slot + 1))
                .collect();
            let ready_np_slots: Vec<u32> = nps.iter().filter(|n| n.ready).map(|n| n.slot).collect();
            if ready.is_empty() {
                self.emit_attack(
                    "宝具就绪: 无",
                    AttackLogMeta {
                        front_servant_ids: *party_ids,
                        candidate_servant_ids: None,
                        command_cards: None,
                        ready_np_slots: Some(ready_np_slots),
                        selected_pick: None,
                    },
                );
            } else {
                self.emit_attack(
                    &format!("宝具就绪: {}", ready.join(" ")),
                    AttackLogMeta {
                        front_servant_ids: *party_ids,
                        candidate_servant_ids: None,
                        command_cards: None,
                        ready_np_slots: Some(ready_np_slots),
                        selected_pick: None,
                    },
                );
            }
        }

        if reads_np_gauge_before_attack(np_detection_mode) {
            self.battle.pre_attack_nps = None;
        }

        Some((cards, nps))
    }
}
