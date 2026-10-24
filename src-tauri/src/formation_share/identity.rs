use serde_json::Value;
use std::collections::HashSet;

pub(super) const REFERENCES: &[&str] = &[
    "memberId",
    "servantMemberId",
    "targetMemberId",
    "frontMemberId",
    "backMemberId",
];

pub(super) fn validate(entry: &Value) -> Result<(), String> {
    let project = &entry["project"];
    let name = project["name"].as_str().unwrap_or_default().trim();
    if name.is_empty() {
        return Err("队伍名称为空".into());
    }
    let slots = project["slots"].as_array().ok_or("编队缺少成员列表")?;
    if slots.len() != 6
        || slots
            .iter()
            .filter(|slot| slot["type"] == "support")
            .count()
            != 1
    {
        return Err("编队必须有六个槽位及一个助战槽位".into());
    }
    let mut members = HashSet::new();
    for slot in slots {
        let id = slot["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 100)
            .ok_or("编队成员标识无效")?;
        if !members.insert(id) {
            return Err("编队成员标识重复".into());
        }
        if !matches!(slot["type"].as_str(), Some("servant" | "support")) {
            return Err("无法识别的编队成员类型".into());
        }
        if let Some(ids) = slot["craftEssenceIds"].as_array() {
            if ids.len() > 10
                || (ids.len() > 1
                    && (slot["type"] != "support" || slot["craftEssenceMultiSelect"] != true))
            {
                return Err("礼装候选数量或多选配置不一致".into());
            }
        }
    }
    let mut nodes = 0;
    visit(entry, &members, &mut nodes, 0)?;
    for key in ["battleScenes", "advancedBattleScenes"] {
        let Some(value) = entry.get(key) else {
            continue;
        };
        let scenes = value.as_array().ok_or("战斗面列表结构无效")?;
        if scenes.len() > 100 {
            return Err("编队波次数超过限制".into());
        }
        for scene in scenes {
            if !scene.is_object() {
                return Err("战斗面配置结构无效".into());
            }
            if let Some(turns) = scene.get("turns") {
                let turns = turns.as_array().ok_or("回合列表结构无效")?;
                if turns.iter().any(|turn| !turn.is_object()) {
                    return Err("回合配置结构无效".into());
                }
            }
        }
    }
    Ok(())
}

fn visit(
    value: &Value,
    members: &HashSet<&str>,
    nodes: &mut usize,
    depth: usize,
) -> Result<(), String> {
    *nodes += 1;
    if *nodes > 25_000 || depth > 64 {
        return Err("编队配置结构超过限制".into());
    }
    match value {
        Value::Object(object) => {
            for key in REFERENCES {
                if let Some(reference) = object.get(*key).filter(|v| !v.is_null()) {
                    let id = reference.as_str().ok_or("成员引用格式错误")?;
                    if !members.contains(id) {
                        return Err(format!("成员引用不存在：{id}"));
                    }
                }
            }
            for value in object.values() {
                visit(value, members, nodes, depth + 1)?;
            }
        }
        Value::Array(values) => {
            if values.len() > 1000 {
                return Err("编队中的行动或规则数量超过限制".into());
            }
            for value in values {
                visit(value, members, nodes, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
