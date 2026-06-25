//! App data and resource path resolution.
//! All persisted user data must be rooted under Tauri's app_data_dir.

use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

pub(crate) fn app_data_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir
}

pub(crate) fn dir_has_data(path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&entry_path) else {
            continue;
        };
        if metadata.is_file() || metadata.file_type().is_symlink() {
            return true;
        }
        if metadata.is_dir() && dir_has_data(&entry_path) {
            return true;
        }
    }
    false
}

pub(crate) fn legacy_app_data_candidates(current: &Path) -> Vec<PathBuf> {
    let Some(parent) = current.parent() else {
        return Vec::new();
    };
    ["com.mash.app", "mash"]
        .into_iter()
        .map(|name| parent.join(name))
        .filter(|path| path != current)
        .collect()
}

#[cfg(unix)]
pub(crate) fn copy_symlink(src: &Path, dst: &Path) -> Result<(), String> {
    use std::os::unix::fs::symlink;

    let target = fs::read_link(src).map_err(|e| format!("read symlink failed: {e}"))?;
    symlink(target, dst).map_err(|e| format!("create symlink failed: {e}"))
}

#[cfg(not(unix))]
pub(crate) fn copy_symlink(src: &Path, dst: &Path) -> Result<(), String> {
    fs::copy(src, dst)
        .map(|_| ())
        .map_err(|e| format!("copy symlink target failed: {e}"))
}

pub(crate) fn copy_dir_contents_preserving_links(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("create destination dir failed: {e}"))?;
    for entry in fs::read_dir(src).map_err(|e| format!("read source dir failed: {e}"))? {
        let entry = entry.map_err(|e| format!("read source entry failed: {e}"))?;
        let entry_src = entry.path();
        let entry_dst = dst.join(entry.file_name());
        let metadata = fs::symlink_metadata(&entry_src)
            .map_err(|e| format!("read source metadata failed: {e}"))?;
        if metadata.file_type().is_symlink() {
            copy_symlink(&entry_src, &entry_dst)?;
        } else if metadata.is_dir() {
            copy_dir_contents_preserving_links(&entry_src, &entry_dst)?;
        } else if metadata.is_file() {
            if let Some(parent) = entry_dst.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("create destination dir failed: {e}"))?;
            }
            fs::copy(&entry_src, &entry_dst).map_err(|e| format!("copy file failed: {e}"))?;
            fs::set_permissions(&entry_dst, metadata.permissions())
                .map_err(|e| format!("set file permissions failed: {e}"))?;
        }
    }
    Ok(())
}

pub(crate) fn migrate_legacy_app_data_dir(
    current: &Path,
    candidates: &[PathBuf],
) -> Result<Option<PathBuf>, String> {
    fs::create_dir_all(current).map_err(|e| format!("create app data dir failed: {e}"))?;
    if dir_has_data(current) {
        return Ok(None);
    }

    let Some(source) = candidates
        .iter()
        .find(|path| path.is_dir() && dir_has_data(path))
    else {
        return Ok(None);
    };

    copy_dir_contents_preserving_links(source, current)?;
    fs::write(
        current.join("identifier-migration.json"),
        serde_json::json!({
            "from": source.to_string_lossy(),
            "to": current.to_string_lossy(),
        })
        .to_string(),
    )
    .map_err(|e| format!("write migration marker failed: {e}"))?;
    Ok(Some(source.clone()))
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StartupMigrationStatus {
    pub(crate) migrated: bool,
    pub(crate) from: Option<String>,
    pub(crate) to: String,
}

pub(crate) fn migrate_legacy_app_data(
    app: &tauri::AppHandle,
) -> Result<StartupMigrationStatus, String> {
    let current = app_data_dir(app);
    let source = migrate_legacy_app_data_dir(&current, &legacy_app_data_candidates(&current))?;
    if let Some(source) = &source {
        eprintln!(
            "[app-data-migration] migrated legacy app data from {} to {}",
            source.display(),
            current.display()
        );
    }
    Ok(StartupMigrationStatus {
        migrated: source.is_some(),
        from: source.map(|path| path.to_string_lossy().into_owned()),
        to: current.to_string_lossy().into_owned(),
    })
}

pub(crate) fn app_assets_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app_data_dir(app).join("assets");
    fs::create_dir_all(&dir).ok();
    dir
}

pub(crate) fn projects_file_path(app: &tauri::AppHandle) -> PathBuf {
    app_data_dir(app).join("projects.json")
}

pub(crate) fn app_ui_settings_path(app: &tauri::AppHandle) -> PathBuf {
    app_data_dir(app).join("app_ui_settings.json")
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppUiSettings {
    #[serde(default)]
    pub(crate) active_project_id: Option<String>,
    #[serde(default)]
    pub(crate) theme: Option<String>,
    /// Global portrait selections keyed by `variantKey`. When present, the
    /// chosen portrait id overrides the default "highest ascension" pick for
    /// every team that includes that variant.
    #[serde(default)]
    pub(crate) servant_portrait_selections: std::collections::HashMap<String, u32>,
}

pub(crate) fn project_battle_scenes_path(app: &tauri::AppHandle, project_id: &str) -> PathBuf {
    let dir = app_data_dir(app).join("projects").join(project_id);
    fs::create_dir_all(&dir).ok();
    dir.join("battle_scenes.json")
}

pub(crate) fn project_advanced_battle_scenes_path(
    app: &tauri::AppHandle,
    project_id: &str,
) -> PathBuf {
    let dir = app_data_dir(app).join("projects").join(project_id);
    fs::create_dir_all(&dir).ok();
    dir.join("advanced_battle_scenes.json")
}

pub(crate) fn project_battle_scenes_path_in_root(root: &Path, project_id: &str) -> PathBuf {
    root.join("projects")
        .join(project_id)
        .join("battle_scenes.json")
}

pub(crate) fn project_advanced_battle_scenes_path_in_root(
    root: &Path,
    project_id: &str,
) -> PathBuf {
    root.join("projects")
        .join(project_id)
        .join("advanced_battle_scenes.json")
}

/// Legacy filename used before the per-scene rename. Kept around so
/// `load_battle_scenes` can migrate any pre-existing project data on
/// first launch after the rename.
pub(crate) fn legacy_project_turns_path(app: &tauri::AppHandle, project_id: &str) -> PathBuf {
    app_data_dir(app)
        .join("projects")
        .join(project_id)
        .join("turns.json")
}
