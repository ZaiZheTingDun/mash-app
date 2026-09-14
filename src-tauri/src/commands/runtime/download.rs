//! Runtime artifact download, progress reporting, and install orchestration.

use super::{
    ensure_download_not_cancelled, import_runtime_bundle_from_zip_path, runtime_manifest,
    runtime_platform_key, runtime_root_dir, runtime_status_from_manifest,
    ResourceDownloadCancelState, RuntimeDownloadInstallResult, RuntimeDownloadProgress,
    RUNTIME_DOWNLOAD_PROGRESS_EVENT,
};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tauri::Emitter;

pub(crate) fn runtime_downloads_dir(runtime_root: &Path) -> PathBuf {
    runtime_root.join(".downloads")
}

pub(crate) fn emit_runtime_download_progress(
    app: &tauri::AppHandle,
    kind: &str,
    phase: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    bytes_per_second: Option<f64>,
    eta_seconds: Option<u64>,
) {
    app.emit(
        RUNTIME_DOWNLOAD_PROGRESS_EVENT,
        RuntimeDownloadProgress {
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

pub(crate) fn download_runtime_artifact(
    app: &tauri::AppHandle,
    kind: &str,
    url: &str,
    destination: &Path,
    cancel: &AtomicBool,
) -> Result<(), String> {
    ensure_download_not_cancelled(cancel)?;
    let parent = destination
        .parent()
        .ok_or_else(|| "runtime 下载路径缺少父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("创建 runtime 下载目录失败: {e}"))?;
    let partial = destination.with_extension("zip.part");
    if partial.exists() {
        fs::remove_file(&partial).map_err(|e| format!("清理 runtime 下载临时文件失败: {e}"))?;
    }

    emit_runtime_download_progress(app, kind, "connecting", 0, None, None, None);
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|e| format!("创建 runtime 下载客户端失败: {e}"))?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|e| format!("下载 runtime 包失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("下载 runtime 包失败: {e}"))?;
    let total = response.content_length();
    let mut file =
        fs::File::create(&partial).map_err(|e| format!("创建 runtime 下载文件失败: {e}"))?;
    let mut buffer = [0_u8; 64 * 1024];
    let started = Instant::now();
    let mut downloaded = 0_u64;
    let mut last_emit = Instant::now();
    emit_runtime_download_progress(app, kind, "downloading", 0, total, Some(0.0), None);

    loop {
        if let Err(err) = ensure_download_not_cancelled(cancel) {
            let _ = fs::remove_file(&partial);
            return Err(err);
        }
        let read = response
            .read(&mut buffer)
            .map_err(|e| format!("读取 runtime 下载流失败: {e}"))?;
        if read == 0 {
            break;
        }
        if let Err(err) = ensure_download_not_cancelled(cancel) {
            let _ = fs::remove_file(&partial);
            return Err(err);
        }
        file.write_all(&buffer[..read])
            .map_err(|e| format!("写入 runtime 下载文件失败: {e}"))?;
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
            emit_runtime_download_progress(
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
        .map_err(|e| format!("写入 runtime 下载文件失败: {e}"))?;
    fs::rename(&partial, destination).map_err(|e| format!("保存 runtime 下载文件失败: {e}"))?;
    emit_runtime_download_progress(app, kind, "downloaded", downloaded, total, None, None);
    Ok(())
}

pub(crate) fn download_and_install_runtime_bundles_inner(
    app: tauri::AppHandle,
    cancel: Arc<ResourceDownloadCancelState>,
    force: bool,
) -> Result<RuntimeDownloadInstallResult, String> {
    let manifest = runtime_manifest(&app)?;
    let platform = runtime_platform_key();
    let runtime_root = runtime_root_dir(&app);
    let status = runtime_status_from_manifest(&manifest, &runtime_root, &platform);
    let artifact = manifest
        .platforms
        .get(&platform)
        .ok_or_else(|| format!("当前平台不支持独立 CV runtime: {platform}"))?;
    let downloads_dir = runtime_downloads_dir(&runtime_root);
    let mut installed = Vec::new();

    if force || !status.runtime_installed {
        let filename = format!(
            "mash-cv-runtime-{platform}-v{}.zip",
            manifest.mash_cv_runtime_version
        );
        let zip_path = downloads_dir.join(filename);
        download_runtime_artifact(
            &app,
            "runtime",
            &artifact.runtime_url,
            &zip_path,
            &cancel.runtime,
        )?;
        ensure_download_not_cancelled(&cancel.runtime)?;
        emit_runtime_download_progress(&app, "runtime", "installing", 0, None, None, None);
        installed.push(import_runtime_bundle_from_zip_path(
            &zip_path,
            &manifest,
            &runtime_root,
            &platform,
        )?);
        emit_runtime_download_progress(&app, "runtime", "installed", 0, None, None, None);
    }

    if force || !status.code_installed {
        let filename = format!("mash-cv-code-v{}.zip", manifest.mash_cv_code_version);
        let zip_path = downloads_dir.join(filename);
        download_runtime_artifact(&app, "code", &artifact.code_url, &zip_path, &cancel.runtime)?;
        ensure_download_not_cancelled(&cancel.runtime)?;
        emit_runtime_download_progress(&app, "code", "installing", 0, None, None, None);
        installed.push(import_runtime_bundle_from_zip_path(
            &zip_path,
            &manifest,
            &runtime_root,
            &platform,
        )?);
        emit_runtime_download_progress(&app, "code", "installed", 0, None, None, None);
    }

    Ok(RuntimeDownloadInstallResult { installed })
}
