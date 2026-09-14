//! Servant portrait and face asset selection.

use super::{servants_data, ServantInfo};
use crate::commands::projects::update_app_ui_settings;
use crate::commands::runtime::resolve_servant_assets_dir;
use crate::paths::AppUiSettings;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Look inside a single servant's asset directory and return the path of
/// the highest-numbered `narrow_servant_*.png` portrait, or `None` when
/// no such file exists. `narrow_servant_<n>.png` corresponds to
/// ascension stage `n` (1-4 for typical servants, with `4` being the
/// final art); picking the lexicographic max is a stable proxy for
/// "most-recent ascension" since the source filenames are
/// zero-prefix-free single digits.
pub(crate) fn pick_portrait_in(servant_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(servant_dir).ok()?;
    entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("narrow_servant_") && name.ends_with(".png"))
        })
        .max()
}

pub(crate) fn pick_portrait_by_id_in(servant_dir: &Path, portrait_id: u32) -> Option<PathBuf> {
    let candidate = servant_dir.join(format!("narrow_servant_{portrait_id}.png"));
    candidate.is_file().then_some(candidate)
}

pub(crate) fn pick_face_in(servant_dir: &Path) -> Option<PathBuf> {
    pick_faces_desc_in(servant_dir).into_iter().next()
}

pub(crate) fn pick_faces_desc_in(servant_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(servant_dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| is_face_template_path(path))
        .collect();
    paths.sort_by(|a, b| face_template_stage(b).cmp(&face_template_stage(a)));
    paths
}

fn is_face_template_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("face_servant_") && name.ends_with(".png"))
}

fn face_template_stage(path: &Path) -> u32 {
    path.file_stem()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("face_servant_"))
        .and_then(|name| name.parse::<u32>().ok())
        .unwrap_or(0)
}

fn pick_face_by_id_in(servant_dir: &Path, face_id: u32) -> Option<PathBuf> {
    let candidate = servant_dir.join(format!("face_servant_{face_id}.png"));
    candidate.is_file().then_some(candidate)
}

/// Enumerate all `narrow_servant_*.png` files in `servant_dir` in
/// natural numeric order, returning `(id, path)` pairs.
pub(crate) fn list_portraits_in(servant_dir: &Path) -> Vec<(u32, PathBuf)> {
    let Ok(entries) = fs::read_dir(servant_dir) else {
        return Vec::new();
    };
    let mut portraits: Vec<(u32, PathBuf)> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            let stem = name.strip_prefix("narrow_servant_")?.strip_suffix(".png")?;
            let id = stem.parse::<u32>().ok()?;
            Some((id, path))
        })
        .collect();
    portraits.sort_by_key(|(id, _)| *id);
    portraits
}

pub(crate) fn portrait_id_is_allowed(allowed_ids: &[u32], portrait_id: u32) -> bool {
    allowed_ids.is_empty() || allowed_ids.contains(&portrait_id)
}

pub(crate) fn list_portraits_for_ids_in(
    servant_dir: &Path,
    allowed_ids: &[u32],
) -> Vec<(u32, PathBuf)> {
    list_portraits_in(servant_dir)
        .into_iter()
        .filter(|(id, _)| portrait_id_is_allowed(allowed_ids, *id))
        .collect()
}

pub(crate) fn pick_portrait_for_ids_with_preferences_in(
    servant_dir: &Path,
    global_id: Option<u32>,
    face_id: Option<u32>,
    allowed_ids: &[u32],
) -> Option<PathBuf> {
    global_id
        .filter(|id| portrait_id_is_allowed(allowed_ids, *id))
        .and_then(|id| pick_portrait_by_id_in(servant_dir, id))
        .or_else(|| {
            face_id
                .filter(|id| portrait_id_is_allowed(allowed_ids, *id))
                .and_then(|id| pick_portrait_by_id_in(servant_dir, id))
        })
        .or_else(|| {
            if allowed_ids.is_empty() {
                pick_portrait_in(servant_dir)
            } else {
                list_portraits_for_ids_in(servant_dir, allowed_ids)
                    .into_iter()
                    .next_back()
                    .map(|(_, path)| path)
            }
        })
}

pub(crate) fn pick_portrait_with_preferences_in(
    servant_dir: &Path,
    global_id: Option<u32>,
    face_id: Option<u32>,
) -> Option<PathBuf> {
    pick_portrait_for_ids_with_preferences_in(servant_dir, global_id, face_id, &[])
}

pub(crate) fn pick_face_for_ids_with_preferences_in(
    servant_dir: &Path,
    global_id: Option<u32>,
    face_id: Option<u32>,
    allowed_ids: &[u32],
) -> Option<PathBuf> {
    global_id
        .filter(|id| portrait_id_is_allowed(allowed_ids, *id))
        .and_then(|id| pick_face_by_id_in(servant_dir, id))
        .or_else(|| {
            face_id
                .filter(|id| portrait_id_is_allowed(allowed_ids, *id))
                .and_then(|id| pick_face_by_id_in(servant_dir, id))
        })
        .or_else(|| {
            if allowed_ids.is_empty() {
                pick_face_in(servant_dir)
            } else {
                pick_faces_desc_in(servant_dir)
                    .into_iter()
                    .find(|path| portrait_id_is_allowed(allowed_ids, face_template_stage(path)))
            }
        })
}

fn servant_for_variant(servant_id: u32, variant_key: &str) -> Result<&'static ServantInfo, String> {
    servants_data()
        .iter()
        .find(|servant| servant.id == servant_id && servant.variant_key == variant_key)
        .ok_or_else(|| format!("从者 #{servant_id} 不包含立绘集合 {variant_key}"))
}

fn servant_by_variant_key(variant_key: &str) -> Result<&'static ServantInfo, String> {
    servants_data()
        .iter()
        .find(|servant| servant.variant_key == variant_key)
        .ok_or_else(|| format!("未找到立绘集合 {variant_key}"))
}

/// Resolve the full-art portrait file for a single servant, returning
/// the absolute path so the frontend can hand it to `convertFileSrc()`.
///
/// Priority: (1) global portrait selection stored in `app_ui_settings.json`
/// for this `variant_key`, (2) explicit `face_id` variant default,
/// (3) default "highest ascension" portrait via [`pick_portrait_in`].
/// Returning `Ok(None)` on a missing file lets the UI fall back to a
/// placeholder without surfacing an error toast.
#[tauri::command]
pub(crate) fn get_servant_portrait_path(
    app: tauri::AppHandle,
    settings_state: tauri::State<'_, Mutex<AppUiSettings>>,
    servant_id: u32,
    face_id: Option<u32>,
    variant_key: Option<String>,
) -> Result<Option<String>, String> {
    let Some(root) = resolve_servant_assets_dir(&app) else {
        return Ok(None);
    };
    let servant_dir = root.join(servant_id.to_string());
    let variant = variant_key
        .as_deref()
        .map(|key| servant_for_variant(servant_id, key))
        .transpose()?;
    let allowed_ids = variant
        .map(|servant| servant.portrait_ids.as_slice())
        .unwrap_or_default();
    let settings = settings_state.lock().unwrap();
    let global_id = variant_key
        .as_deref()
        .and_then(|key| settings.servant_portrait_selections.get(key).copied());
    let picked = if allowed_ids.is_empty() {
        pick_portrait_with_preferences_in(&servant_dir, global_id, face_id)
    } else {
        pick_portrait_for_ids_with_preferences_in(&servant_dir, global_id, face_id, allowed_ids)
    };
    Ok(picked.map(|path| path.to_string_lossy().into_owned()))
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PortraitOption {
    pub(crate) id: u32,
    pub(crate) path: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServantPortraitOptions {
    pub(crate) options: Vec<PortraitOption>,
    pub(crate) selected_id: Option<u32>,
}

/// List all available full-art portraits for `servant_id` in ascending
/// numeric order, along with the currently-selected portrait id (if any)
/// for this `variant_key` from the global settings.
#[tauri::command]
pub(crate) fn list_servant_portraits(
    app: tauri::AppHandle,
    settings_state: tauri::State<'_, Mutex<AppUiSettings>>,
    servant_id: u32,
    variant_key: String,
) -> Result<ServantPortraitOptions, String> {
    let servant = servant_for_variant(servant_id, &variant_key)?;
    let Some(root) = resolve_servant_assets_dir(&app) else {
        return Ok(ServantPortraitOptions {
            options: Vec::new(),
            selected_id: None,
        });
    };
    let servant_dir = root.join(servant_id.to_string());
    let options: Vec<PortraitOption> =
        list_portraits_for_ids_in(&servant_dir, &servant.portrait_ids)
            .into_iter()
            .map(|(id, path)| PortraitOption {
                id,
                path: path.to_string_lossy().into_owned(),
            })
            .collect();
    let settings = settings_state.lock().unwrap();
    let selected_id = settings
        .servant_portrait_selections
        .get(&variant_key)
        .copied()
        .filter(|id| {
            portrait_id_is_allowed(&servant.portrait_ids, *id)
                && options.iter().any(|option| option.id == *id)
        });
    Ok(ServantPortraitOptions {
        options,
        selected_id,
    })
}

/// Persist a global portrait selection for `variant_key`. This overrides
/// the default highest-ascension pick for all teams using this variant.
#[tauri::command]
pub(crate) fn save_servant_portrait_selection(
    app: tauri::AppHandle,
    settings_state: tauri::State<'_, Mutex<AppUiSettings>>,
    variant_key: String,
    portrait_id: u32,
) -> Result<(), String> {
    let servant = servant_by_variant_key(&variant_key)?;
    if !portrait_id_is_allowed(&servant.portrait_ids, portrait_id) {
        return Err(format!(
            "立绘 {portrait_id} 不属于从者 {} 的当前集合",
            servant.name_cn
        ));
    }
    let root =
        resolve_servant_assets_dir(&app).ok_or_else(|| "未找到从者立绘资源目录".to_string())?;
    if pick_portrait_by_id_in(&root.join(servant.id.to_string()), portrait_id).is_none() {
        return Err(format!("从者 {} 缺少立绘 {portrait_id}", servant.name_cn));
    }
    update_app_ui_settings(&app, settings_state.inner(), |settings| {
        settings
            .servant_portrait_selections
            .insert(variant_key, portrait_id);
    })?;
    Ok(())
}

#[tauri::command]
pub(crate) fn get_servant_face_path(
    app: tauri::AppHandle,
    settings_state: tauri::State<'_, Mutex<AppUiSettings>>,
    servant_id: u32,
    face_id: Option<u32>,
    variant_key: Option<String>,
) -> Result<Option<String>, String> {
    let Some(root) = resolve_servant_assets_dir(&app) else {
        return Ok(None);
    };
    let servant_dir = root.join(servant_id.to_string());
    let variant = variant_key
        .as_deref()
        .map(|key| servant_for_variant(servant_id, key))
        .transpose()?;
    let allowed_ids = variant
        .map(|servant| servant.portrait_ids.as_slice())
        .unwrap_or_default();
    let settings = settings_state.lock().unwrap();
    let global_id = variant_key
        .as_deref()
        .and_then(|key| settings.servant_portrait_selections.get(key).copied());
    let picked =
        pick_face_for_ids_with_preferences_in(&servant_dir, global_id, face_id, allowed_ids);
    Ok(picked.map(|path| path.to_string_lossy().into_owned()))
}
