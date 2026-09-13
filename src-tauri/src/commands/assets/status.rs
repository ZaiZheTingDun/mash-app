//! Installed asset status and self-check reporting.

use super::*;

#[derive(serde::Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetBundleStatus {
    pub(crate) installed: bool,
    pub(crate) imported_servants: bool,
    pub(crate) imported_craft_essences: bool,
    pub(crate) servant_files: u64,
    pub(crate) craft_essence_files: u64,
    pub(crate) install_dir: String,
    pub(crate) current_version: Option<u32>,
    pub(crate) app_assets_version: u32,
    pub(crate) remote_latest_version: Option<u32>,
    pub(crate) remote_latest_base_version: Option<u32>,
    pub(crate) target_version: Option<u32>,
    pub(crate) update_available: bool,
    pub(crate) update_download_size: u64,
    pub(crate) update_plan: String,
    pub(crate) latest_url: String,
    pub(crate) remote_manifest_url: Option<String>,
    pub(crate) update_check_error: Option<String>,
}

#[derive(serde::Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelfCheckAssetGroup {
    pub(crate) entries: u64,
    pub(crate) has_image: bool,
    pub(crate) has_json: bool,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelfCheckStatus {
    pub(crate) app_version: String,
    pub(crate) cv_runtime_version: String,
    pub(crate) cv_runtime_installed: bool,
    pub(crate) cv_code_version: String,
    pub(crate) cv_code_installed: bool,
    pub(crate) asset_version: Option<u32>,
    pub(crate) app_assets_version: u32,
    pub(crate) servants: SelfCheckAssetGroup,
    pub(crate) ces: SelfCheckAssetGroup,
}

pub(crate) fn count_files_recursive(dir: &Path) -> Result<u64, String> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in fs::read_dir(dir).map_err(|e| format!("读取素材目录失败: {e}"))? {
        let entry = entry.map_err(|e| format!("读取素材目录失败: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("读取素材类型失败: {e}"))?;
        if file_type.is_dir() {
            count += count_files_recursive(&entry.path())?;
        } else if file_type.is_file() {
            count += 1;
        }
    }
    Ok(count)
}

fn file_ext_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

fn is_supported_asset_image(path: &Path) -> bool {
    matches!(
        file_ext_lower(path).as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp")
    )
}

fn scan_asset_files_recursive(
    dir: &Path,
    has_image: &mut bool,
    has_json: &mut bool,
) -> Result<(), String> {
    if !dir.is_dir() || (*has_image && *has_json) {
        return Ok(());
    }

    for entry in fs::read_dir(dir).map_err(|e| format!("读取素材目录失败: {e}"))? {
        let entry = entry.map_err(|e| format!("读取素材目录失败: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("读取素材类型失败: {e}"))?;
        let path = entry.path();
        if file_type.is_dir() {
            scan_asset_files_recursive(&path, has_image, has_json)?;
        } else if file_type.is_file() {
            *has_image |= is_supported_asset_image(&path);
            *has_json |= file_ext_lower(&path).as_deref() == Some("json");
        }
        if *has_image && *has_json {
            break;
        }
    }
    Ok(())
}

pub(crate) fn scan_self_check_asset_group(dir: &Path) -> SelfCheckAssetGroup {
    if !dir.is_dir() {
        return SelfCheckAssetGroup::default();
    }

    let entries = fs::read_dir(dir)
        .ok()
        .map(|items| {
            items
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry
                        .file_type()
                        .map(|file_type| file_type.is_dir())
                        .unwrap_or(false)
                })
                .count() as u64
        })
        .unwrap_or(0);
    let mut has_image = false;
    let mut has_json = false;
    let _ = scan_asset_files_recursive(dir, &mut has_image, &mut has_json);
    SelfCheckAssetGroup {
        entries,
        has_image,
        has_json,
    }
}

pub(crate) fn self_check_status_for_app(app: &tauri::AppHandle) -> Result<SelfCheckStatus, String> {
    let runtime = runtime_status_for_app(app)?;
    let assets = asset_bundle_status_for_app(app)?;
    let assets_root = app_assets_dir(app);
    Ok(SelfCheckStatus {
        app_version: app.package_info().version.to_string(),
        cv_runtime_version: runtime
            .installed_runtime_version
            .unwrap_or(runtime.required_runtime_version),
        cv_runtime_installed: runtime.runtime_installed,
        cv_code_version: runtime
            .installed_code_version
            .unwrap_or(runtime.required_code_version),
        cv_code_installed: runtime.code_installed,
        asset_version: assets.current_version,
        app_assets_version: assets.app_assets_version,
        servants: scan_self_check_asset_group(&assets_root.join("servants")),
        ces: scan_self_check_asset_group(&assets_root.join("ces")),
    })
}

pub(crate) fn asset_bundle_status_for_app(
    app: &tauri::AppHandle,
) -> Result<AssetBundleStatus, String> {
    let app_manifest = assets_app_manifest(app)?;
    let assets_root = app_assets_dir(app);
    Ok(asset_bundle_status_from_root(&assets_root, &app_manifest))
}

pub(crate) fn asset_bundle_status_from_root(
    assets_root: &Path,
    app_manifest: &AssetsAppManifest,
) -> AssetBundleStatus {
    let servants = assets_root.join("servants");
    let craft_essences = assets_root.join("ces");
    let servant_files = count_files_recursive(&servants).unwrap_or(0);
    let craft_essence_files = count_files_recursive(&craft_essences).unwrap_or(0);
    let imported_servants = servant_files > 0;
    let imported_craft_essences = craft_essence_files > 0;
    let current_version =
        local_asset_version(assets_root, imported_servants, imported_craft_essences);
    let version_installed =
        current_version.is_some_and(|version| version >= app_manifest.assets_version);
    let update_available =
        current_version.is_some_and(|version| version < app_manifest.assets_version);
    AssetBundleStatus {
        installed: imported_servants && imported_craft_essences && version_installed,
        imported_servants,
        imported_craft_essences,
        servant_files,
        craft_essence_files,
        install_dir: assets_root.to_string_lossy().into_owned(),
        current_version,
        app_assets_version: app_manifest.assets_version,
        remote_latest_version: None,
        remote_latest_base_version: None,
        target_version: update_available.then_some(app_manifest.assets_version),
        update_available,
        update_download_size: 0,
        update_plan: if update_available { "pending" } else { "none" }.to_string(),
        latest_url: app_manifest.latest_url.clone(),
        remote_manifest_url: None,
        update_check_error: None,
    }
}

#[tauri::command]
pub(crate) fn get_asset_bundle_status(app: tauri::AppHandle) -> Result<AssetBundleStatus, String> {
    asset_bundle_status_for_app(&app)
}

#[tauri::command]
pub(crate) fn get_self_check_status(app: tauri::AppHandle) -> Result<SelfCheckStatus, String> {
    self_check_status_for_app(&app)
}
