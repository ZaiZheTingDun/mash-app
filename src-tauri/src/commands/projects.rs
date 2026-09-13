//! Project CRUD and configuration import/export.
//! This module owns the persisted project JSON shape and scene-file round trips.

use super::*;
use crate::runner::grand_class_definitions;
use crate::storage::{read_json_or_default, write_json_atomic};

pub(crate) fn read_projects(app: &tauri::AppHandle) -> Result<Vec<Project>, String> {
    read_projects_from_path(&projects_file_path(app))
}

pub(crate) fn read_projects_from_path(path: &Path) -> Result<Vec<Project>, String> {
    let projects: Vec<Project> = read_json_or_default(path, "项目配置")?;
    Ok(projects.into_iter().map(normalize_project).collect())
}

pub(crate) fn write_projects(app: &tauri::AppHandle, projects: &[Project]) -> Result<(), String> {
    write_projects_to_path(&projects_file_path(app), projects)
}

pub(crate) fn write_projects_to_path(path: &Path, projects: &[Project]) -> Result<(), String> {
    write_json_atomic(path, projects, "项目配置")
}

#[tauri::command]
pub(crate) fn list_projects(app: tauri::AppHandle) -> Result<Vec<Project>, String> {
    read_projects(&app)
}

#[tauri::command]
pub(crate) fn get_grand_class_definitions() -> Vec<GrandClassDefinition> {
    grand_class_definitions()
}

#[tauri::command]
pub(crate) fn create_project(
    app: tauri::AppHandle,
    name: String,
    advanced_mode: Option<bool>,
    grand_class: Option<GrandClass>,
    group_id: Option<String>,
) -> Result<Project, String> {
    let project = new_project(
        name,
        advanced_mode.unwrap_or(false),
        grand_class.unwrap_or_default(),
    );
    let mut projects = read_projects(&app)?;
    let mut catalog = read_project_catalog(&app, &projects)?;
    if let Some(group_id) = group_id.as_deref() {
        if !catalog.groups.iter().any(|group| group.id == group_id) {
            return Err(format!("project group not found: {group_id}"));
        }
    }
    projects.push(project.clone());
    write_projects(&app, &projects)?;
    insert_project_into_catalog(&mut catalog, project.id.clone(), group_id.as_deref())?;
    write_project_catalog(&app, &catalog)?;
    Ok(project)
}

pub(crate) fn new_project(name: String, advanced_mode: bool, grand_class: GrandClass) -> Project {
    Project {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        advanced_mode,
        support_servant_id: None,
        support_servant_variant_key: None,
        support_grand_mode: advanced_mode,
        support_grand_craft_essence_ids: default_support_grand_craft_essence_ids(),
        support_grand_craft_essence_id_lists: default_support_grand_craft_essence_id_lists(),
        support_grand_craft_essence_mlb_required: default_support_grand_craft_essence_mlb_required(
        ),
        support_grand_bond_ce_mode: SupportGrandBondCeMode::Any,
        grand_class,
        grand_servants: Vec::new(),
        grand_card_strategy: GrandCardStrategy::default(),
        support_servant_level_min: None,
        support_noble_phantasm_level_min: None,
        support_star_map_score_min: None,
        support_grand_star_map_score_min: None,
        support_skill_level_mins: default_support_skill_level_mins(),
        support_append_skill_level_mins: default_support_append_skill_level_mins(),
        recognition_settings: None,
        disable_auto_skill_target_recognition: false,
        mystic_code_id: None,
        prefer_higher_critical_chance: false,
        slots: default_project_slots(),
        repeat_mission: false,
        repeat_mode: Some(ProjectRepeatMode::Single),
        repeat_count: None,
        ap_recovery_items: Vec::new(),
        ap_recovery_limits: Default::default(),
    }
}

pub(crate) fn copy_project_dir(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let entry_src = entry.path();
        let entry_dst = dst.join(entry.file_name());
        if entry_src.is_dir() {
            copy_project_dir(&entry_src, &entry_dst)?;
        } else {
            fs::copy(&entry_src, &entry_dst).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn duplicate_project(
    app: tauri::AppHandle,
    source_id: String,
    name: String,
) -> Result<Project, String> {
    let mut projects = read_projects(&app)?;
    let mut catalog = read_project_catalog(&app, &projects)?;
    let source = projects
        .iter()
        .find(|p| p.id == source_id)
        .cloned()
        .ok_or_else(|| format!("project not found: {source_id}"))?;
    let project = Project {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        ..source
    };
    projects.push(project.clone());
    write_projects(&app, &projects)?;

    let source_group_id = catalog
        .groups
        .iter()
        .find(|group| group.project_ids.contains(&source_id))
        .map(|group| group.id.clone());
    insert_project_into_catalog(&mut catalog, project.id.clone(), source_group_id.as_deref())?;
    write_project_catalog(&app, &catalog)?;

    let projects_dir = app_data_dir(&app).join("projects");
    copy_project_dir(
        &projects_dir.join(&source_id),
        &projects_dir.join(&project.id),
    )?;
    Ok(project)
}

/// Replace the stored project entry whose ``id`` matches ``project.id`` with
/// the supplied value. Used by the team-builder support slot to persist the
/// pinned servant id without a dedicated single-field setter (so future
/// project-level fields don't each need their own command).
#[tauri::command]
pub(crate) fn update_project(app: tauri::AppHandle, project: Project) -> Result<Project, String> {
    let mut projects = read_projects(&app)?;
    let Some(index) = projects.iter().position(|item| item.id == project.id) else {
        return Err(format!("project not found: {}", project.id));
    };
    projects[index] = normalize_project(project);
    write_projects(&app, &projects)?;
    Ok(projects[index].clone())
}

#[tauri::command]
pub(crate) fn delete_project(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut projects = read_projects(&app)?;
    let mut catalog = read_project_catalog(&app, &projects)?;
    projects.retain(|p| p.id != id);
    write_projects(&app, &projects)?;
    remove_project_from_catalog(&mut catalog, &id);
    write_project_catalog(&app, &catalog)?;
    let dir = app_data_dir(&app).join("projects").join(&id);
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn save_battle_scenes(
    app: tauri::AppHandle,
    project_id: String,
    scenes: Vec<BattleScene>,
) -> Result<(), String> {
    let path = project_battle_scenes_path(&app, &project_id);
    let scenes: Vec<BattleScene> = scenes
        .into_iter()
        .map(BattleScene::normalize_turns)
        .collect();
    write_json_atomic(&path, &scenes, "普通指令配置")
}

#[tauri::command]
pub(crate) fn load_battle_scenes(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<Vec<BattleScene>, String> {
    let path = project_battle_scenes_path(&app, &project_id);
    if path.exists() {
        let scenes: Vec<BattleScene> = read_json_or_default(&path, "普通指令配置")?;
        return Ok(scenes
            .into_iter()
            .map(BattleScene::normalize_turns)
            .collect());
    }

    // One-shot migration: pre-rename projects stored their per-scene
    // config under `turns.json`. The on-disk JSON shape is identical
    // (BattleScene was just renamed from Turn), so we can read it as-is,
    // write it under the new filename, and remove the legacy file.
    let legacy = legacy_project_turns_path(&app, &project_id);
    if legacy.exists() {
        let scenes: Vec<BattleScene> =
            read_json_or_default::<Vec<BattleScene>>(&legacy, "旧版普通指令配置")?
                .into_iter()
                .map(BattleScene::normalize_turns)
                .collect();
        write_json_atomic(&path, &scenes, "普通指令配置")?;
        fs::remove_file(&legacy).map_err(|error| format!("移除旧版指令配置失败：{error}"))?;
        return Ok(scenes);
    }

    Ok(Vec::new())
}

#[tauri::command]
pub(crate) fn save_advanced_battle_scenes(
    app: tauri::AppHandle,
    project_id: String,
    scenes: Vec<AdvancedBattleScene>,
) -> Result<(), String> {
    let path = project_advanced_battle_scenes_path(&app, &project_id);
    write_json_atomic(&path, &scenes, "高级指令配置")
}

#[tauri::command]
pub(crate) fn load_advanced_battle_scenes(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<Vec<AdvancedBattleScene>, String> {
    let path = project_advanced_battle_scenes_path(&app, &project_id);
    read_json_or_default(&path, "高级指令配置")
}

mod catalog;
pub(crate) use catalog::*;

mod normalization;
pub(crate) use normalization::*;

mod ui_settings;
pub(crate) use ui_settings::*;

mod config_transfer;
pub(crate) use config_transfer::*;

// ---------------------------------------------------------------------------
// Slot-servant delete
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeleteSlotServantResult {
    pub(crate) project: Project,
}

#[tauri::command]
pub(crate) fn delete_slot_servant(
    app: tauri::AppHandle,
    project_id: String,
    slot_id: String,
) -> Result<DeleteSlotServantResult, String> {
    let mut projects = read_projects(&app)?;
    let idx = projects
        .iter()
        .position(|p| p.id == project_id)
        .ok_or_else(|| format!("project not found: {project_id}"))?;
    clear_project_slot_servant(&mut projects[idx], &slot_id)?;

    let saved_project = normalize_project(projects[idx].clone());
    projects[idx] = saved_project.clone();
    write_projects(&app, &projects)?;

    Ok(DeleteSlotServantResult {
        project: saved_project,
    })
}

pub(crate) fn clear_project_slot_servant(
    project: &mut Project,
    slot_id: &str,
) -> Result<(), String> {
    let slot = project
        .slots
        .iter_mut()
        .find(|slot| slot.id == slot_id)
        .ok_or_else(|| format!("slot not found: {slot_id}"))?;
    let is_support = slot.kind == "support";
    slot.servant_id = None;
    slot.servant_variant_key = None;
    slot.craft_essence_id = None;
    slot.craft_essence_ids.clear();
    slot.craft_essence_multi_select = false;
    slot.craft_essence_mlb_required = true;

    if is_support {
        project.support_servant_id = None;
        project.support_servant_variant_key = None;
        project.support_servant_level_min = None;
        project.support_noble_phantasm_level_min = None;
        project.support_star_map_score_min = None;
        project.support_grand_star_map_score_min = None;
        project.support_skill_level_mins = default_support_skill_level_mins();
        project.support_append_skill_level_mins = default_support_append_skill_level_mins();
        project.support_grand_mode = false;
        project.support_grand_craft_essence_ids = default_support_grand_craft_essence_ids();
        project.support_grand_craft_essence_id_lists =
            default_support_grand_craft_essence_id_lists();
        project.support_grand_craft_essence_mlb_required =
            default_support_grand_craft_essence_mlb_required();
        project.support_grand_bond_ce_mode = SupportGrandBondCeMode::Any;
    }
    Ok(())
}
