//! Project catalog normalization and group-management commands.

use super::*;
use std::collections::HashSet;

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

pub(crate) fn read_project_catalog(
    app: &tauri::AppHandle,
    projects: &[Project],
) -> Result<ProjectCatalog, String> {
    read_project_catalog_from_path(&project_catalog_path(app), projects)
}

pub(crate) fn write_project_catalog(
    app: &tauri::AppHandle,
    catalog: &ProjectCatalog,
) -> Result<(), String> {
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

pub(crate) fn remove_project_from_catalog(catalog: &mut ProjectCatalog, project_id: &str) {
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
