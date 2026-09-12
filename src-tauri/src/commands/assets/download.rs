//! Remote asset download, verification, and installation commands.

use super::*;

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

#[tauri::command]
pub(crate) fn cancel_asset_operation(
    cancel_state: tauri::State<'_, Arc<ResourceDownloadCancelState>>,
) -> Result<(), String> {
    cancel_state.assets.store(true, Ordering::Relaxed);
    Ok(())
}
