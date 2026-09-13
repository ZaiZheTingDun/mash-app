//! Local ZIP import and shared asset-tree installation helpers.

use super::*;

#[derive(serde::Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetBundleImportResult {
    pub(crate) imported_servants: bool,
    pub(crate) imported_craft_essences: bool,
    pub(crate) servant_files: u64,
    pub(crate) craft_essence_files: u64,
    pub(crate) install_dir: String,
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

fn read_import_asset_version(extracted_root: &Path, import_root: &Path) -> Result<u32, String> {
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
        return Ok(record.version);
    }

    Err("文件不是素材包文件".to_string())
}

fn ensure_asset_import_not_cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("导入已取消".to_string())
    } else {
        Ok(())
    }
}

fn extract_zip_archive_inner(
    zip_path: &Path,
    destination: &Path,
    cancel: Option<&AtomicBool>,
) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("无法打开压缩包: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("无法读取 zip 压缩包: {e}"))?;

    for index in 0..archive.len() {
        if let Some(cancel) = cancel {
            ensure_asset_import_not_cancelled(cancel)?;
        }
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
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            if let Some(cancel) = cancel {
                ensure_asset_import_not_cancelled(cancel)?;
            }
            let read = entry
                .read(&mut buffer)
                .map_err(|e| format!("解压文件失败: {e}"))?;
            if read == 0 {
                break;
            }
            out.write_all(&buffer[..read])
                .map_err(|e| format!("解压文件失败: {e}"))?;
        }
        out.flush().map_err(|e| format!("写入解压文件失败: {e}"))?;
    }

    Ok(())
}

pub(crate) fn extract_zip_archive(zip_path: &Path, destination: &Path) -> Result<(), String> {
    extract_zip_archive_inner(zip_path, destination, None)
}

pub(crate) fn locate_import_root(extracted_root: &Path) -> PathBuf {
    let nested_assets = extracted_root.join("assets");
    if nested_assets.is_dir() {
        nested_assets
    } else {
        extracted_root.to_path_buf()
    }
}

fn copy_asset_tree(source: &Path, destination: &Path) -> Result<FileCopyStats, String> {
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

fn replace_asset_tree(source: &Path, destination: &Path) -> Result<FileCopyStats, String> {
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

#[cfg(test)]
pub(crate) fn import_asset_bundle_from_zip_path(
    zip_path: &Path,
    assets_root: &Path,
) -> Result<AssetBundleImportResult, String> {
    let cancel = AtomicBool::new(false);
    import_asset_bundle_from_zip_path_with_cancel(zip_path, assets_root, &cancel)
}

pub(crate) fn import_asset_bundle_from_zip_path_with_cancel(
    zip_path: &Path,
    assets_root: &Path,
    cancel: &AtomicBool,
) -> Result<AssetBundleImportResult, String> {
    cleanup_replaced_asset_trees(assets_root);
    ensure_asset_import_not_cancelled(cancel)?;
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
    extract_zip_archive_inner(zip_path, &extracted_root, Some(cancel))?;

    let import_root = locate_import_root(&extracted_root);
    let imported_version = read_import_asset_version(&extracted_root, &import_root)?;
    ensure_asset_import_not_cancelled(cancel)?;
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
    cancel_state: tauri::State<'_, Arc<ResourceDownloadCancelState>>,
) -> Result<AssetBundleImportResult, String> {
    let zip_path = PathBuf::from(zip_path);
    if !zip_path.is_file() {
        return Err("选择的压缩包不存在".to_string());
    }
    let assets_root = app_assets_dir(&app);
    let cancel_state = cancel_state.inner().clone();
    cancel_state.assets.store(false, Ordering::Relaxed);
    let result = tauri::async_runtime::spawn_blocking(move || {
        import_asset_bundle_from_zip_path_with_cancel(&zip_path, &assets_root, &cancel_state.assets)
    })
    .await
    .map_err(|e| format!("素材包导入任务失败: {e}"))??;
    refresh_asset_protocol_scope(&app)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_asset_directories_accepts_skills_without_servants_or_ces() {
        let temp = tempfile::tempdir().unwrap();
        let import_root = temp.path().join("import");
        let assets_root = temp.path().join("assets");
        fs::create_dir_all(import_root.join("skills").join("2477450")).unwrap();
        fs::write(
            import_root
                .join("skills")
                .join("2477450")
                .join("skill.json"),
            b"{}",
        )
        .unwrap();

        let (has_servants, has_ces, servant_stats, ce_stats) =
            install_asset_directories(&import_root, &assets_root, true).unwrap();

        assert!(!has_servants);
        assert!(!has_ces);
        assert_eq!(servant_stats, FileCopyStats::default());
        assert_eq!(ce_stats, FileCopyStats::default());
        assert!(assets_root
            .join("skills")
            .join("2477450")
            .join("skill.json")
            .is_file());
    }

    #[test]
    fn install_asset_directories_rejects_archives_without_known_asset_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let import_root = temp.path().join("import");
        let assets_root = temp.path().join("assets");
        fs::create_dir_all(import_root.join("other")).unwrap();

        let err = install_asset_directories(&import_root, &assets_root, true).unwrap_err();

        assert!(err.contains("servants/ces/icons/skills/mystic-codes"));
    }

    #[test]
    fn cleanup_replaced_asset_trees_removes_all_managed_asset_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let assets_root = temp.path();
        let stale_icons = assets_root.join("icons.replaced-123");
        let stale_skills = assets_root.join("skills.replaced-234");
        let stale_codes = assets_root.join("mystic-codes.replaced-456");
        let keep = assets_root.join("other.replaced-789");
        fs::create_dir_all(&stale_icons).unwrap();
        fs::create_dir_all(&stale_skills).unwrap();
        fs::create_dir_all(&stale_codes).unwrap();
        fs::create_dir_all(&keep).unwrap();

        cleanup_replaced_asset_trees(assets_root);

        assert!(!stale_icons.exists());
        assert!(!stale_skills.exists());
        assert!(!stale_codes.exists());
        assert!(keep.exists());
    }
}
