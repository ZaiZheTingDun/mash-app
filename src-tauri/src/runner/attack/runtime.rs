//! Device-facing attack flow orchestration.
//!
//! Selection and condition policies remain in the parent module; this module
//! owns screen reads, taps, retries, and battle-state transitions.

use super::*;

impl Runner {
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

    fn current_attack_requires_np_recognition(&self) -> bool {
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
                let sample = match self.sidecar().find_noble_phantasms(None, None) {
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
            let simulate_missed_tap = screen == "Attack"
                && selected_count == 2
                && match consume_simulate_stuck_attack_selection(&self.app_handle) {
                    Ok(enabled) => enabled,
                    Err(err) => {
                        self.emit_warn(
                            screen,
                            &format!("读取选卡卡住测试开关失败，本次正常选卡: {err}"),
                        );
                        false
                    }
                };
            if simulate_missed_tap {
                self.emit_warn(
                    screen,
                    "调试测试：已故意跳过第 3 张卡的点击，等待触发选卡恢复",
                );
            } else if !self.tap_at(screen, point) {
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
