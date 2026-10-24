//! Application adapter: snapshots, model validation, previews, clipboard, commit.
//! The transport itself has no knowledge of Tauri or the selected server.

use super::*;
use crate::formation_share as transport;
use serde_json::Value;
use tauri_plugin_clipboard_manager::ClipboardExt;

mod preview;
use preview::{build_preview, FormationSharePreview};

const PREVIEW_LIFETIME: Duration = Duration::from_secs(15 * 60);
const MAX_PREVIEWS: usize = 8;

struct PendingPreview {
    expires_at: Instant,
    entry: ConfigExportEntry,
}

#[derive(Default)]
pub(crate) struct FormationSharePreviews(Mutex<HashMap<String, PendingPreview>>);

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FormationShareCode {
    code: String,
    characters: usize,
    copied: bool,
}

fn decode_entry(text: &str) -> Result<ConfigExportEntry, String> {
    let mut value = transport::decode(text)?;
    validate_options(&value)?;
    canonicalize_member_metadata(&mut value)?;
    let mut entry: ConfigExportEntry = serde_json::from_value(value.clone())
        .map_err(|error| format!("无法识别编队中的行动或配置：{error}"))?;
    let modeled = serde_json::to_value(&entry).map_err(|error| error.to_string())?;
    transport::reject_discarded_fields(&value, &modeled)?;
    let roles = grand_class_definitions()
        .into_iter()
        .find(|definition| definition.id == entry.project.grand_class)
        .map(|definition| definition.roles)
        .unwrap_or_default();
    if entry
        .project
        .grand_servants
        .iter()
        .filter_map(|member| member.role.as_ref())
        .any(|role| !roles.iter().any(|known| &known.role == role))
    {
        return Err("不支持的冠位角色，请升级 Mash 或检查配置".into());
    }
    entry.project = normalize_project(entry.project);
    // Running preferences never come from a shared strategy, including after
    // future changes to Project's persistence defaults.
    entry.project.repeat_mission = false;
    entry.project.repeat_mode = Some(ProjectRepeatMode::Single);
    entry.project.repeat_count = None;
    entry.project.ap_recovery_items.clear();
    entry.project.ap_recovery_limits = Default::default();
    entry.battle_scenes = entry
        .battle_scenes
        .into_iter()
        .map(BattleScene::normalize_turns)
        .collect();
    for scene in &mut entry.advanced_battle_scenes {
        if scene.turns.is_empty() && !scene.startup_actions.is_empty() {
            scene.turns.push(AdvancedBattleTurn {
                id: uuid::Uuid::new_v4().to_string(),
                actions: std::mem::take(&mut scene.startup_actions),
            });
        }
    }
    Ok(entry)
}

fn validate_options(value: &Value) -> Result<(), String> {
    if let Some(object) = value.as_object() {
        for (key, choices) in [
            ("skill", &["skill_1", "skill_2", "skill_3"][..]),
            ("spell", &["np_release", "restore"][..]),
            ("npCard", &["auto", "buster", "arts", "quick"][..]),
            ("priority", &["damage", "np"][..]),
            ("kind", &["", "any", "command", "np"][..]),
            ("color", &["", "any", "buster", "arts", "quick"][..]),
            ("suit", &["any", "buster", "arts", "quick"][..]),
        ] {
            if let Some(option) = object.get(key).and_then(Value::as_str) {
                if !choices.contains(&option) {
                    return Err(format!("不支持的配置选项 {key}：{option}，请升级 Mash"));
                }
            }
        }
        if object.contains_key("optionCount")
            || (object.contains_key("index") && object.contains_key("type"))
        {
            if !matches!(
                object.get("type").and_then(Value::as_str),
                Some(
                    "SelectAddInfo" | "selectTreasureDeviceInfo" | "commandTypeSelfTreasureDevice"
                )
            ) {
                return Err("不支持的技能追加选择类型，请升级 Mash".into());
            }
        }
        for (key, max) in [
            ("supportServantLevelMin", 120),
            ("supportNoblePhantasmLevelMin", 5),
            ("supportStarMapScoreMin", 62),
            ("supportGrandStarMapScoreMin", 16),
            ("minCritChance", 100),
        ] {
            if object
                .get(key)
                .and_then(Value::as_u64)
                .is_some_and(|v| v > max)
            {
                return Err(format!("配置参数 {key} 超过允许范围"));
            }
        }
        for key in ["supportSkillLevelMins", "supportAppendSkillLevelMins"] {
            if object
                .get(key)
                .and_then(Value::as_array)
                .is_some_and(|a| a.iter().any(|v| v.as_u64().is_some_and(|v| v > 10)))
            {
                return Err("助战技能要求超过允许范围".into());
            }
        }
        if object
            .get("slotIndex")
            .and_then(Value::as_u64)
            .is_some_and(|v| v > 5)
            || object
                .get("slot")
                .and_then(Value::as_u64)
                .is_some_and(|v| v > 4)
        {
            return Err("配置中的成员或指令卡位置超过范围".into());
        }
        if let Some(card) = object.get("card").and_then(Value::as_str) {
            let parsed = card
                .strip_prefix("servant_")
                .and_then(|s| s.split_once('_'));
            if !parsed.is_some_and(|(member, kind)| {
                member.parse::<u8>().is_ok_and(|n| (1..=6).contains(&n))
                    && ["np", "buster", "arts", "quick", "all"].contains(&kind)
            }) {
                return Err("不支持的出牌类型，请升级 Mash".into());
            }
        }
        for value in object.values() {
            validate_options(value)?;
        }
    } else if let Some(values) = value.as_array() {
        for value in values {
            validate_options(value)?;
        }
    }
    Ok(())
}

fn canonicalize_member_metadata(value: &mut Value) -> Result<(), String> {
    let slots = value["project"]["slots"].as_array().ok_or("缺少成员列表")?;
    let members: HashMap<String, (Option<u64>, bool)> = slots
        .iter()
        .map(|slot| {
            let support = slot["type"] == "support";
            let servant = if support {
                &value["project"]["supportServantId"]
            } else {
                &slot["servantId"]
            };
            (
                slot["id"].as_str().unwrap_or_default().to_string(),
                (servant.as_u64(), support),
            )
        })
        .collect();
    fn walk(
        value: &mut Value,
        members: &HashMap<String, (Option<u64>, bool)>,
    ) -> Result<(), String> {
        match value {
            Value::Object(object) => {
                if object.get("grandServant") == Some(&Value::Bool(true))
                    && object.get("memberId").is_some_and(|v| !v.is_null())
                {
                    return Err("冠位占位规则不能同时引用固定成员".into());
                }
                for (member_key, servant_key, support_key) in [
                    ("memberId", "servantId", "isSupport"),
                    ("servantMemberId", "servantId", "servantIsSupport"),
                    ("targetMemberId", "targetServantId", "targetIsSupport"),
                    ("frontMemberId", "frontServantId", "frontIsSupport"),
                    ("backMemberId", "backServantId", "backIsSupport"),
                ] {
                    if let Some(member) = object.get(member_key).and_then(Value::as_str) {
                        let (servant, support) = members.get(member).ok_or("成员引用不存在")?;
                        if object
                            .get(servant_key)
                            .and_then(Value::as_u64)
                            .is_some_and(|id| Some(id) != *servant)
                            || object
                                .get(support_key)
                                .and_then(Value::as_bool)
                                .is_some_and(|flag| flag != *support)
                        {
                            return Err("成员引用与从者或助战身份不一致".into());
                        }
                        // Missing descriptive metadata is derived from the
                        // authoritative member, never from a duplicate servant.
                        if object.get(servant_key).is_none_or(Value::is_null) {
                            object.insert(servant_key.into(), serde_json::json!(servant));
                        }
                        object
                            .entry(support_key)
                            .or_insert(serde_json::json!(support));
                    }
                }
                for value in object.values_mut() {
                    walk(value, members)?;
                }
            }
            Value::Array(values) => {
                for value in values {
                    walk(value, members)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    walk(value, &members)
}

fn snapshot_from_root(root: &Path, project_id: &str) -> Result<ConfigExportEntry, String> {
    let package = export_package_for_project_ids(root, &[project_id.to_string()])?;
    package
        .configs
        .into_iter()
        .next()
        .ok_or_else(|| "未找到可分享的队伍".into())
}

fn code_from_entry(entry: &ConfigExportEntry) -> Result<String, String> {
    let value = serde_json::to_value(entry).map_err(|error| error.to_string())?;
    let code = transport::encode(&value)?;
    // Export must not produce a code this version cannot read. Also check
    // referenced metadata before the user sends it to another installation.
    decode_entry(&code)?;
    Ok(code)
}

#[tauri::command]
pub(crate) async fn create_formation_share(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<FormationShareCode, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = {
            let store = app.state::<ProjectStoreLock>();
            let _guard = store.0.lock().map_err(|error| error.to_string())?;
            snapshot_from_root(&app_data_dir(&app), &project_id)?
        };
        let code = code_from_entry(&snapshot)?;
        let copied = app.clipboard().write_text(code.clone()).is_ok();
        Ok(FormationShareCode {
            characters: code.len(),
            code,
            copied,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn preview_formation_share(
    app: tauri::AppHandle,
    text: String,
) -> Result<FormationSharePreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let entry = decode_entry(&text)?;
        let target_name = {
            let store = app.state::<ProjectStoreLock>();
            let _guard = store.0.lock().map_err(|error| error.to_string())?;
            let names = read_projects(&app)?
                .into_iter()
                .map(|p| p.name)
                .collect::<Vec<_>>();
            unique_import_name(&entry.project.name, &names)
        };
        let token = uuid::Uuid::new_v4().to_string();
        let server = *app
            .state::<Mutex<Server>>()
            .lock()
            .map_err(|e| e.to_string())?;
        let mystic_code_name = entry.project.mystic_code_id.and_then(|id| {
            crate::commands::catalog::get_mystic_codes(app.clone())
                .ok()
                .and_then(|codes| {
                    codes
                        .into_iter()
                        .find(|code| code.id == id)
                        .map(|code| code.name)
                })
        });
        let preview = build_preview(&entry, token.clone(), target_name, server, mystic_code_name);
        let state = app.state::<FormationSharePreviews>();
        let mut pending = state.0.lock().map_err(|e| e.to_string())?;
        pending.retain(|_, preview| preview.expires_at > Instant::now());
        if pending.len() >= MAX_PREVIEWS {
            return Err("待导入的编队过多，请关闭其他预览后重试".into());
        }
        pending.insert(
            token,
            PendingPreview {
                expires_at: Instant::now() + PREVIEW_LIFETIME,
                entry,
            },
        );
        Ok(preview)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) fn discard_formation_share_preview(
    app: tauri::AppHandle,
    preview_token: String,
) -> Result<(), String> {
    app.state::<FormationSharePreviews>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&preview_token);
    Ok(())
}

#[tauri::command]
pub(crate) async fn import_formation_share(
    app: tauri::AppHandle,
    preview_token: String,
    name: String,
    group_id: Option<String>,
) -> Result<Project, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<FormationSharePreviews>();
        let mut pending = state.0.lock().map_err(|e| e.to_string())?;
        let store = app.state::<ProjectStoreLock>();
        let _guard = store.0.lock().map_err(|e| e.to_string())?;
        import_preview_from_root(
            &app_data_dir(&app),
            &mut pending,
            &preview_token,
            name,
            group_id.as_deref(),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

fn import_preview_from_root(
    root: &Path,
    pending: &mut HashMap<String, PendingPreview>,
    token: &str,
    name: String,
    group_id: Option<&str>,
) -> Result<Project, String> {
    let preview = pending
        .get(token)
        .ok_or("编队预览已失效，请重新粘贴分享码")?;
    if preview.expires_at <= Instant::now() {
        pending.remove(token);
        return Err("编队预览已过期，请重新预览".into());
    }
    let mut entry = preview.entry.clone();
    entry.project.name = name;
    let result = super::import_commit::commit_config_import(root, vec![entry], group_id)?;
    pending.remove(token);
    Ok(result.imported_projects.into_iter().next().unwrap())
}

#[cfg(test)]
mod tests;
