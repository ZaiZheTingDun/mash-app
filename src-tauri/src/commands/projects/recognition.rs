//! Read and update recognition thresholds against the current persisted project.

use super::*;
use crate::commands::automation::effective_recognition_settings;
use crate::commands::settings::{
    normalize_recognition_threshold, RecognitionSettings, ThresholdKey,
};

#[tauri::command]
pub(crate) fn get_project_recognition_settings(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<RecognitionSettings, String> {
    let _guard = app.state::<ProjectStoreLock>().inner().lock()?;
    let projects = read_projects(&app)?;
    let project = projects
        .iter()
        .find(|project| project.id == project_id)
        .ok_or_else(|| format!("project not found: {project_id}"))?;
    let global = *app
        .state::<Mutex<RecognitionSettings>>()
        .lock()
        .map_err(|err| err.to_string())?;
    Ok(effective_recognition_settings(
        global,
        project.recognition_settings,
    ))
}

fn set_threshold(
    project: &mut Project,
    key: ThresholdKey,
    value: Option<f64>,
) -> Result<(), String> {
    let value = value
        .map(|value| normalize_recognition_threshold(key, value))
        .transpose()?;
    let mut settings = project.recognition_settings.unwrap_or_default();
    match key {
        ThresholdKey::SupportCeThreshold => settings.support_ce_threshold = value,
        ThresholdKey::SupportCeFullGateThreshold => settings.support_ce_full_gate_threshold = value,
        ThresholdKey::SupportMlbIconThreshold => settings.support_mlb_icon_threshold = value,
        ThresholdKey::SupportBondIconThreshold => settings.support_bond_icon_threshold = value,
    }
    // Keep unrelated overrides, including explicit false values.
    project.recognition_settings =
        (settings != ProjectRecognitionSettings::default()).then_some(settings);
    Ok(())
}

fn save_threshold_to_path(
    path: &Path,
    project_id: &str,
    key: ThresholdKey,
    value: Option<f64>,
) -> Result<Project, String> {
    let mut projects = read_projects_from_path(path)?;
    let project = projects
        .iter_mut()
        .find(|project| project.id == project_id)
        .ok_or_else(|| format!("project not found: {project_id}"))?;
    set_threshold(project, key, value)?;
    let saved = project.clone();
    write_projects_to_path(path, &projects)?;
    Ok(saved)
}

#[tauri::command]
pub(crate) fn set_project_recognition_threshold(
    app: tauri::AppHandle,
    project_id: String,
    key: ThresholdKey,
    value: Option<f64>,
) -> Result<Project, String> {
    let _guard = app.state::<ProjectStoreLock>().inner().lock()?;
    save_threshold_to_path(&projects_file_path(&app), &project_id, key, value)
}

#[cfg(test)]
mod tests;
