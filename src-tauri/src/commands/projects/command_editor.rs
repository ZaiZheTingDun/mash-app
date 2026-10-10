//! Command editor mutations stay backend-owned.
use super::*;
use serde_json::{json, Value};
use std::sync::Mutex;

#[derive(Default)]
pub(crate) struct CommandEditorLock(Mutex<()>);

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommandEditorState {
    scenes: Vec<Value>,
    wave: usize,
    turn: usize,
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum CommandEditorMutation {
    AddWave,
    DeleteWave,
    AddTurn,
    DeleteTurn,
    UpdateTurn { turn: Value },
    UpdateScene { scene: Value },
}

fn default_turn(advanced: bool) -> Value {
    let id = uuid::Uuid::new_v4().to_string();
    if advanced {
        json!({ "id": id, "actions": [] })
    } else {
        json!({ "id": id, "preparationActions": [], "enemyTarget": null,
            "attackPriority": (0..3).map(|_| json!({"id":uuid::Uuid::new_v4().to_string(),"card":null})).collect::<Vec<_>>(),
            "attackMode":"normal", "criticalStrategy":{"memberPriority":[],"chainPriority":["mighty","buster","arts","quick"]},
            "advancedCardStrategy":{"customRules":[]} })
    }
}

fn default_scene(advanced: bool) -> Value {
    json!({ "id": uuid::Uuid::new_v4().to_string(), "turns": [default_turn(advanced)] })
}

fn normalize_documents(scenes: Vec<Value>, advanced: bool) -> Result<Vec<Value>, String> {
    let scenes = if scenes.is_empty() {
        vec![default_scene(advanced)]
    } else {
        scenes
    };
    scenes
        .into_iter()
        .map(|scene| {
            if advanced {
                let mut scene: AdvancedBattleScene =
                    serde_json::from_value(scene).map_err(|e| e.to_string())?;
                if scene.turns.is_empty() {
                    scene.turns.push(AdvancedBattleTurn {
                        id: uuid::Uuid::new_v4().to_string(),
                        actions: std::mem::take(&mut scene.startup_actions),
                    });
                }
                scene.startup_actions.clear();
                for slot in 0..5 {
                    if !scene
                        .command_conditions
                        .iter()
                        .any(|card| card.slot == slot)
                    {
                        scene.command_conditions.push(
                            serde_json::from_value(
                                json!({"slot":slot,"servant":"any","suit":"any"}),
                            )
                            .map_err(|e| e.to_string())?,
                        );
                    }
                }
                scene.command_conditions.sort_by_key(|card| card.slot);
                serde_json::to_value(scene).map_err(|e| e.to_string())
            } else {
                let scene: BattleScene =
                    serde_json::from_value(scene).map_err(|e| e.to_string())?;
                let mut scene = scene.normalize_turns();
                for turn in &mut scene.turns {
                    while turn.attack_priority.len() < 3 {
                        turn.attack_priority.push(AttackCard {
                            id: uuid::Uuid::new_v4().to_string(),
                            card: None,
                            member_id: None,
                            servant_id: None,
                            is_support: false,
                        });
                    }
                }
                serde_json::to_value(scene).map_err(|e| e.to_string())
            }
        })
        .collect()
}

fn read_document(
    app: &tauri::AppHandle,
    project_id: &str,
    advanced: bool,
) -> Result<Vec<Value>, String> {
    let values = if advanced {
        load_advanced_battle_scenes(app.clone(), project_id.to_string())?
            .into_iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
    } else {
        load_battle_scenes(app.clone(), project_id.to_string())?
            .into_iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
    }
    .map_err(|e| e.to_string())?;
    normalize_documents(values, advanced)
}

fn write_document(
    app: &tauri::AppHandle,
    project_id: &str,
    advanced: bool,
    scenes: &[Value],
) -> Result<(), String> {
    let path = if advanced {
        project_advanced_battle_scenes_path(app, project_id)
    } else {
        project_battle_scenes_path(app, project_id)
    };
    write_json_atomic(&path, scenes, "指令配置")
}

fn mutate_document(
    mut scenes: Vec<Value>,
    advanced: bool,
    mutation: CommandEditorMutation,
    wave: usize,
    turn: usize,
) -> Result<CommandEditorState, String> {
    if wave >= scenes.len() {
        return Err("当前面已不存在，请重新加载".into());
    }
    let turns = scenes[wave]["turns"].as_array().ok_or("回合配置无效")?;
    if turn >= turns.len() {
        return Err("当前回合已不存在，请重新加载".into());
    }
    let mut next_wave = wave;
    let mut next_turn = turn;
    match mutation {
        CommandEditorMutation::AddWave => {
            if advanced {
                return Err("冠位任务仅配置一面战斗".into());
            }
            scenes.push(default_scene(advanced));
            next_wave = scenes.len() - 1;
            next_turn = 0;
        }
        CommandEditorMutation::DeleteWave => {
            if advanced || scenes.len() <= 1 {
                return Err("至少保留一面战斗".into());
            }
            scenes.remove(wave);
            next_wave = wave.min(scenes.len() - 1);
            next_turn = 0;
        }
        CommandEditorMutation::AddTurn => {
            let turns = scenes[wave]["turns"].as_array_mut().ok_or("回合配置无效")?;
            turns.push(default_turn(advanced));
            next_turn = turns.len() - 1;
        }
        CommandEditorMutation::DeleteTurn => {
            let turns = scenes[wave]["turns"].as_array_mut().ok_or("回合配置无效")?;
            if turns.len() <= 1 {
                return Err("至少保留一个回合".into());
            }
            turns.remove(turn);
            next_turn = turn.min(turns.len() - 1);
        }
        CommandEditorMutation::UpdateTurn { turn: updated } => {
            if updated["id"] != scenes[wave]["turns"][turn]["id"] {
                return Err("回合身份不匹配".into());
            }
            scenes[wave]["turns"][turn] = updated;
        }
        CommandEditorMutation::UpdateScene { scene } => {
            if scene["id"] != scenes[wave]["id"] {
                return Err("场景身份不匹配".into());
            }
            scenes[wave] = scene;
        }
    }
    Ok(CommandEditorState {
        scenes: normalize_documents(scenes, advanced)?,
        wave: next_wave,
        turn: next_turn,
    })
}

#[tauri::command]
pub(crate) fn load_command_editor(
    app: tauri::AppHandle,
    editor_lock: tauri::State<'_, CommandEditorLock>,
    project_id: String,
    advanced: bool,
) -> Result<CommandEditorState, String> {
    let _guard = editor_lock.0.lock().map_err(|e| e.to_string())?;
    let scenes = read_document(&app, &project_id, advanced)?;
    // Persist backend defaults once so subsequent mutations use the same identities.
    write_document(&app, &project_id, advanced, &scenes)?;
    Ok(CommandEditorState {
        scenes,
        wave: 0,
        turn: 0,
    })
}

#[tauri::command]
pub(crate) fn mutate_command_editor(
    app: tauri::AppHandle,
    editor_lock: tauri::State<'_, CommandEditorLock>,
    project_id: String,
    advanced: bool,
    mutation: CommandEditorMutation,
    wave: usize,
    turn: usize,
) -> Result<CommandEditorState, String> {
    let _guard = editor_lock.0.lock().map_err(|e| e.to_string())?;
    let scenes = read_document(&app, &project_id, advanced)?;
    let next = mutate_document(scenes, advanced, mutation, wave, turn)?;
    write_document(&app, &project_id, advanced, &next.scenes)?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_legacy_turns_are_normalized() {
        let normal = normalize_documents(vec![], false).unwrap();
        assert_eq!(
            normal[0]["turns"][0]["attackPriority"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let advanced = normalize_documents(vec![json!({"id":"a", "startupActions":[{"type":"enemyTarget","id":"t","target":"enemy_1"}]})], true).unwrap();
        assert_eq!(
            advanced[0]["turns"][0]["actions"].as_array().unwrap().len(),
            1
        );
        assert!(advanced[0]["startupActions"].as_array().unwrap().is_empty());
    }
    #[test]
    fn deletes_first_or_last_turn_and_preserves_other_configuration() {
        let first = normalize_documents(vec![], false).unwrap();
        let first_id = first[0]["turns"][0]["id"].clone();
        let added = mutate_document(first, false, CommandEditorMutation::AddTurn, 0, 0).unwrap();
        assert_eq!(added.turn, 1);
        let second_id = added.scenes[0]["turns"][1]["id"].clone();
        let deleted = mutate_document(
            added.scenes.clone(),
            false,
            CommandEditorMutation::DeleteTurn,
            0,
            0,
        )
        .unwrap();
        assert_eq!(deleted.turn, 0);
        assert_eq!(deleted.scenes[0]["turns"][0]["id"], second_id);
        assert!(mutate_document(
            deleted.scenes,
            false,
            CommandEditorMutation::DeleteTurn,
            0,
            0
        )
        .is_err());
        let last =
            mutate_document(added.scenes, false, CommandEditorMutation::DeleteTurn, 0, 1).unwrap();
        assert_eq!(last.scenes[0]["turns"][0]["id"], first_id);
    }
    #[test]
    fn editor_state_has_no_undo_and_rejects_undo_mutations() {
        for advanced in [false, true] {
            let scenes = normalize_documents(vec![], advanced).unwrap();
            let added =
                mutate_document(scenes, advanced, CommandEditorMutation::AddTurn, 0, 0).unwrap();
            let state = serde_json::to_value(added).unwrap();
            assert!(state.get("canUndo").is_none());
            assert_eq!(state["turn"], 1);
            assert_eq!(state["scenes"][0]["turns"].as_array().unwrap().len(), 2);
        }
        assert!(serde_json::from_value::<CommandEditorMutation>(json!({"type": "undo"})).is_err());
    }
    #[test]
    fn wave_boundaries_and_scene_identity_are_checked() {
        let scenes = normalize_documents(vec![], false).unwrap();
        assert!(mutate_document(
            scenes.clone(),
            false,
            CommandEditorMutation::DeleteWave,
            0,
            0
        )
        .is_err());
        assert!(mutate_document(
            scenes.clone(),
            false,
            CommandEditorMutation::UpdateScene {
                scene: json!({"id":"wrong"})
            },
            0,
            0
        )
        .is_err());
        let added = mutate_document(scenes, false, CommandEditorMutation::AddWave, 0, 0).unwrap();
        assert_eq!(added.wave, 1);
        let removed =
            mutate_document(added.scenes, false, CommandEditorMutation::DeleteWave, 1, 0).unwrap();
        assert_eq!(removed.wave, 0);
        assert!(mutate_document(
            normalize_documents(vec![], true).unwrap(),
            true,
            CommandEditorMutation::AddWave,
            0,
            0
        )
        .is_err());
    }
}
