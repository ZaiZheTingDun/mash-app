//! Member IDs are authoritative within local documents. Shared documents must
//! agree with their own lineup before any metadata is accepted.

use super::*;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

pub(super) fn canonicalize_member_metadata(
    value: &mut Value,
    project: &Value,
    refresh: bool,
) -> Result<(), String> {
    clear_empty_attacks(value);
    let slots = project["slots"].as_array().ok_or("缺少成员列表")?;
    let members: HashMap<String, (Option<u64>, bool)> = slots
        .iter()
        .map(|slot| {
            let support = slot["type"] == "support";
            let servant = if support {
                &project["supportServantId"]
            } else {
                &slot["servantId"]
            };
            (
                slot["id"].as_str().unwrap_or_default().to_string(),
                (servant.as_u64(), support),
            )
        })
        .collect();
    walk(value, &members, refresh, "编队")
}

fn walk(
    value: &mut Value,
    members: &HashMap<String, (Option<u64>, bool)>,
    refresh: bool,
    path: &str,
) -> Result<(), String> {
    match value {
        Value::Object(object) => {
            if object.get("grandServant") == Some(&Value::Bool(true))
                && object.get("memberId").is_some_and(|v| !v.is_null())
            {
                return Err(format!(
                    "{path}：出卡设置冲突：设定任意冠位时不能同时指定具体从者"
                ));
            }
            for (member_key, servant_key, support_key) in [
                ("memberId", "servantId", "isSupport"),
                ("servantMemberId", "servantId", "servantIsSupport"),
                ("targetMemberId", "targetServantId", "targetIsSupport"),
                ("frontMemberId", "frontServantId", "frontIsSupport"),
                ("backMemberId", "backServantId", "backIsSupport"),
            ] {
                if let Some(member) = object.get(member_key).and_then(Value::as_str) {
                    let (servant, support) = members.get(member).ok_or_else(|| {
                        format!("{path}：成员引用不存在（{member_key}：{member}）")
                    })?;
                    if !refresh
                        && (object
                            .get(servant_key)
                            .and_then(Value::as_u64)
                            .is_some_and(|id| Some(id) != *servant)
                            || object
                                .get(support_key)
                                .and_then(Value::as_bool)
                                .is_some_and(|flag| flag != *support))
                    {
                        return Err(format!(
                            "{path}：成员引用与从者或助战身份不一致（{member_key}：{member}）"
                        ));
                    }
                    object.insert(servant_key.into(), json!(servant));
                    object.insert(support_key.into(), json!(support));
                }
            }
            for (key, value) in object.iter_mut() {
                if let Value::Array(values) = value {
                    for (index, value) in values.iter_mut().enumerate() {
                        let position = index + 1;
                        let location = match key.as_str() {
                            "battleScenes" => format!("普通指令 / 第 {position} 面"),
                            "advancedBattleScenes" => format!("冠位指令 / 第 {position} 面"),
                            "turns" => format!("{path} / 第 {position} 回合"),
                            "attackPriority" => format!("{path} / 第 {position} 项出卡"),
                            "preparationActions" | "startupActions" => {
                                format!("{path} / 第 {position} 项技能指令")
                            }
                            "actions" => format!("{path} / 第 {position} 项行动"),
                            _ => format!("{path} / {key}[{position}]"),
                        };
                        walk(value, members, refresh, &location)?;
                    }
                } else {
                    walk(value, members, refresh, &format!("{path} / {key}"))?;
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                walk(value, members, refresh, path)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn clear_empty_attacks(value: &mut Value) {
    match value {
        Value::Object(object) => {
            // Old clears reset only `card`; empty attacks have no member.
            if object.get("card") == Some(&Value::Null) {
                object.insert("memberId".into(), Value::Null);
                object.insert("servantId".into(), Value::Null);
                object.insert("isSupport".into(), Value::Bool(false));
            }
            for value in object.values_mut() {
                clear_empty_attacks(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                clear_empty_attacks(value);
            }
        }
        _ => {}
    }
}

/// Used by editor reads/writes, execution and export. Staging roots used by
/// imports have no published project list yet, so only model normalization is
/// applied there; the receiving installation validates shares before staging.
pub(super) fn normalize_stored_member_metadata<T: Serialize + DeserializeOwned>(
    root: &Path,
    project_id: &str,
    scenes: Vec<T>,
    advanced: bool,
) -> Result<Vec<T>, String> {
    let key = if advanced {
        "advancedBattleScenes"
    } else {
        "battleScenes"
    };
    let mut value = json!({key: scenes});
    clear_empty_attacks(&mut value);
    if let Some(project) = read_projects_from_path(&root.join("projects.json"))?
        .into_iter()
        .find(|project| project.id == project_id)
    {
        canonicalize_member_metadata(&mut value, &json!(project), true)?;
    }
    serde_json::from_value(value[key].take()).map_err(|error| error.to_string())
}
