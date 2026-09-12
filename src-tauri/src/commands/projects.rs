//! Project CRUD and configuration import/export.
//! This module owns the persisted project JSON shape and scene-file round trips.

use super::*;
use crate::runner::{grand_class_definitions, grand_strategy};
use crate::storage::{read_json_or_default, write_json_atomic};
use std::collections::HashSet;

pub(crate) fn read_projects(app: &tauri::AppHandle) -> Result<Vec<Project>, String> {
    read_projects_from_path(&projects_file_path(app))
}

pub(crate) fn read_projects_from_path(path: &Path) -> Result<Vec<Project>, String> {
    let projects: Vec<Project> = read_json_or_default(path, "项目配置")?;
    Ok(projects.into_iter().map(normalize_project).collect())
}

pub(crate) fn normalize_project_catalog(
    mut catalog: ProjectCatalog,
    projects: &[Project],
) -> ProjectCatalog {
    catalog.schema_version = 1;
    let valid_project_ids: HashSet<&str> =
        projects.iter().map(|project| project.id.as_str()).collect();
    let mut seen_project_ids = HashSet::new();
    let mut seen_group_ids = HashSet::new();

    catalog.groups.retain_mut(|group| {
        if group.id.is_empty() || !seen_group_ids.insert(group.id.clone()) {
            return false;
        }
        group.name = group.name.trim().to_string();
        if group.name.is_empty() {
            return false;
        }
        group.project_ids.retain(|project_id| {
            valid_project_ids.contains(project_id.as_str())
                && seen_project_ids.insert(project_id.clone())
        });
        true
    });
    catalog.ungrouped_project_ids.retain(|project_id| {
        valid_project_ids.contains(project_id.as_str())
            && seen_project_ids.insert(project_id.clone())
    });
    for project in projects {
        if seen_project_ids.insert(project.id.clone()) {
            catalog.ungrouped_project_ids.push(project.id.clone());
        }
    }
    catalog
}

pub(crate) fn read_project_catalog_from_path(
    path: &Path,
    projects: &[Project],
) -> Result<ProjectCatalog, String> {
    let catalog = read_json_or_default(path, "项目目录文件")?;
    Ok(normalize_project_catalog(catalog, projects))
}

pub(crate) fn write_project_catalog_to_path(
    path: &Path,
    catalog: &ProjectCatalog,
) -> Result<(), String> {
    write_json_atomic(path, catalog, "项目目录")
}

fn read_project_catalog(
    app: &tauri::AppHandle,
    projects: &[Project],
) -> Result<ProjectCatalog, String> {
    read_project_catalog_from_path(&project_catalog_path(app), projects)
}

fn write_project_catalog(app: &tauri::AppHandle, catalog: &ProjectCatalog) -> Result<(), String> {
    write_project_catalog_to_path(&project_catalog_path(app), catalog)
}

fn validate_project_group_name(
    catalog: &ProjectCatalog,
    name: &str,
    except_id: Option<&str>,
) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("分组名称不能为空".to_string());
    }
    if catalog
        .groups
        .iter()
        .any(|group| group.id != except_id.unwrap_or_default() && group.name == name)
    {
        return Err(format!("分组「{name}」已存在"));
    }
    Ok(name)
}

fn remove_project_from_catalog(catalog: &mut ProjectCatalog, project_id: &str) {
    catalog
        .ungrouped_project_ids
        .retain(|existing_id| existing_id != project_id);
    for group in &mut catalog.groups {
        group
            .project_ids
            .retain(|existing_id| existing_id != project_id);
    }
}

pub(crate) fn insert_project_into_catalog(
    catalog: &mut ProjectCatalog,
    project_id: String,
    group_id: Option<&str>,
) -> Result<(), String> {
    remove_project_from_catalog(catalog, &project_id);
    if let Some(group_id) = group_id {
        let group = catalog
            .groups
            .iter_mut()
            .find(|group| group.id == group_id)
            .ok_or_else(|| format!("project group not found: {group_id}"))?;
        group.project_ids.push(project_id);
    } else {
        catalog.ungrouped_project_ids.push(project_id);
    }
    Ok(())
}

pub(crate) fn reorder_catalog_ids(
    current_ids: &mut Vec<String>,
    ordered_ids: Vec<String>,
    label: &str,
) -> Result<(), String> {
    let current: HashSet<&str> = current_ids.iter().map(String::as_str).collect();
    let ordered: HashSet<&str> = ordered_ids.iter().map(String::as_str).collect();
    if current_ids.len() != ordered_ids.len()
        || ordered.len() != ordered_ids.len()
        || current != ordered
    {
        return Err(format!("{label}排序内容与当前目录不一致"));
    }
    *current_ids = ordered_ids;
    Ok(())
}

pub(crate) fn normalize_project(mut project: Project) -> Project {
    for slot in &mut project.slots {
        let mut ids = Vec::new();
        for id in slot
            .craft_essence_ids
            .iter()
            .copied()
            .chain(slot.craft_essence_id)
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
            if ids.len() == 10 {
                break;
            }
        }
        if slot.kind != "support" || !slot.craft_essence_multi_select {
            ids.truncate(1);
            slot.craft_essence_multi_select = false;
        }
        slot.craft_essence_id = ids.first().copied();
        slot.craft_essence_ids = ids;
    }
    for index in 0..3 {
        let mut ids = Vec::new();
        for id in project.support_grand_craft_essence_id_lists[index]
            .iter()
            .copied()
            .chain(project.support_grand_craft_essence_ids[index])
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
            if ids.len() == if index == 1 { 1 } else { 10 } {
                break;
            }
        }
        if index == 1 {
            ids.truncate(1);
        }
        project.support_grand_craft_essence_ids[index] = ids.first().copied();
        project.support_grand_craft_essence_id_lists[index] = ids;
    }
    let repeat_mode = match project.repeat_mode {
        Some(mode) => mode,
        None if project.repeat_mission => ProjectRepeatMode::Infinite,
        None => ProjectRepeatMode::Single,
    };
    project.repeat_mission = !matches!(repeat_mode, ProjectRepeatMode::Single);
    project.repeat_mode = Some(repeat_mode);
    if !matches!(project.repeat_mode, Some(ProjectRepeatMode::Count)) {
        project.repeat_count = None;
    }
    project.support_star_map_score_min = project
        .support_star_map_score_min
        .map(|score| score.min(62));
    project.support_grand_star_map_score_min = project
        .support_grand_star_map_score_min
        .map(|score| score.min(16));
    project.support_servant_level_min = project
        .support_servant_level_min
        .map(|level| level.clamp(1, 120));
    normalize_grand_card_rule_slots(&mut project);
    normalize_grand_servants(&mut project);
    normalize_project_recognition_settings(&mut project);
    project
}

fn normalize_project_recognition_settings(project: &mut Project) {
    if let Some(settings) = &mut project.recognition_settings {
        if settings.stop_on_five_star_ce_drop == Some(true)
            && settings
                .five_star_ce_drop_target_count
                .is_none_or(|count| count == 0)
        {
            settings.five_star_ce_drop_target_count = Some(1);
        }
        if settings.stop_on_five_star_ce_drop != Some(true)
            && settings.five_star_ce_drop_target_count == Some(1)
        {
            settings.five_star_ce_drop_target_count = None;
        }
    }
}

fn slot_member_metadata(
    slot: &ProjectSlot,
    support_servant_id: Option<u32>,
) -> (String, Option<u32>, bool) {
    let is_support = slot.kind == "support";
    let servant_id = if is_support {
        support_servant_id
    } else {
        slot.servant_id
    };
    (slot.id.clone(), servant_id, is_support)
}

fn normalize_grand_card_rule_slots(project: &mut Project) {
    let slots = project.slots.clone();
    let support_servant_id = project.support_servant_id;
    for rule in &mut project.grand_card_strategy.custom_rules {
        for slot in &mut rule.slots {
            if slot.grand_servant {
                slot.member_id = None;
                slot.slot_index = None;
                continue;
            }
            let valid_slot = slot
                .member_id
                .as_deref()
                .and_then(|member_id| {
                    slots
                        .iter()
                        .enumerate()
                        .find(|(_, project_slot)| project_slot.id == member_id)
                        .map(|(index, project_slot)| (index as u32, project_slot))
                })
                .or_else(|| {
                    slot.slot_index.and_then(|index| {
                        slots
                            .get(index as usize)
                            .map(|project_slot| (index, project_slot))
                    })
                })
                .and_then(|(index, project_slot)| {
                    let (_member_id, actual_id, is_support) =
                        slot_member_metadata(project_slot, support_servant_id);
                    (is_support == slot.is_support
                        && actual_id.is_some()
                        && actual_id == slot.servant_id)
                        .then_some(index)
                });
            let resolved_slot = valid_slot.or_else(|| {
                slot.servant_id.and_then(|servant_id| {
                    slots.iter().enumerate().find_map(|(index, project_slot)| {
                        let (_member_id, actual_id, is_support) =
                            slot_member_metadata(project_slot, support_servant_id);
                        (is_support == slot.is_support && actual_id == Some(servant_id))
                            .then_some(index as u32)
                    })
                })
            });
            slot.slot_index = resolved_slot;
            if let Some(index) = resolved_slot.and_then(|index| slots.get(index as usize)) {
                let (member_id, servant_id, is_support) =
                    slot_member_metadata(index, support_servant_id);
                slot.member_id = Some(member_id);
                slot.servant_id = servant_id;
                slot.is_support = is_support;
            }
        }
    }
}

fn normalize_grand_servants(project: &mut Project) {
    let slots = project.slots.clone();
    let support_servant_id = project.support_servant_id;
    for config in &mut project.grand_servants {
        let resolved_slot = config
            .member_id
            .as_deref()
            .and_then(|member_id| {
                slots
                    .iter()
                    .enumerate()
                    .find(|(_, project_slot)| project_slot.id == member_id)
                    .map(|(index, _)| index as u32)
            })
            .or_else(|| {
                slots
                    .get(config.slot_index as usize)
                    .map(|_| config.slot_index)
            })
            .or_else(|| {
                config.servant_id.and_then(|servant_id| {
                    slots.iter().enumerate().find_map(|(index, project_slot)| {
                        let (_member_id, actual_id, is_support) =
                            slot_member_metadata(project_slot, support_servant_id);
                        (is_support == config.is_support && actual_id == Some(servant_id))
                            .then_some(index as u32)
                    })
                })
            });
        if let Some(index) = resolved_slot {
            config.slot_index = index;
            if let Some(slot) = slots.get(index as usize) {
                let (member_id, servant_id, is_support) =
                    slot_member_metadata(slot, support_servant_id);
                config.member_id = Some(member_id);
                config.servant_id = servant_id;
                config.is_support = is_support;
            }
        }
    }
    grand_strategy(project.grand_class).normalize_servants(&mut project.grand_servants);
}

pub(crate) fn write_projects(app: &tauri::AppHandle, projects: &[Project]) -> Result<(), String> {
    write_projects_to_path(&projects_file_path(app), projects)
}

pub(crate) fn write_projects_to_path(path: &Path, projects: &[Project]) -> Result<(), String> {
    write_json_atomic(path, projects, "项目配置")
}

pub(crate) fn read_app_ui_settings_from_path(path: &Path) -> Result<AppUiSettings, String> {
    read_json_or_default(path, "界面设置")
}

pub(crate) fn write_app_ui_settings_to_path(
    path: &Path,
    settings: &AppUiSettings,
) -> Result<(), String> {
    write_json_atomic(path, settings, "界面设置")
}

pub(crate) fn update_app_ui_settings(
    app: &tauri::AppHandle,
    state: &Mutex<AppUiSettings>,
    update: impl FnOnce(&mut AppUiSettings),
) -> Result<AppUiSettings, String> {
    let mut guard = state.lock().unwrap();
    let mut next = guard.clone();
    update(&mut next);
    write_app_ui_settings_to_path(&app_ui_settings_path(app), &next)?;
    *guard = next.clone();
    Ok(next)
}

#[tauri::command]
pub(crate) fn get_active_project_id(
    state: tauri::State<'_, Mutex<AppUiSettings>>,
) -> Option<String> {
    state.lock().unwrap().active_project_id.clone()
}

#[tauri::command]
pub(crate) fn set_active_project_id(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppUiSettings>>,
    active_project_id: Option<String>,
) -> Result<(), String> {
    update_app_ui_settings(&app, state.inner(), |settings| {
        settings.active_project_id = active_project_id;
    })?;
    Ok(())
}

#[tauri::command]
pub(crate) fn get_app_theme(state: tauri::State<'_, Mutex<AppUiSettings>>) -> Option<String> {
    state
        .lock()
        .unwrap()
        .theme
        .clone()
        .filter(|theme| theme == "light" || theme == "dark" || theme == "system")
}

#[tauri::command]
pub(crate) fn set_app_theme(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppUiSettings>>,
    theme: String,
) -> Result<(), String> {
    if theme != "light" && theme != "dark" && theme != "system" {
        return Err(format!("invalid app theme: {theme}"));
    }
    update_app_ui_settings(&app, state.inner(), |settings| {
        settings.theme = Some(theme);
    })?;
    Ok(())
}

#[tauri::command]
pub(crate) fn get_battle_start_panel(
    state: tauri::State<'_, Mutex<AppUiSettings>>,
) -> crate::paths::BattleStartPanel {
    state.lock().unwrap().battle_start_panel
}

#[tauri::command]
pub(crate) fn set_battle_start_panel(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppUiSettings>>,
    value: crate::paths::BattleStartPanel,
) -> Result<crate::paths::BattleStartPanel, String> {
    update_app_ui_settings(&app, state.inner(), |settings| {
        settings.battle_start_panel = value;
    })?;
    Ok(value)
}

#[tauri::command]
pub(crate) fn get_mystic_code_gender(
    state: tauri::State<'_, Mutex<AppUiSettings>>,
) -> crate::paths::MysticCodeGender {
    state.lock().unwrap().mystic_code_gender
}

#[tauri::command]
pub(crate) fn set_mystic_code_gender(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppUiSettings>>,
    value: crate::paths::MysticCodeGender,
) -> Result<crate::paths::MysticCodeGender, String> {
    update_app_ui_settings(&app, state.inner(), |settings| {
        settings.mystic_code_gender = value;
    })?;
    Ok(value)
}

#[tauri::command]
pub(crate) fn list_projects(app: tauri::AppHandle) -> Result<Vec<Project>, String> {
    read_projects(&app)
}

#[tauri::command]
pub(crate) fn get_project_catalog(app: tauri::AppHandle) -> Result<ProjectCatalog, String> {
    let projects = read_projects(&app)?;
    read_project_catalog(&app, &projects)
}

#[tauri::command]
pub(crate) fn create_project_group(
    app: tauri::AppHandle,
    name: String,
) -> Result<ProjectCatalog, String> {
    let projects = read_projects(&app)?;
    let mut catalog = read_project_catalog(&app, &projects)?;
    let name = validate_project_group_name(&catalog, &name, None)?;
    catalog.groups.push(ProjectGroup {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        project_ids: Vec::new(),
    });
    write_project_catalog(&app, &catalog)?;
    Ok(catalog)
}

#[tauri::command]
pub(crate) fn rename_project_group(
    app: tauri::AppHandle,
    group_id: String,
    name: String,
) -> Result<ProjectCatalog, String> {
    let projects = read_projects(&app)?;
    let mut catalog = read_project_catalog(&app, &projects)?;
    let name = validate_project_group_name(&catalog, &name, Some(&group_id))?;
    let group = catalog
        .groups
        .iter_mut()
        .find(|group| group.id == group_id)
        .ok_or_else(|| format!("project group not found: {group_id}"))?;
    group.name = name;
    write_project_catalog(&app, &catalog)?;
    Ok(catalog)
}

#[tauri::command]
pub(crate) fn delete_project_group(
    app: tauri::AppHandle,
    group_id: String,
) -> Result<ProjectCatalog, String> {
    let projects = read_projects(&app)?;
    let mut catalog = read_project_catalog(&app, &projects)?;
    let index = catalog
        .groups
        .iter()
        .position(|group| group.id == group_id)
        .ok_or_else(|| format!("project group not found: {group_id}"))?;
    let group = catalog.groups.remove(index);
    catalog.ungrouped_project_ids.extend(group.project_ids);
    write_project_catalog(&app, &catalog)?;
    Ok(catalog)
}

#[tauri::command]
pub(crate) fn move_project_to_group(
    app: tauri::AppHandle,
    project_id: String,
    group_id: Option<String>,
) -> Result<ProjectCatalog, String> {
    let projects = read_projects(&app)?;
    if !projects.iter().any(|project| project.id == project_id) {
        return Err(format!("project not found: {project_id}"));
    }
    let mut catalog = read_project_catalog(&app, &projects)?;
    insert_project_into_catalog(&mut catalog, project_id, group_id.as_deref())?;
    write_project_catalog(&app, &catalog)?;
    Ok(catalog)
}

#[tauri::command]
pub(crate) fn reorder_project_groups(
    app: tauri::AppHandle,
    group_ids: Vec<String>,
) -> Result<ProjectCatalog, String> {
    let projects = read_projects(&app)?;
    let mut catalog = read_project_catalog(&app, &projects)?;
    let mut current_group_ids: Vec<String> = catalog
        .groups
        .iter()
        .map(|group| group.id.clone())
        .collect();
    reorder_catalog_ids(&mut current_group_ids, group_ids, "分组")?;
    let positions: std::collections::HashMap<String, usize> = current_group_ids
        .into_iter()
        .enumerate()
        .map(|(index, id)| (id, index))
        .collect();
    catalog
        .groups
        .sort_by_key(|group| positions.get(&group.id).copied().unwrap_or(usize::MAX));
    write_project_catalog(&app, &catalog)?;
    Ok(catalog)
}

#[tauri::command]
pub(crate) fn reorder_projects_in_group(
    app: tauri::AppHandle,
    group_id: Option<String>,
    project_ids: Vec<String>,
) -> Result<ProjectCatalog, String> {
    let projects = read_projects(&app)?;
    let mut catalog = read_project_catalog(&app, &projects)?;
    let current_ids = if let Some(group_id) = group_id.as_deref() {
        &mut catalog
            .groups
            .iter_mut()
            .find(|group| group.id == group_id)
            .ok_or_else(|| format!("project group not found: {group_id}"))?
            .project_ids
    } else {
        &mut catalog.ungrouped_project_ids
    };
    reorder_catalog_ids(current_ids, project_ids, "队伍")?;
    write_project_catalog(&app, &catalog)?;
    Ok(catalog)
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

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportableConfigSummary {
    id: String,
    name: String,
    advanced_mode: bool,
    battle_scene_count: usize,
    advanced_battle_scene_count: usize,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportConfigsResult {
    pub(crate) file_path: String,
    pub(crate) exported_count: usize,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigImportPreviewItem {
    pub(crate) import_key: String,
    pub(crate) source_name: String,
    pub(crate) target_name: String,
    pub(crate) advanced_mode: bool,
    pub(crate) battle_scene_count: usize,
    pub(crate) advanced_battle_scene_count: usize,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigImportInvalidItem {
    pub(crate) label: String,
    pub(crate) reason: String,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigImportPreview {
    pub(crate) file_name: String,
    pub(crate) valid_configs: Vec<ConfigImportPreviewItem>,
    pub(crate) invalid_items: Vec<ConfigImportInvalidItem>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigImportResult {
    pub(crate) imported_projects: Vec<Project>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigExportPackage {
    pub(crate) schema_version: u32,
    pub(crate) exported_at: String,
    #[serde(default)]
    pub(crate) app_version: Option<String>,
    pub(crate) configs: Vec<ConfigExportEntry>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigExportEntry {
    pub(crate) project: Project,
    #[serde(default)]
    pub(crate) battle_scenes: Vec<BattleScene>,
    #[serde(default)]
    pub(crate) advanced_battle_scenes: Vec<AdvancedBattleScene>,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedConfigEntry {
    pub(crate) source_index: usize,
    pub(crate) source_name: String,
    pub(crate) target_name: String,
    pub(crate) entry: ConfigExportEntry,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedConfigImport {
    valid: Vec<ParsedConfigEntry>,
    invalid: Vec<ConfigImportInvalidItem>,
}

pub(crate) fn load_battle_scenes_from_root(
    root: &Path,
    project_id: &str,
) -> Result<Vec<BattleScene>, String> {
    let scenes: Vec<BattleScene> = read_json_or_default(
        &project_battle_scenes_path_in_root(root, project_id),
        "普通指令配置",
    )?;
    Ok(scenes
        .into_iter()
        .map(BattleScene::normalize_turns)
        .collect())
}

pub(crate) fn load_advanced_battle_scenes_from_root(
    root: &Path,
    project_id: &str,
) -> Result<Vec<AdvancedBattleScene>, String> {
    read_json_or_default(
        &project_advanced_battle_scenes_path_in_root(root, project_id),
        "高级指令配置",
    )
}

pub(crate) fn write_battle_scenes_to_root(
    root: &Path,
    project_id: &str,
    scenes: &[BattleScene],
) -> Result<(), String> {
    let path = project_battle_scenes_path_in_root(root, project_id);
    let scenes: Vec<BattleScene> = scenes
        .iter()
        .cloned()
        .map(BattleScene::normalize_turns)
        .collect();
    write_json_atomic(&path, &scenes, "普通指令配置")
}

pub(crate) fn write_advanced_battle_scenes_to_root(
    root: &Path,
    project_id: &str,
    scenes: &[AdvancedBattleScene],
) -> Result<(), String> {
    let path = project_advanced_battle_scenes_path_in_root(root, project_id);
    write_json_atomic(&path, scenes, "高级指令配置")
}

pub(crate) fn exportable_config_summaries_from_root(
    root: &Path,
) -> Result<Vec<ExportableConfigSummary>, String> {
    let projects = read_projects_from_path(&root.join("projects.json"))?;
    let mut summaries = Vec::with_capacity(projects.len());
    for project in projects {
        summaries.push(ExportableConfigSummary {
            battle_scene_count: load_battle_scenes_from_root(root, &project.id)?.len(),
            advanced_battle_scene_count: load_advanced_battle_scenes_from_root(root, &project.id)?
                .len(),
            id: project.id,
            name: project.name,
            advanced_mode: project.advanced_mode,
        });
    }
    Ok(summaries)
}

pub(crate) fn current_export_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format_unix_timestamp_utc(seconds)
}

pub(crate) fn format_unix_timestamp_utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

pub(crate) fn civil_from_days(days_since_unix_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month as u32, day as u32)
}

pub(crate) fn export_package_for_project_ids(
    root: &Path,
    project_ids: &[String],
) -> Result<ConfigExportPackage, String> {
    if project_ids.is_empty() {
        return Err("请选择需要导出的配置".to_string());
    }
    let projects = read_projects_from_path(&root.join("projects.json"))?;
    let selected: std::collections::HashSet<&str> =
        project_ids.iter().map(String::as_str).collect();
    let mut configs = Vec::new();
    for project_id in project_ids {
        let project = projects
            .iter()
            .find(|project| project.id == *project_id)
            .cloned()
            .ok_or_else(|| format!("未找到配置: {project_id}"))?;
        configs.push(ConfigExportEntry {
            battle_scenes: load_battle_scenes_from_root(root, &project.id)?,
            advanced_battle_scenes: load_advanced_battle_scenes_from_root(root, &project.id)?,
            project,
        });
    }
    if configs.len() != selected.len() {
        return Err("导出配置列表包含重复项".to_string());
    }
    Ok(ConfigExportPackage {
        schema_version: 1,
        exported_at: current_export_timestamp(),
        app_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        configs,
    })
}

pub(crate) fn write_config_package_zip(
    package: &ConfigExportPackage,
    target: &Path,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建导出目录失败: {e}"))?;
    }
    let file = fs::File::create(target).map_err(|e| format!("创建导出文件失败: {e}"))?;
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file(
            "mash-config-export.json",
            zip::write::SimpleFileOptions::default(),
        )
        .map_err(|e| format!("写入导出包失败: {e}"))?;
    let json = serde_json::to_vec_pretty(package).map_err(|e| e.to_string())?;
    writer
        .write_all(&json)
        .map_err(|e| format!("写入导出包失败: {e}"))?;
    writer
        .finish()
        .map_err(|e| format!("完成导出包失败: {e}"))?;
    Ok(())
}

pub(crate) fn read_config_package_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension == "zip" {
        let file = fs::File::open(path).map_err(|e| format!("无法打开配置包: {e}"))?;
        let mut archive = ZipArchive::new(file).map_err(|e| format!("无法读取配置包 zip: {e}"))?;
        let mut entry = archive
            .by_name("mash-config-export.json")
            .map_err(|_| "配置包缺少 mash-config-export.json".to_string())?;
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| format!("读取配置包失败: {e}"))?;
        Ok(bytes)
    } else {
        fs::read(path).map_err(|e| format!("无法读取配置文件: {e}"))
    }
}

pub(crate) fn parse_config_import_from_bytes(
    bytes: &[u8],
    existing_names: &[String],
) -> Result<ParsedConfigImport, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("配置文件不是有效 JSON: {e}"))?;
    let schema_version = value
        .get("schemaVersion")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| "配置文件缺少 schemaVersion".to_string())?;
    if schema_version != 1 {
        return Err(format!("不支持的配置文件版本: {schema_version}"));
    }
    let configs = value
        .get("configs")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "配置文件缺少 configs".to_string())?;

    let mut used_names = existing_names.to_vec();
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    for (index, config) in configs.iter().enumerate() {
        let label = config
            .get("project")
            .and_then(|project| project.get("name"))
            .and_then(|name| name.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("第 {} 项", index + 1));
        match serde_json::from_value::<ConfigExportEntry>(config.clone()) {
            Ok(mut entry) => {
                entry.project = normalize_project(entry.project);
                let source_name = entry.project.name.trim().to_string();
                if source_name.is_empty() {
                    invalid.push(ConfigImportInvalidItem {
                        label,
                        reason: "配置名称为空".to_string(),
                    });
                    continue;
                }
                entry.battle_scenes = entry
                    .battle_scenes
                    .into_iter()
                    .map(BattleScene::normalize_turns)
                    .collect();
                let target_name = unique_import_name(&source_name, &used_names);
                used_names.push(target_name.clone());
                valid.push(ParsedConfigEntry {
                    source_index: index,
                    source_name,
                    target_name,
                    entry,
                });
            }
            Err(err) => invalid.push(ConfigImportInvalidItem {
                label,
                reason: format!("配置结构无法识别: {err}"),
            }),
        }
    }
    Ok(ParsedConfigImport { valid, invalid })
}

pub(crate) fn unique_import_name(source_name: &str, existing_names: &[String]) -> String {
    let base = format!("{source_name}（导入）");
    if !existing_names.iter().any(|name| name == &base) {
        return base;
    }
    for index in 2.. {
        let candidate = format!("{source_name}（导入 {index}）");
        if !existing_names.iter().any(|name| name == &candidate) {
            return candidate;
        }
    }
    unreachable!()
}

pub(crate) fn preview_config_import_from_path(
    root: &Path,
    path: &Path,
) -> Result<ConfigImportPreview, String> {
    if !path.is_file() {
        return Err("选择的配置文件不存在".to_string());
    }
    let bytes = read_config_package_bytes(path)?;
    let existing_names: Vec<String> = read_projects_from_path(&root.join("projects.json"))?
        .into_iter()
        .map(|project| project.name)
        .collect();
    let parsed = parse_config_import_from_bytes(&bytes, &existing_names)?;
    Ok(ConfigImportPreview {
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("配置文件")
            .to_string(),
        valid_configs: parsed
            .valid
            .into_iter()
            .map(|entry| ConfigImportPreviewItem {
                import_key: entry.source_index.to_string(),
                source_name: entry.source_name,
                target_name: entry.target_name,
                advanced_mode: entry.entry.project.advanced_mode,
                battle_scene_count: entry.entry.battle_scenes.len(),
                advanced_battle_scene_count: entry.entry.advanced_battle_scenes.len(),
            })
            .collect(),
        invalid_items: parsed.invalid,
    })
}

pub(crate) fn import_configurations_from_path(
    root: &Path,
    path: &Path,
    import_keys: &[String],
) -> Result<ConfigImportResult, String> {
    if !path.is_file() {
        return Err("选择的配置文件不存在".to_string());
    }
    if import_keys.is_empty() {
        return Err("请选择需要导入的配置".to_string());
    }
    let bytes = read_config_package_bytes(path)?;
    let projects_path = root.join("projects.json");
    let mut projects = read_projects_from_path(&projects_path)?;
    let existing_names: Vec<String> = projects
        .iter()
        .map(|project| project.name.clone())
        .collect();
    let parsed = parse_config_import_from_bytes(&bytes, &existing_names)?;
    let selected: std::collections::HashSet<&str> =
        import_keys.iter().map(String::as_str).collect();
    let selected_entries: Vec<ParsedConfigEntry> = parsed
        .valid
        .into_iter()
        .filter(|entry| selected.contains(entry.source_index.to_string().as_str()))
        .collect();
    if selected_entries.is_empty() {
        return Err("没有可导入的配置".to_string());
    }
    let mut imported_projects = Vec::new();
    for parsed_entry in selected_entries {
        let mut project = parsed_entry.entry.project;
        project.id = uuid::Uuid::new_v4().to_string();
        project.name = parsed_entry.target_name;
        let project = normalize_project(project);
        write_battle_scenes_to_root(root, &project.id, &parsed_entry.entry.battle_scenes)?;
        write_advanced_battle_scenes_to_root(
            root,
            &project.id,
            &parsed_entry.entry.advanced_battle_scenes,
        )?;
        projects.push(project.clone());
        imported_projects.push(project);
    }
    write_projects_to_path(&projects_path, &projects)?;
    Ok(ConfigImportResult { imported_projects })
}

#[tauri::command]
pub(crate) fn list_exportable_configs(
    app: tauri::AppHandle,
) -> Result<Vec<ExportableConfigSummary>, String> {
    exportable_config_summaries_from_root(&app_data_dir(&app))
}

#[tauri::command]
pub(crate) async fn export_configs(
    app: tauri::AppHandle,
    project_ids: Vec<String>,
) -> Result<Option<ExportConfigsResult>, String> {
    let root = app_data_dir(&app);
    let package = export_package_for_project_ids(&root, &project_ids)?;
    let picked = app
        .dialog()
        .file()
        .set_title("选择导出文件夹")
        .blocking_pick_folder();
    let Some(folder) = picked else {
        return Ok(None);
    };
    let folder = folder.into_path().map_err(|e| e.to_string())?;
    let target = folder.join(format!(
        "mash-config-{}.mashconfig.zip",
        current_export_timestamp()
    ));
    write_config_package_zip(&package, &target)?;
    Ok(Some(ExportConfigsResult {
        file_path: target.to_string_lossy().into_owned(),
        exported_count: package.configs.len(),
    }))
}

#[tauri::command]
pub(crate) async fn pick_config_import_file(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("Mash 配置", &["zip", "json"])
        .set_title("选择配置文件")
        .blocking_pick_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub(crate) fn preview_config_import(
    app: tauri::AppHandle,
    file_path: String,
) -> Result<ConfigImportPreview, String> {
    preview_config_import_from_path(&app_data_dir(&app), &PathBuf::from(file_path))
}

#[tauri::command]
pub(crate) fn import_configurations(
    app: tauri::AppHandle,
    file_path: String,
    import_keys: Vec<String>,
) -> Result<ConfigImportResult, String> {
    import_configurations_from_path(&app_data_dir(&app), &PathBuf::from(file_path), &import_keys)
}

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
