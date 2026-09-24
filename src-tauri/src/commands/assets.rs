//! Asset bundle status, import, download, and self-check commands.
//! Base packs replace managed asset trees; patches merge without deleting unrelated files.

use super::*;

pub(crate) const ASSETS_MANIFEST_JSON: &str = include_str!("../../resources/assets-manifest.json");
pub(crate) const ASSET_DOWNLOAD_PROGRESS_EVENT: &str = "asset-download-progress";
// To add a new asset directory: append its name here. install_asset_directories and
// cleanup_replaced_asset_trees will handle it automatically.
pub(crate) const ASSET_DIRS: &[&str] = &["servants", "ces", "icons", "skills", "mystic-codes"];

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetsAppManifest {
    pub(crate) assets_version: u32,
    pub(crate) latest_url: String,
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetsLatestManifest {
    pub(crate) latest: u32,
    pub(crate) latest_base: u32,
    pub(crate) manifest: String,
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetsRemoteManifest {
    pub(crate) latest: u32,
    pub(crate) latest_base: u32,
    pub(crate) base: AssetsBaseRelease,
    #[serde(default)]
    pub(crate) patches: Vec<AssetsPatchRelease>,
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetsBaseRelease {
    pub(crate) version: u32,
    #[serde(default)]
    pub(crate) packs: Vec<AssetsPackRelease>,
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetsPackRelease {
    pub(crate) name: String,
    pub(crate) file: String,
    pub(crate) sha256: String,
    pub(crate) size: u64,
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetsPatchRelease {
    pub(crate) from: u32,
    pub(crate) to: u32,
    pub(crate) file: String,
    pub(crate) sha256: String,
    pub(crate) size: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetVersionRecord {
    pub(crate) version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AssetInstallPlan {
    None,
    Base {
        target_version: u32,
        packs: Vec<AssetsPackRelease>,
        patches: Vec<AssetsPatchRelease>,
    },
    ForceBase {
        target_version: u32,
        packs: Vec<AssetsPackRelease>,
        patches: Vec<AssetsPatchRelease>,
    },
    Patches {
        target_version: u32,
        patches: Vec<AssetsPatchRelease>,
    },
}

impl AssetInstallPlan {
    pub(crate) fn plan_type(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Base { .. } => "base",
            Self::ForceBase { .. } => "force-base",
            Self::Patches { .. } => "patch",
        }
    }

    pub(crate) fn target_version(&self) -> Option<u32> {
        match self {
            Self::None => None,
            Self::Base { target_version, .. }
            | Self::ForceBase { target_version, .. }
            | Self::Patches { target_version, .. } => Some(*target_version),
        }
    }

    #[cfg(test)]
    pub(crate) fn download_size(&self) -> u64 {
        match self {
            Self::None => 0,
            Self::Base { packs, patches, .. } | Self::ForceBase { packs, patches, .. } => {
                packs.iter().map(|item| item.size).sum::<u64>()
                    + patches.iter().map(|item| item.size).sum::<u64>()
            }
            Self::Patches { patches, .. } => patches.iter().map(|item| item.size).sum(),
        }
    }
}

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetDownloadProgress {
    pub(crate) kind: String,
    pub(crate) phase: String,
    pub(crate) downloaded_bytes: u64,
    pub(crate) total_bytes: Option<u64>,
    pub(crate) bytes_per_second: Option<f64>,
    pub(crate) eta_seconds: Option<u64>,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetDownloadInstallResult {
    pub(crate) installed: bool,
    pub(crate) installed_version: Option<u32>,
    pub(crate) plan: String,
    pub(crate) servant_files: u64,
    pub(crate) craft_essence_files: u64,
    pub(crate) install_dir: String,
}

pub(crate) fn refresh_asset_protocol_scope(app: &tauri::AppHandle) -> Result<(), String> {
    let asset_scope = app.asset_protocol_scope();
    if let Some(dir) = resolve_servant_assets_dir(app) {
        asset_scope
            .allow_directory(dir, true)
            .map_err(|e| e.to_string())?;
    }
    if let Some(dir) = resolve_ce_assets_dir(app) {
        asset_scope
            .allow_directory(dir, true)
            .map_err(|e| e.to_string())?;
    }
    if let Some(dir) = resolve_mystic_code_assets_dir(app) {
        asset_scope
            .allow_directory(dir, true)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn parse_assets_app_manifest(contents: &str) -> Result<AssetsAppManifest, String> {
    serde_json::from_str(contents).map_err(|e| format!("解析素材包配置失败: {e}"))
}

pub(crate) fn assets_app_manifest(app: &tauri::AppHandle) -> Result<AssetsAppManifest, String> {
    let contents = app
        .path()
        .resource_dir()
        .ok()
        .map(|base| base.join("resources").join("assets-manifest.json"))
        .and_then(|path| fs::read_to_string(path).ok())
        .unwrap_or_else(|| ASSETS_MANIFEST_JSON.to_string());
    parse_assets_app_manifest(&contents)
}

pub(crate) fn asset_version_path(assets_root: &Path) -> PathBuf {
    assets_root.join("assets-version.json")
}

pub(crate) fn read_asset_version(assets_root: &Path) -> Option<AssetVersionRecord> {
    fs::read_to_string(asset_version_path(assets_root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub(crate) fn write_asset_version(assets_root: &Path, version: u32) -> Result<(), String> {
    fs::create_dir_all(assets_root).map_err(|e| format!("创建素材目录失败: {e}"))?;
    let record = AssetVersionRecord { version };
    fs::write(
        asset_version_path(assets_root),
        serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入素材包版本记录失败: {e}"))
}

pub(crate) fn local_asset_version(
    assets_root: &Path,
    imported_servants: bool,
    imported_craft_essences: bool,
) -> Option<u32> {
    read_asset_version(assets_root)
        .map(|record| record.version)
        .or_else(|| (imported_servants || imported_craft_essences).then_some(1))
}

pub(crate) fn http_text(url: &str) -> Result<String, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("创建下载客户端失败: {e}"))?
        .get(url)
        .send()
        .map_err(|e| format!("读取远端素材包配置失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("读取远端素材包配置失败: {e}"))?
        .text()
        .map_err(|e| format!("读取远端素材包配置失败: {e}"))
}

pub(crate) fn parse_assets_latest_manifest(contents: &str) -> Result<AssetsLatestManifest, String> {
    serde_json::from_str(contents).map_err(|e| format!("解析远端素材 latest.json 失败: {e}"))
}

pub(crate) fn parse_assets_remote_manifest(contents: &str) -> Result<AssetsRemoteManifest, String> {
    serde_json::from_str(contents).map_err(|e| format!("解析远端素材 manifest 失败: {e}"))
}

pub(crate) fn url_dir(url: &str) -> String {
    url.rsplit_once('/')
        .map(|(base, _)| format!("{base}/"))
        .unwrap_or_default()
}

pub(crate) fn join_remote_url(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        format!("{}{}", base.trim_end_matches('/'), format!("/{path}"))
    }
}

pub(crate) fn fetch_assets_remote_manifest(
    latest_url: &str,
) -> Result<(AssetsLatestManifest, AssetsRemoteManifest, String, String), String> {
    let latest_contents = http_text(latest_url)?;
    let latest = parse_assets_latest_manifest(&latest_contents)?;
    let latest_base_url = url_dir(latest_url);
    let manifest_url = join_remote_url(&latest_base_url, &latest.manifest);
    let manifest_contents = http_text(&manifest_url)?;
    let manifest = parse_assets_remote_manifest(&manifest_contents)?;
    Ok((latest, manifest, latest_base_url, manifest_url))
}

pub(crate) fn asset_update_plan(
    current_version: Option<u32>,
    assets_complete: bool,
    app_assets_version: u32,
    remote: &AssetsRemoteManifest,
    force_base: bool,
) -> AssetInstallPlan {
    let target_version = app_assets_version.min(remote.latest);
    if force_base {
        let patches =
            asset_patch_chain(remote.base.version, target_version, remote).unwrap_or_default();
        return AssetInstallPlan::ForceBase {
            target_version,
            packs: remote.base.packs.clone(),
            patches,
        };
    }

    let current_version = assets_complete.then_some(current_version).flatten();

    if current_version.is_some_and(|version| version >= target_version) {
        return AssetInstallPlan::None;
    }

    let Some(version) = current_version else {
        let Some(patches) = asset_patch_chain(remote.base.version, target_version, remote) else {
            return AssetInstallPlan::Base {
                target_version: remote.base.version.min(target_version),
                packs: remote.base.packs.clone(),
                patches: Vec::new(),
            };
        };
        return AssetInstallPlan::Base {
            target_version,
            packs: remote.base.packs.clone(),
            patches,
        };
    };

    let Some(patches) = asset_patch_chain(version, target_version, remote) else {
        let Some(base_patches) = asset_patch_chain(remote.base.version, target_version, remote)
        else {
            return AssetInstallPlan::Base {
                target_version: remote.base.version.min(target_version),
                packs: remote.base.packs.clone(),
                patches: Vec::new(),
            };
        };
        return AssetInstallPlan::Base {
            target_version,
            packs: remote.base.packs.clone(),
            patches: base_patches,
        };
    };

    if patches.is_empty() {
        AssetInstallPlan::None
    } else {
        AssetInstallPlan::Patches {
            target_version,
            patches,
        }
    }
}

pub(crate) fn asset_patch_chain(
    mut version: u32,
    target_version: u32,
    remote: &AssetsRemoteManifest,
) -> Option<Vec<AssetsPatchRelease>> {
    let mut patches = Vec::new();
    while version < target_version {
        let patch = remote.patches.iter().find(|patch| {
            patch.from == version && patch.to <= target_version && patch.to > version
        })?;
        patches.push(patch.clone());
        version = patch.to;
    }
    Some(patches)
}

mod import;
pub(crate) use import::*;

mod status;
pub(crate) use status::*;

mod download;
pub(crate) use download::*;
