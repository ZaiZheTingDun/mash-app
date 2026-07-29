//! Asset bundle status, import, download, and self-check commands.
//! Base packs replace managed asset trees; patches merge without deleting unrelated files.

use super::*;

pub(crate) const ASSETS_MANIFEST_JSON: &str = include_str!("../../resources/assets-manifest.json");
pub(crate) const ASSET_DOWNLOAD_PROGRESS_EVENT: &str = "asset-download-progress";
// To add a new asset directory: append its name here. install_asset_directories and
// cleanup_replaced_asset_trees will handle it automatically.
pub(crate) const ASSET_DIRS: &[&str] = &["servants", "ces", "icons", "mystic-codes"];

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

#[derive(serde::Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetBundleImportResult {
    pub(crate) imported_servants: bool,
    pub(crate) imported_craft_essences: bool,
    pub(crate) servant_files: u64,
    pub(crate) craft_essence_files: u64,
    pub(crate) install_dir: String,
}

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FileCopyStats {
    pub(crate) files: u64,
    pub(crate) bytes: u64,
}

impl std::ops::AddAssign for FileCopyStats {
    fn add_assign(&mut self, rhs: Self) {
        self.files += rhs.files;
        self.bytes += rhs.bytes;
    }
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

pub(crate) fn read_import_asset_version(
    extracted_root: &Path,
    import_root: &Path,
) -> Result<Option<u32>, String> {
    let mut candidates = vec![import_root.join("assets-version.json")];
    let root_record = extracted_root.join("assets-version.json");
    if root_record != candidates[0] {
        candidates.push(root_record);
    }

    for path in candidates {
        if !path.exists() {
            continue;
        }
        let contents =
            fs::read_to_string(&path).map_err(|e| format!("读取素材包版本记录失败: {e}"))?;
        let record: AssetVersionRecord =
            serde_json::from_str(&contents).map_err(|e| format!("解析素材包版本记录失败: {e}"))?;
        return Ok(Some(record.version));
    }

    Ok(None)
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

pub(crate) fn extract_zip_archive(zip_path: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("无法打开压缩包: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("无法读取 zip 压缩包: {e}"))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("读取 zip 条目失败: {e}"))?;
        let Some(relative) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        let output = destination.join(relative);
        if entry.name().ends_with('/') {
            fs::create_dir_all(&output).map_err(|e| format!("创建目录失败: {e}"))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let mut out = fs::File::create(&output).map_err(|e| format!("写入解压文件失败: {e}"))?;
        io::copy(&mut entry, &mut out).map_err(|e| format!("解压文件失败: {e}"))?;
        out.flush().map_err(|e| format!("写入解压文件失败: {e}"))?;
    }

    Ok(())
}

pub(crate) fn locate_import_root(extracted_root: &Path) -> PathBuf {
    let nested_assets = extracted_root.join("assets");
    if nested_assets.is_dir() {
        nested_assets
    } else {
        extracted_root.to_path_buf()
    }
}

pub(crate) fn copy_asset_tree(source: &Path, destination: &Path) -> Result<FileCopyStats, String> {
    let mut stats = FileCopyStats::default();
    fs::create_dir_all(destination).map_err(|e| format!("创建素材目录失败: {e}"))?;
    let entries = fs::read_dir(source).map_err(|e| format!("读取素材目录失败: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取素材目录失败: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("读取素材类型失败: {e}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if file_type.is_dir() {
            stats += copy_asset_tree(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("创建素材目录失败: {e}"))?;
            }
            let bytes = fs::copy(&source_path, &destination_path)
                .map_err(|e| format!("复制素材文件失败: {e}"))?;
            stats += FileCopyStats { files: 1, bytes };
        }
    }
    Ok(stats)
}

pub(crate) fn merge_asset_tree(source: &Path, destination: &Path) -> Result<FileCopyStats, String> {
    copy_asset_tree(source, destination)
}

pub(crate) fn cleanup_replaced_asset_trees(assets_root: &Path) {
    let Ok(entries) = fs::read_dir(assets_root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let is_replaced_asset_tree = ASSET_DIRS
            .iter()
            .any(|dir| name.starts_with(&format!("{dir}.replaced-")));
        if !is_replaced_asset_tree || !path.is_dir() {
            continue;
        }
        if let Err(err) = fs::remove_dir_all(&path) {
            eprintln!(
                "[assets] cleanup stale replaced asset tree failed: {}: {err}",
                path.display()
            );
        }
    }
}

pub(crate) fn replace_asset_tree(
    source: &Path,
    destination: &Path,
) -> Result<FileCopyStats, String> {
    let staging = destination.with_extension(format!("import-{}", uuid::Uuid::new_v4()));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| format!("清理临时目录失败: {e}"))?;
    }
    let stats = copy_asset_tree(source, &staging)?;

    let replaced = destination.with_extension(format!("replaced-{}", uuid::Uuid::new_v4()));
    if replaced.exists() {
        fs::remove_dir_all(&replaced).map_err(|e| format!("清理旧素材临时目录失败: {e}"))?;
    }
    if destination.exists() {
        fs::rename(destination, &replaced).map_err(|e| format!("替换旧素材失败: {e}"))?;
    }
    if let Err(err) = fs::rename(&staging, destination) {
        if replaced.exists() {
            let _ = fs::rename(&replaced, destination);
        }
        return Err(format!("安装素材失败: {err}"));
    }
    if replaced.exists() {
        if let Err(err) = fs::remove_dir_all(&replaced) {
            eprintln!(
                "[assets] cleanup replaced asset tree failed: {}: {err}",
                replaced.display()
            );
        }
    }
    Ok(stats)
}

pub(crate) fn install_asset_directories(
    import_root: &Path,
    assets_root: &Path,
    replace_existing: bool,
) -> Result<(bool, bool, FileCopyStats, FileCopyStats), String> {
    let any_present = ASSET_DIRS
        .iter()
        .any(|name| import_root.join(name).is_dir());
    if !any_present {
        return Err(format!("压缩包内未找到素材目录 ({})", ASSET_DIRS.join("/")));
    }

    fs::create_dir_all(assets_root).map_err(|e| format!("创建素材目录失败: {e}"))?;
    let install_tree = |source: &Path, destination: PathBuf| {
        if replace_existing {
            replace_asset_tree(source, &destination)
        } else {
            merge_asset_tree(source, &destination)
        }
    };

    let mut servant_stats = FileCopyStats::default();
    let mut ce_stats = FileCopyStats::default();
    let mut has_servants = false;
    let mut has_ces = false;
    for &name in ASSET_DIRS {
        let source = import_root.join(name);
        if !source.is_dir() {
            continue;
        }
        let stats = install_tree(&source, assets_root.join(name))?;
        match name {
            "servants" => {
                has_servants = true;
                servant_stats = stats;
            }
            "ces" => {
                has_ces = true;
                ce_stats = stats;
            }
            _ => {}
        }
    }
    Ok((has_servants, has_ces, servant_stats, ce_stats))
}

pub(crate) fn import_asset_bundle_from_zip_path(
    zip_path: &Path,
    assets_root: &Path,
) -> Result<AssetBundleImportResult, String> {
    cleanup_replaced_asset_trees(assets_root);
    let temp = tempfile::Builder::new()
        .prefix("asset-import-")
        .tempdir_in(
            assets_root
                .parent()
                .ok_or_else(|| "无法定位素材根目录".to_string())?,
        )
        .map_err(|e| format!("创建临时目录失败: {e}"))?;
    let extracted_root = temp.path().join("unzipped");
    fs::create_dir_all(&extracted_root).map_err(|e| format!("创建临时目录失败: {e}"))?;
    extract_zip_archive(zip_path, &extracted_root)?;

    let import_root = locate_import_root(&extracted_root);
    let imported_version = read_import_asset_version(&extracted_root, &import_root)?.unwrap_or(1);
    let (has_servants, has_ces, servant_stats, ce_stats) =
        install_asset_directories(&import_root, assets_root, true)?;
    write_asset_version(assets_root, imported_version)?;
    cleanup_replaced_asset_trees(assets_root);

    Ok(AssetBundleImportResult {
        imported_servants: has_servants,
        imported_craft_essences: has_ces,
        servant_files: servant_stats.files,
        craft_essence_files: ce_stats.files,
        install_dir: assets_root.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_asset_directories_accepts_icons_without_servants_or_ces() {
        let temp = tempfile::tempdir().unwrap();
        let import_root = temp.path().join("import");
        let assets_root = temp.path().join("assets");
        fs::create_dir_all(import_root.join("icons")).unwrap();
        fs::write(import_root.join("icons").join("skill.png"), b"icon").unwrap();

        let (has_servants, has_ces, servant_stats, ce_stats) =
            install_asset_directories(&import_root, &assets_root, true).unwrap();

        assert!(!has_servants);
        assert!(!has_ces);
        assert_eq!(servant_stats, FileCopyStats::default());
        assert_eq!(ce_stats, FileCopyStats::default());
        assert!(assets_root.join("icons").join("skill.png").is_file());
    }

    #[test]
    fn install_asset_directories_rejects_archives_without_known_asset_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let import_root = temp.path().join("import");
        let assets_root = temp.path().join("assets");
        fs::create_dir_all(import_root.join("other")).unwrap();

        let err = install_asset_directories(&import_root, &assets_root, true).unwrap_err();

        assert!(err.contains("servants/ces/icons/mystic-codes"));
    }

    #[test]
    fn cleanup_replaced_asset_trees_removes_all_managed_asset_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let assets_root = temp.path();
        let stale_icons = assets_root.join("icons.replaced-123");
        let stale_codes = assets_root.join("mystic-codes.replaced-456");
        let keep = assets_root.join("other.replaced-789");
        fs::create_dir_all(&stale_icons).unwrap();
        fs::create_dir_all(&stale_codes).unwrap();
        fs::create_dir_all(&keep).unwrap();

        cleanup_replaced_asset_trees(assets_root);

        assert!(!stale_icons.exists());
        assert!(!stale_codes.exists());
        assert!(keep.exists());
    }
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

pub(crate) fn file_ext_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

pub(crate) fn is_supported_asset_image(path: &Path) -> bool {
    matches!(
        file_ext_lower(path).as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp")
    )
}

pub(crate) fn scan_asset_files_recursive(
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

#[tauri::command]
pub(crate) async fn pick_asset_bundle(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("ZIP", &["zip"])
        .set_title("选择素材压缩包")
        .blocking_pick_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub(crate) async fn import_asset_bundle(
    app: tauri::AppHandle,
    zip_path: String,
) -> Result<AssetBundleImportResult, String> {
    let zip_path = PathBuf::from(zip_path);
    if !zip_path.is_file() {
        return Err("选择的压缩包不存在".to_string());
    }
    let result = import_asset_bundle_from_zip_path(&zip_path, &app_assets_dir(&app))?;
    refresh_asset_protocol_scope(&app)?;
    Ok(result)
}

pub(crate) fn asset_downloads_dir(assets_root: &Path) -> PathBuf {
    assets_root.join(".downloads")
}

pub(crate) fn emit_asset_download_progress(
    app: &tauri::AppHandle,
    kind: &str,
    phase: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    bytes_per_second: Option<f64>,
    eta_seconds: Option<u64>,
) {
    app.emit(
        ASSET_DOWNLOAD_PROGRESS_EVENT,
        AssetDownloadProgress {
            kind: kind.to_string(),
            phase: phase.to_string(),
            downloaded_bytes,
            total_bytes,
            bytes_per_second,
            eta_seconds,
        },
    )
    .ok();
}

pub(crate) fn download_asset_artifact(
    app: &tauri::AppHandle,
    kind: &str,
    url: &str,
    destination: &Path,
    cancel: &AtomicBool,
) -> Result<(), String> {
    ensure_download_not_cancelled(cancel)?;
    let parent = destination
        .parent()
        .ok_or_else(|| "素材包下载路径缺少父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("创建素材包下载目录失败: {e}"))?;
    let partial = destination.with_extension("zip.part");
    if partial.exists() {
        fs::remove_file(&partial).map_err(|e| format!("清理素材包下载临时文件失败: {e}"))?;
    }

    emit_asset_download_progress(app, kind, "connecting", 0, None, None, None);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("创建素材包下载客户端失败: {e}"))?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|e| format!("下载素材包失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("下载素材包失败: {e}"))?;
    let total = response.content_length();
    let mut file =
        fs::File::create(&partial).map_err(|e| format!("创建素材包下载文件失败: {e}"))?;
    let mut buffer = [0_u8; 64 * 1024];
    let started = Instant::now();
    let mut downloaded = 0_u64;
    let mut last_emit = Instant::now();
    emit_asset_download_progress(app, kind, "downloading", 0, total, Some(0.0), None);

    loop {
        if let Err(err) = ensure_download_not_cancelled(cancel) {
            let _ = fs::remove_file(&partial);
            return Err(err);
        }
        let read = response
            .read(&mut buffer)
            .map_err(|e| format!("读取素材包下载流失败: {e}"))?;
        if read == 0 {
            break;
        }
        if let Err(err) = ensure_download_not_cancelled(cancel) {
            let _ = fs::remove_file(&partial);
            return Err(err);
        }
        file.write_all(&buffer[..read])
            .map_err(|e| format!("写入素材包下载文件失败: {e}"))?;
        downloaded += read as u64;

        let now = Instant::now();
        if now.duration_since(last_emit).as_millis() >= 250 || total == Some(downloaded) {
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            let bytes_per_second = downloaded as f64 / elapsed;
            let eta_seconds = total.and_then(|value| {
                if bytes_per_second > 0.0 && value > downloaded {
                    Some(((value - downloaded) as f64 / bytes_per_second).ceil() as u64)
                } else {
                    None
                }
            });
            emit_asset_download_progress(
                app,
                kind,
                "downloading",
                downloaded,
                total,
                Some(bytes_per_second),
                eta_seconds,
            );
            last_emit = now;
        }
    }

    file.flush()
        .map_err(|e| format!("写入素材包下载文件失败: {e}"))?;
    fs::rename(&partial, destination).map_err(|e| format!("保存素材包下载文件失败: {e}"))?;
    emit_asset_download_progress(app, kind, "downloaded", downloaded, total, None, None);
    Ok(())
}

pub(crate) fn install_asset_zip_merge(
    zip_path: &Path,
    assets_root: &Path,
) -> Result<FileCopyStats, String> {
    let temp = tempfile::Builder::new()
        .prefix("asset-update-")
        .tempdir_in(
            assets_root
                .parent()
                .ok_or_else(|| "无法定位素材根目录".to_string())?,
        )
        .map_err(|e| format!("创建临时目录失败: {e}"))?;
    let extracted_root = temp.path().join("unzipped");
    fs::create_dir_all(&extracted_root).map_err(|e| format!("创建临时目录失败: {e}"))?;
    extract_zip_archive(zip_path, &extracted_root)?;
    let import_root = locate_import_root(&extracted_root);
    let (_has_servants, _has_ces, servant_stats, ce_stats) =
        install_asset_directories(&import_root, assets_root, false)?;
    let mut stats = FileCopyStats::default();
    stats += servant_stats;
    stats += ce_stats;
    Ok(stats)
}

pub(crate) fn merge_asset_zip_into_root(
    zip_path: &Path,
    assets_root: &Path,
) -> Result<FileCopyStats, String> {
    let temp = tempfile::Builder::new()
        .prefix("asset-base-")
        .tempdir_in(
            assets_root
                .parent()
                .ok_or_else(|| "无法定位素材根目录".to_string())?,
        )
        .map_err(|e| format!("创建临时目录失败: {e}"))?;
    let extracted_root = temp.path().join("unzipped");
    fs::create_dir_all(&extracted_root).map_err(|e| format!("创建临时目录失败: {e}"))?;
    extract_zip_archive(zip_path, &extracted_root)?;
    let import_root = locate_import_root(&extracted_root);
    let (_has_servants, _has_ces, servant_stats, ce_stats) =
        install_asset_directories(&import_root, assets_root, false)?;
    let mut stats = FileCopyStats::default();
    stats += servant_stats;
    stats += ce_stats;
    Ok(stats)
}

pub(crate) fn install_asset_zip_base_replace(
    zip_paths: &[PathBuf],
    assets_root: &Path,
) -> Result<FileCopyStats, String> {
    let temp = tempfile::Builder::new()
        .prefix("asset-base-combined-")
        .tempdir_in(
            assets_root
                .parent()
                .ok_or_else(|| "无法定位素材根目录".to_string())?,
        )
        .map_err(|e| format!("创建临时目录失败: {e}"))?;
    let combined_root = temp.path().join("assets");
    fs::create_dir_all(&combined_root).map_err(|e| format!("创建临时目录失败: {e}"))?;

    for zip_path in zip_paths {
        merge_asset_zip_into_root(zip_path, &combined_root)?;
    }

    let (_has_servants, _has_ces, servant_stats, ce_stats) =
        install_asset_directories(&combined_root, assets_root, true)?;
    let mut stats = FileCopyStats::default();
    stats += servant_stats;
    stats += ce_stats;
    Ok(stats)
}

pub(crate) fn verify_asset_artifact(
    zip_path: &Path,
    expected_sha256: &str,
    expected_size: u64,
) -> Result<(), String> {
    let actual_size = fs::metadata(zip_path)
        .map_err(|e| format!("读取素材包文件信息失败: {e}"))?
        .len();
    if actual_size != expected_size {
        return Err(format!(
            "素材包大小不匹配，期望 {expected_size} bytes，实际 {actual_size} bytes"
        ));
    }
    let actual = sha256_file(zip_path)?;
    if actual.eq_ignore_ascii_case(expected_sha256) {
        Ok(())
    } else {
        Err(format!(
            "素材包 sha256 不匹配，期望 {expected_sha256}，实际 {actual}"
        ))
    }
}

pub(crate) fn download_and_install_asset_bundles_inner(
    app: tauri::AppHandle,
    force_base: bool,
    cancel: Arc<ResourceDownloadCancelState>,
) -> Result<AssetDownloadInstallResult, String> {
    let app_manifest = assets_app_manifest(&app)?;
    let assets_root = app_assets_dir(&app);
    cleanup_replaced_asset_trees(&assets_root);
    let local_status = asset_bundle_status_from_root(&assets_root, &app_manifest);
    let (_latest, remote, latest_base_url, _manifest_url) =
        fetch_assets_remote_manifest(&app_manifest.latest_url)?;
    let plan = asset_update_plan(
        local_status.current_version,
        local_status.imported_servants && local_status.imported_craft_essences,
        app_manifest.assets_version,
        &remote,
        force_base,
    );
    let downloads_dir = asset_downloads_dir(&assets_root);
    let target_version = plan.target_version();

    match &plan {
        AssetInstallPlan::None => {}
        AssetInstallPlan::Base { packs, patches, .. } => {
            for pack in packs {
                let url = join_remote_url(&latest_base_url, &pack.file);
                let filename = pack
                    .file
                    .rsplit('/')
                    .next()
                    .filter(|value| !value.is_empty())
                    .unwrap_or(pack.name.as_str());
                let zip_path = downloads_dir.join(filename);
                download_asset_artifact(&app, &pack.name, &url, &zip_path, &cancel.assets)?;
                verify_asset_artifact(&zip_path, &pack.sha256, pack.size)?;
                ensure_download_not_cancelled(&cancel.assets)?;
                emit_asset_download_progress(&app, &pack.name, "installing", 0, None, None, None);
                install_asset_zip_merge(&zip_path, &assets_root)?;
                emit_asset_download_progress(&app, &pack.name, "installed", 0, None, None, None);
            }
            for patch in patches {
                let kind = format!("patch-v{}-to-v{}", patch.from, patch.to);
                let url = join_remote_url(&latest_base_url, &patch.file);
                let filename = patch
                    .file
                    .rsplit('/')
                    .next()
                    .filter(|value| !value.is_empty())
                    .unwrap_or(kind.as_str());
                let zip_path = downloads_dir.join(filename);
                download_asset_artifact(&app, &kind, &url, &zip_path, &cancel.assets)?;
                verify_asset_artifact(&zip_path, &patch.sha256, patch.size)?;
                ensure_download_not_cancelled(&cancel.assets)?;
                emit_asset_download_progress(&app, &kind, "installing", 0, None, None, None);
                install_asset_zip_merge(&zip_path, &assets_root)?;
                write_asset_version(&assets_root, patch.to)?;
                emit_asset_download_progress(&app, &kind, "installed", 0, None, None, None);
            }
        }
        AssetInstallPlan::ForceBase { packs, patches, .. } => {
            let mut base_zip_paths = Vec::new();
            for pack in packs {
                let url = join_remote_url(&latest_base_url, &pack.file);
                let filename = pack
                    .file
                    .rsplit('/')
                    .next()
                    .filter(|value| !value.is_empty())
                    .unwrap_or(pack.name.as_str());
                let zip_path = downloads_dir.join(filename);
                download_asset_artifact(&app, &pack.name, &url, &zip_path, &cancel.assets)?;
                verify_asset_artifact(&zip_path, &pack.sha256, pack.size)?;
                ensure_download_not_cancelled(&cancel.assets)?;
                base_zip_paths.push(zip_path);
            }
            ensure_download_not_cancelled(&cancel.assets)?;
            emit_asset_download_progress(&app, "base", "installing", 0, None, None, None);
            install_asset_zip_base_replace(&base_zip_paths, &assets_root)?;
            emit_asset_download_progress(&app, "base", "installed", 0, None, None, None);

            for patch in patches {
                let kind = format!("patch-v{}-to-v{}", patch.from, patch.to);
                let url = join_remote_url(&latest_base_url, &patch.file);
                let filename = patch
                    .file
                    .rsplit('/')
                    .next()
                    .filter(|value| !value.is_empty())
                    .unwrap_or(kind.as_str());
                let zip_path = downloads_dir.join(filename);
                download_asset_artifact(&app, &kind, &url, &zip_path, &cancel.assets)?;
                verify_asset_artifact(&zip_path, &patch.sha256, patch.size)?;
                ensure_download_not_cancelled(&cancel.assets)?;
                emit_asset_download_progress(&app, &kind, "installing", 0, None, None, None);
                install_asset_zip_merge(&zip_path, &assets_root)?;
                write_asset_version(&assets_root, patch.to)?;
                emit_asset_download_progress(&app, &kind, "installed", 0, None, None, None);
            }
        }
        AssetInstallPlan::Patches { patches, .. } => {
            for patch in patches {
                let kind = format!("patch-v{}-to-v{}", patch.from, patch.to);
                let url = join_remote_url(&latest_base_url, &patch.file);
                let filename = patch
                    .file
                    .rsplit('/')
                    .next()
                    .filter(|value| !value.is_empty())
                    .unwrap_or(kind.as_str());
                let zip_path = downloads_dir.join(filename);
                download_asset_artifact(&app, &kind, &url, &zip_path, &cancel.assets)?;
                verify_asset_artifact(&zip_path, &patch.sha256, patch.size)?;
                ensure_download_not_cancelled(&cancel.assets)?;
                emit_asset_download_progress(&app, &kind, "installing", 0, None, None, None);
                install_asset_zip_merge(&zip_path, &assets_root)?;
                write_asset_version(&assets_root, patch.to)?;
                emit_asset_download_progress(&app, &kind, "installed", 0, None, None, None);
            }
        }
    }

    if let Some(version) = target_version {
        write_asset_version(&assets_root, version)?;
    }
    cleanup_replaced_asset_trees(&assets_root);
    refresh_asset_protocol_scope(&app)?;
    let final_status = asset_bundle_status_from_root(&assets_root, &app_manifest);
    Ok(AssetDownloadInstallResult {
        installed: !matches!(plan, AssetInstallPlan::None),
        installed_version: target_version,
        plan: plan.plan_type().to_string(),
        servant_files: final_status.servant_files,
        craft_essence_files: final_status.craft_essence_files,
        install_dir: assets_root.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub(crate) async fn download_asset_bundles(
    app: tauri::AppHandle,
    force_base: Option<bool>,
    cancel_state: tauri::State<'_, Arc<ResourceDownloadCancelState>>,
) -> Result<AssetDownloadInstallResult, String> {
    let cancel_state = cancel_state.inner().clone();
    cancel_state.assets.store(false, Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        download_and_install_asset_bundles_inner(app, force_base.unwrap_or(false), cancel_state)
    })
    .await
    .map_err(|e| format!("素材包下载任务失败: {e}"))?
}

#[tauri::command]
pub(crate) fn cancel_resource_downloads(
    cancel_state: tauri::State<'_, Arc<ResourceDownloadCancelState>>,
) -> Result<(), String> {
    cancel_state.runtime.store(true, Ordering::Relaxed);
    cancel_state.assets.store(true, Ordering::Relaxed);
    Ok(())
}
