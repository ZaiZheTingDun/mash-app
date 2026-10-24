//! Read-only, localized projection. Catalogs enrich labels; unresolved IDs are
//! retained and reported rather than being removed from a received strategy.

use super::*;
use std::collections::HashMap;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FormationSharePreview {
    pub token: String,
    pub source_name: String,
    pub target_name: String,
    pub advanced_mode: bool,
    pub members: Vec<FormationShareMember>,
    pub support_requirements: Vec<String>,
    pub sections: Vec<FormationShareSection>,
    pub warnings: Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FormationShareMember {
    position: usize,
    name: String,
    is_support: bool,
    craft_essences: Vec<String>,
    mlb_required: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FormationShareSection {
    title: String,
    lines: Vec<String>,
}

pub(super) fn build_preview(
    entry: &ConfigExportEntry,
    token: String,
    target_name: String,
    server: Server,
    mystic_code_name: Option<String>,
) -> FormationSharePreview {
    let servants = crate::commands::catalog::servants_data();
    let ces = crate::commands::catalog::craft_essences_data();
    let mut warnings = Vec::new();
    if servants.is_err() || ces.is_err() {
        warnings.push("本地资源目录尚不可用，部分名称以编号显示，导入后请核对资源。".into());
    }
    let ce_name = |id: u32| {
        ces.as_ref()
            .ok()
            .and_then(|items| items.iter().find(|ce| ce.id == id))
            .map(|ce| ce.name.clone())
            .unwrap_or_else(|| format!("礼装 #{id}"))
    };
    let mut labels = HashMap::new();
    let mut positions = Vec::new();
    let members = entry
        .project
        .slots
        .iter()
        .enumerate()
        .map(|(index, slot)| {
            let support = slot.kind == "support";
            let id = if support {
                entry.project.support_servant_id
            } else {
                slot.servant_id
            };
            let variant = if support {
                entry.project.support_servant_variant_key.as_deref()
            } else {
                slot.servant_variant_key.as_deref()
            };
            let servant = servants.as_ref().ok().and_then(|items| {
                items
                    .iter()
                    .find(|s| Some(s.variant_key.as_str()) == variant && Some(s.id) == id)
                    .or_else(|| items.iter().find(|s| Some(s.id) == id))
            });
            let name = servant
                .map(|s| match server {
                    Server::Cn => s
                        .name_cn_server
                        .as_deref()
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or(&s.name_cn)
                        .to_string(),
                    Server::Jp => s.name_jp.clone(),
                })
                .unwrap_or_else(|| {
                    id.map(|id| format!("从者 #{id}"))
                        .unwrap_or_else(|| "空槽位".into())
                });
            if id.is_some() && servant.is_none() {
                warnings.push(format!(
                    "第 {} 位的从者资源无法识别，已保留原编号，请更新资源或手动核对。",
                    index + 1
                ));
            }
            if id.is_none() {
                warnings.push(format!("第 {} 位尚未选择从者。", index + 1));
            }
            if variant.is_some() && servant.is_some_and(|s| Some(s.variant_key.as_str()) != variant)
            {
                warnings.push(format!(
                    "第 {} 位的从者变体资源无法识别，已保留原变体标识。",
                    index + 1
                ));
            }
            let label = format!(
                "第{}位{} · {}",
                index + 1,
                if support { "（助战）" } else { "" },
                name
            );
            labels.insert(slot.id.clone(), label.clone());
            positions.push(label);
            let ids = if slot.craft_essence_ids.is_empty() {
                slot.craft_essence_id.into_iter().collect()
            } else {
                slot.craft_essence_ids.clone()
            };
            for id in &ids {
                if ces
                    .as_ref()
                    .ok()
                    .is_none_or(|items| !items.iter().any(|ce| ce.id == *id))
                {
                    warnings.push(format!(
                        "礼装 #{id} 在本地资源目录中无法识别，已保留原编号。"
                    ));
                }
            }
            FormationShareMember {
                position: index + 1,
                name,
                is_support: support,
                craft_essences: ids.into_iter().map(ce_name).collect(),
                mlb_required: slot.craft_essence_mlb_required,
            }
        })
        .collect();
    let context = Labels {
        members: labels,
        positions,
    };
    let mut sections = Vec::new();
    if let Some(id) = entry.project.mystic_code_id {
        sections.push(FormationShareSection {
            title: "御主礼装".into(),
            lines: vec![mystic_code_name.unwrap_or_else(|| format!("御主礼装 #{id}"))],
        });
    }
    for (wave, scene) in entry.battle_scenes.iter().enumerate() {
        for (turn, data) in scene.turns.iter().enumerate() {
            let value = serde_json::to_value(data).unwrap_or(Value::Null);
            let mut lines = action_lines(&value["preparationActions"], &context);
            if let Some(target) = value["enemyTarget"].as_str() {
                lines.insert(0, format!("选择敌方目标：{target}"));
            }
            if let Some(cards) = value["attackPriority"].as_array() {
                let cards = cards
                    .iter()
                    .filter(|card| card["card"].is_string())
                    .map(|card| context.card(card))
                    .collect::<Vec<_>>();
                if !cards.is_empty() {
                    lines.push(format!("出牌顺序：{}", cards.join(" → ")));
                }
            }
            if data.attack_mode != AttackMode::Normal {
                lines.push(format!(
                    "出牌模式：{}",
                    if data.attack_mode == AttackMode::Critical {
                        "暴击"
                    } else {
                        "自定义"
                    }
                ));
            }
            if data.attack_mode == AttackMode::Critical {
                strategy_lines(&value["criticalStrategy"], &context, &mut lines);
            }
            if data.attack_mode == AttackMode::Advanced {
                strategy_lines(&value["advancedCardStrategy"], &context, &mut lines);
            }
            sections.push(FormationShareSection {
                title: format!("普通指令 · 第{}面 · 第{}回合", wave + 1, turn + 1),
                lines,
            });
        }
    }
    for (wave, scene) in entry.advanced_battle_scenes.iter().enumerate() {
        let value = serde_json::to_value(scene).unwrap_or(Value::Null);
        let mut lines = action_lines(&value["controlActions"], &context);
        if let Some(target) = scene.enemy_target.as_ref() {
            lines.push(format!("敌方目标：{target}"));
        }
        if scene.main_output.is_some() {
            lines.push(format!(
                "主输出：{} · {} · 宝具 {}",
                context.member(&value["mainOutput"], "memberId", "servant"),
                match value["mainOutput"]["outputType"].as_str() {
                    Some("np") => "宝具输出",
                    Some("critical") => "暴击输出",
                    _ => "未指定输出",
                },
                color(value["mainOutput"]["npCard"].as_str().unwrap_or("auto"))
            ));
        }
        if let Some(change) = scene.grand_auto_order_change {
            lines.push(format!(
                "冠位自动换人：{}",
                if change { "开启" } else { "关闭" }
            ));
        }
        condition_lines(&value["commandConditions"], &context, &mut lines);
        sections.push(FormationShareSection {
            title: format!("高级指令 · 第{}面", wave + 1),
            lines,
        });
        for (turn, data) in scene.turns.iter().enumerate() {
            let actions = serde_json::to_value(&data.actions).unwrap_or(Value::Null);
            sections.push(FormationShareSection {
                title: format!("高级指令 · 第{}面 · 第{}回合", wave + 1, turn + 1),
                lines: action_lines(&actions, &context),
            });
        }
        for (index, rule) in scene.rules.iter().enumerate() {
            let rule = serde_json::to_value(rule).unwrap_or(Value::Null);
            let mut lines = Vec::new();
            if let Some(groups) = rule["npConditionGroups"].as_array() {
                for (index, group) in groups.iter().enumerate() {
                    if let Some(slots) = group["slots"].as_array() {
                        let slots = slots
                            .iter()
                            .map(|slot| {
                                format!(
                                    "{}：宝具{}",
                                    context.member(slot, "memberId", "servant"),
                                    if slot["ready"] == true {
                                        "就绪"
                                    } else {
                                        "未就绪"
                                    }
                                )
                            })
                            .collect::<Vec<_>>();
                        lines.push(format!("宝具条件组 {}：{}", index + 1, slots.join("；")));
                    }
                }
            }
            if let Some(groups) = rule["commandConditionGroups"].as_array() {
                for (index, group) in groups.iter().enumerate() {
                    lines.push(format!("指令卡条件组 {}", index + 1));
                    condition_lines(&group["cards"], &context, &mut lines);
                }
            }
            lines.extend(action_lines(&rule["actions"], &context));
            sections.push(FormationShareSection {
                title: format!("高级指令 · 第{}面 · 规则{}", wave + 1, index + 1),
                lines,
            });
        }
    }
    let strategy = serde_json::to_value(&entry.project.grand_card_strategy).unwrap_or(Value::Null);
    let mut grand = entry
        .project
        .grand_servants
        .iter()
        .map(|member| {
            let value = serde_json::to_value(member).unwrap_or(Value::Null);
            format!(
                "{} · {} · 宝具 {} · 优先 {}",
                context.member(&value, "memberId", "servant"),
                match member.role.as_deref() {
                    Some("main") => "主冠位",
                    Some("deputy") => "副冠位",
                    Some("single") => "单体",
                    Some("aoe") => "全体",
                    Some(other) => other,
                    None => "未分配角色",
                },
                color(&member.np_card),
                color(&member.priority)
            )
        })
        .collect::<Vec<_>>();
    if entry.project.advanced_mode
        || entry.project.support_grand_mode
        || !grand.is_empty()
        || !entry.project.grand_card_strategy.custom_rules.is_empty()
    {
        strategy_lines(&strategy, &context, &mut grand);
    }
    if !grand.is_empty() {
        sections.push(FormationShareSection {
            title: grand_class_definitions()
                .into_iter()
                .find(|definition| definition.id == entry.project.grand_class)
                .map(|definition| format!("冠位与选卡策略 · {}", definition.label))
                .unwrap_or_else(|| "冠位与选卡策略".into()),
            lines: grand,
        });
    }
    if sections.iter().all(|section| section.lines.is_empty()) {
        warnings.push("尚未配置完整行动，可导入后继续编辑。".into());
    }
    let mut requirements = Vec::new();
    for (label, value) in [
        ("等级", entry.project.support_servant_level_min),
        ("宝具", entry.project.support_noble_phantasm_level_min),
        ("星图", entry.project.support_star_map_score_min),
        ("冠位星图", entry.project.support_grand_star_map_score_min),
    ] {
        if let Some(value) = value {
            requirements.push(format!("{label}至少 {value}"));
        }
    }
    for (label, levels) in [
        ("技能", entry.project.support_skill_level_mins.as_slice()),
        (
            "追加技能",
            entry.project.support_append_skill_level_mins.as_slice(),
        ),
    ] {
        if levels.iter().any(Option::is_some) {
            requirements.push(format!(
                "{label}：{}",
                levels
                    .iter()
                    .map(|v| v.map(|v| v.to_string()).unwrap_or_else(|| "任意".into()))
                    .collect::<Vec<_>>()
                    .join(" / ")
            ));
        }
    }
    if entry.project.support_grand_mode {
        requirements.push("冠位助战".into());
        for (index, ids) in entry
            .project
            .support_grand_craft_essence_id_lists
            .iter()
            .enumerate()
        {
            if !ids.is_empty() {
                requirements.push(format!(
                    "冠位礼装 {}：{}{}",
                    index + 1,
                    ids.iter()
                        .copied()
                        .map(ce_name)
                        .collect::<Vec<_>>()
                        .join(" / "),
                    if entry.project.support_grand_craft_essence_mlb_required[index] {
                        "（满破）"
                    } else {
                        ""
                    }
                ));
            }
        }
        requirements.push(format!(
            "羁绊礼装模式：{}",
            match entry.project.support_grand_bond_ce_mode {
                SupportGrandBondCeMode::Any => "任意",
                SupportGrandBondCeMode::Bond => "羁绊",
                SupportGrandBondCeMode::BondNp => "羁绊与宝具",
            }
        ));
    }
    if entry.project.disable_auto_skill_target_recognition {
        requirements.push("关闭自动技能目标识别".into());
    }
    if entry.project.prefer_higher_critical_chance {
        requirements.push("同类卡优先较高暴击率".into());
    }
    if let Some(mode) = entry
        .project
        .recognition_settings
        .and_then(|r| r.support_class_filter_mode)
    {
        requirements.push(format!(
            "助战职阶筛选：{}",
            match mode {
                SupportClassFilterMode::CorrespondingClass => "对应职阶",
                SupportClassFilterMode::AllClasses => "全部职阶",
                SupportClassFilterMode::Both => "两者均可",
            }
        ));
    }
    warnings.sort();
    warnings.dedup();
    FormationSharePreview {
        token,
        source_name: entry.project.name.clone(),
        target_name,
        advanced_mode: entry.project.advanced_mode,
        members,
        support_requirements: requirements,
        sections,
        warnings,
    }
}

struct Labels {
    members: HashMap<String, String>,
    positions: Vec<String>,
}

impl Labels {
    fn member(&self, value: &Value, key: &str, fallback: &str) -> String {
        if value["grandServant"] == true {
            return "冠位从者".into();
        }
        if value[fallback] == "any" {
            return "任意从者".into();
        }
        value[key]
            .as_str()
            .and_then(|id| self.members.get(id))
            .cloned()
            .or_else(|| {
                value[fallback]
                    .as_str()?
                    .strip_prefix("servant_")?
                    .split('_')
                    .next()?
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| self.positions.get(i))
                    .cloned()
            })
            .unwrap_or_else(|| "未指定成员".into())
    }
    fn card(&self, value: &Value) -> String {
        let card = value["card"].as_str().unwrap_or("未指定出牌");
        let owner = self.member(value, "memberId", "card");
        if card.contains("np") {
            format!("{owner} · 宝具")
        } else {
            format!(
                "{owner} · {}",
                color(card.rsplit('_').next().unwrap_or(card))
            )
        }
    }
}

fn color(value: &str) -> &str {
    match value {
        "buster" => "红卡",
        "arts" => "蓝卡",
        "quick" => "绿卡",
        "any" | "all" => "任意",
        "auto" => "自动",
        "damage" => "伤害",
        "np" => "宝具",
        "mighty" => "三色连携",
        "mainBraveChain" => "主冠位连携",
        "mainReadyNp" => "主冠位就绪宝具",
        "deputyBraveChain" => "副冠位连携",
        "mainColorChain" => "主冠位同色连携",
        "deputyColorChain" => "副冠位同色连携",
        "fallback" => "其他出牌",
        other => other,
    }
}

fn action_lines(actions: &Value, labels: &Labels) -> Vec<String> {
    actions
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, action)| {
            let target = labels.member(action, "targetMemberId", "target");
            let skill = action["skill"]
                .as_str()
                .and_then(|s| s.strip_prefix("skill_"))
                .unwrap_or("未选择");
            let mut line = match action["type"].as_str() {
                Some("servant") => format!(
                    "{} · 技能{skill} → {target}",
                    labels.member(action, "servantMemberId", "servant")
                ),
                Some("equipment") if action["orderChange"].is_object() => format!(
                    "御主换人：{} ↔ {}",
                    labels.member(&action["orderChange"], "frontMemberId", "front"),
                    labels.member(&action["orderChange"], "backMemberId", "back")
                ),
                Some("equipment") => format!("御主技能{skill} → {target}"),
                Some("commandSpell") => format!(
                    "令咒 · {} → {target}",
                    if action["spell"] == "np_release" {
                        "宝具解放"
                    } else {
                        "灵基修复"
                    }
                ),
                Some("enemyTarget") => format!(
                    "选择敌方目标：{}",
                    action["target"].as_str().unwrap_or("未指定")
                ),
                Some("attack") => labels.card(action),
                _ => "尚未完成的行动".into(),
            };
            if let Some(selection) = action.get("skillSelection").filter(|v| v.is_object()) {
                line.push_str(&format!(
                    " · {}（选择 {}）",
                    selection["label"].as_str().unwrap_or("技能追加选择"),
                    selection["index"]
                ));
            }
            format!("{}. {line}", index + 1)
        })
        .collect()
}

fn condition_lines(values: &Value, labels: &Labels, lines: &mut Vec<String>) {
    for value in values.as_array().into_iter().flatten() {
        lines.push(format!(
            "指令卡 {}：{} · {}{}",
            value["slot"].as_u64().unwrap_or(0) + 1,
            labels.member(value, "memberId", "servant"),
            color(value["suit"].as_str().unwrap_or("any")),
            value["minCritChance"]
                .as_u64()
                .map(|v| format!(" · 暴击率至少 {v}%"))
                .unwrap_or_default()
        ));
    }
}

fn strategy_lines(strategy: &Value, labels: &Labels, lines: &mut Vec<String>) {
    if let Some(priority) = strategy["chainPriority"]
        .as_array()
        .filter(|p| !p.is_empty())
    {
        lines.push(format!(
            "连携优先级：{}",
            priority
                .iter()
                .filter_map(Value::as_str)
                .map(color)
                .collect::<Vec<_>>()
                .join(" → ")
        ));
    }
    if let Some(priority) = strategy["memberPriority"]
        .as_array()
        .filter(|p| !p.is_empty())
    {
        lines.push(format!(
            "成员优先级：{}",
            priority
                .iter()
                .map(|v| labels.member(v, "memberId", "servant"))
                .collect::<Vec<_>>()
                .join(" → ")
        ));
    }
    if let Some(rules) = strategy["customRules"].as_array() {
        for (index, rule) in rules.iter().enumerate() {
            let cards = rule["slots"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|slot| {
                    format!(
                        "{} · {} · {}",
                        labels.member(slot, "memberId", "servant"),
                        match slot["kind"].as_str() {
                            Some("np") => "宝具",
                            Some("command") => "指令卡",
                            _ => "任意出牌",
                        },
                        color(slot["color"].as_str().unwrap_or("any"))
                    )
                })
                .collect::<Vec<_>>();
            lines.push(format!(
                "选卡规则 {} {}：{}",
                index + 1,
                rule["name"].as_str().unwrap_or_default(),
                cards.join(" → ")
            ));
        }
    }
}
