//! CV runtime bundle installation, status, and download commands.
//! Runtime artifacts remain versioned separately from app assets and project data.

use super::*;

pub(crate) const RUNTIME_MANIFEST_JSON: &str =
    include_str!("../../resources/runtime-manifest.json");
pub(crate) const RUNTIME_DIR_NAME: &str = "mash-cv";
pub(crate) const RUNTIME_DOWNLOAD_PROGRESS_EVENT: &str = "runtime-download-progress";

#[derive(Default)]
pub(crate) struct ResourceDownloadCancelState {
    pub(crate) runtime: AtomicBool,
    pub(crate) assets: AtomicBool,
}

pub(crate) fn cancelled_download_error() -> String {
    "下载已取消".to_string()
}

pub(crate) fn ensure_download_not_cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err(cancelled_download_error())
    } else {
        Ok(())
    }
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeManifest {
    pub(crate) mash_cv_runtime_version: String,
    pub(crate) mash_cv_code_version: String,
    pub(crate) platforms: HashMap<String, RuntimePlatformArtifact>,
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimePlatformArtifact {
    pub(crate) runtime_url: String,
    pub(crate) runtime_sha256: String,
    pub(crate) code_url: String,
    pub(crate) code_sha256: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeVersionRecord {
    pub(crate) version: String,
    pub(crate) platform: String,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeStatus {
    pub(crate) required_runtime_version: String,
    pub(crate) installed_runtime_version: Option<String>,
    pub(crate) runtime_installed: bool,
    pub(crate) required_code_version: String,
    pub(crate) installed_code_version: Option<String>,
    pub(crate) code_installed: bool,
    pub(crate) installed: bool,
    pub(crate) platform: String,
    pub(crate) runtime_download_url: Option<String>,
    pub(crate) runtime_expected_sha256: Option<String>,
    pub(crate) runtime_install_dir: String,
    pub(crate) executable_path: String,
    pub(crate) code_download_url: Option<String>,
    pub(crate) code_expected_sha256: Option<String>,
    pub(crate) code_install_dir: String,
    pub(crate) code_path: String,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeInstallResult {
    pub(crate) installed_kind: String,
    pub(crate) installed_version: String,
    pub(crate) platform: String,
    pub(crate) install_dir: String,
    pub(crate) executable_path: Option<String>,
    pub(crate) code_path: Option<String>,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeDownloadProgress {
    pub(crate) kind: String,
    pub(crate) phase: String,
    pub(crate) downloaded_bytes: u64,
    pub(crate) total_bytes: Option<u64>,
    pub(crate) bytes_per_second: Option<f64>,
    pub(crate) eta_seconds: Option<u64>,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeDownloadInstallResult {
    pub(crate) installed: Vec<RuntimeInstallResult>,
}

pub(crate) fn runtime_platform_key() -> String {
    runtime_platform_key_from(std::env::consts::OS, std::env::consts::ARCH)
}

pub(crate) fn runtime_platform_key_from(os: &str, arch: &str) -> String {
    let os = match os {
        "macos" => "darwin",
        other => other,
    };
    format!("{os}-{arch}")
}

pub(crate) fn runtime_exe_name() -> &'static str {
    if cfg!(windows) {
        "mash-cv.exe"
    } else {
        "mash-cv"
    }
}

pub(crate) fn runtime_bundle_root() -> &'static str {
    "mash-cv-runtime"
}

pub(crate) fn code_bundle_root() -> &'static str {
    "mash-cv-code"
}

pub(crate) fn parse_runtime_manifest(contents: &str) -> Result<RuntimeManifest, String> {
    serde_json::from_str(contents).map_err(|e| format!("解析 runtime manifest 失败: {e}"))
}

pub(crate) fn runtime_manifest(app: &tauri::AppHandle) -> Result<RuntimeManifest, String> {
    let contents = app
        .path()
        .resource_dir()
        .ok()
        .map(|base| base.join("resources").join("runtime-manifest.json"))
        .and_then(|path| fs::read_to_string(path).ok())
        .unwrap_or_else(|| RUNTIME_MANIFEST_JSON.to_string());
    parse_runtime_manifest(&contents)
}

pub(crate) fn runtime_root_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app_data_dir(app).join("runtime").join(RUNTIME_DIR_NAME);
    fs::create_dir_all(&dir).ok();
    dir
}

pub(crate) fn runtime_base_root(runtime_root: &Path) -> PathBuf {
    runtime_root.join("runtime")
}

pub(crate) fn runtime_code_root(runtime_root: &Path) -> PathBuf {
    runtime_root.join("code")
}

pub(crate) fn runtime_version_path(runtime_root: &Path) -> PathBuf {
    runtime_root.join("runtime").join("runtime-version.json")
}

pub(crate) fn code_version_path(runtime_root: &Path) -> PathBuf {
    runtime_root.join("code").join("code-version.json")
}

pub(crate) fn runtime_version_dir(runtime_root: &Path, version: &str) -> PathBuf {
    runtime_base_root(runtime_root).join(version)
}

pub(crate) fn code_version_dir(runtime_root: &Path, version: &str) -> PathBuf {
    runtime_code_root(runtime_root).join(version)
}

pub(crate) fn runtime_executable_path(runtime_root: &Path, version: &str) -> PathBuf {
    runtime_version_dir(runtime_root, version)
        .join(runtime_bundle_root())
        .join(runtime_exe_name())
}

pub(crate) fn read_runtime_version(runtime_root: &Path) -> Option<RuntimeVersionRecord> {
    fs::read_to_string(runtime_version_path(runtime_root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub(crate) fn read_code_version(runtime_root: &Path) -> Option<RuntimeVersionRecord> {
    fs::read_to_string(code_version_path(runtime_root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub(crate) fn runtime_code_path(runtime_root: &Path, version: &str) -> PathBuf {
    code_version_dir(runtime_root, version).join(code_bundle_root())
}

pub(crate) fn runtime_models_path(runtime_root: &Path, version: &str) -> PathBuf {
    runtime_version_dir(runtime_root, version)
        .join(runtime_bundle_root())
        .join("_internal")
        .join("mash_cv")
        .join("models")
}

pub(crate) fn runtime_status_from_manifest(
    manifest: &RuntimeManifest,
    runtime_root: &Path,
    platform: &str,
) -> RuntimeStatus {
    let installed_runtime_version = read_runtime_version(runtime_root).map(|record| record.version);
    let installed_code_version = read_code_version(runtime_root).map(|record| record.version);
    let executable = runtime_executable_path(runtime_root, &manifest.mash_cv_runtime_version);
    let code_path = runtime_code_path(runtime_root, &manifest.mash_cv_code_version);
    let artifact = manifest.platforms.get(platform);
    let runtime_installed = installed_runtime_version.as_deref()
        == Some(manifest.mash_cv_runtime_version.as_str())
        && executable.is_file();
    let code_installed = installed_code_version.as_deref()
        == Some(manifest.mash_cv_code_version.as_str())
        && code_path.join("mash_cv").is_dir();

    RuntimeStatus {
        required_runtime_version: manifest.mash_cv_runtime_version.clone(),
        installed_runtime_version,
        runtime_installed,
        required_code_version: manifest.mash_cv_code_version.clone(),
        installed_code_version,
        code_installed,
        installed: runtime_installed && code_installed,
        platform: platform.to_string(),
        runtime_download_url: artifact.map(|item| item.runtime_url.clone()),
        runtime_expected_sha256: artifact.map(|item| item.runtime_sha256.clone()),
        runtime_install_dir: runtime_version_dir(runtime_root, &manifest.mash_cv_runtime_version)
            .to_string_lossy()
            .into_owned(),
        executable_path: executable.to_string_lossy().into_owned(),
        code_download_url: artifact.map(|item| item.code_url.clone()),
        code_expected_sha256: artifact.map(|item| item.code_sha256.clone()),
        code_install_dir: code_version_dir(runtime_root, &manifest.mash_cv_code_version)
            .to_string_lossy()
            .into_owned(),
        code_path: code_path.to_string_lossy().into_owned(),
    }
}

pub(crate) fn runtime_status_for_app(app: &tauri::AppHandle) -> Result<RuntimeStatus, String> {
    let manifest = runtime_manifest(app)?;
    let platform = runtime_platform_key();
    let runtime_root = runtime_root_dir(app);
    Ok(runtime_status_from_manifest(
        &manifest,
        &runtime_root,
        &platform,
    ))
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("无法打开 runtime 压缩包: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| format!("读取 runtime 压缩包失败: {e}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn zip_has_valid_root(relative: &Path, root: &str) -> bool {
    relative.components().next().and_then(|part| match part {
        std::path::Component::Normal(name) => name.to_str(),
        _ => None,
    }) == Some(root)
}

pub(crate) fn zip_entry_is_unix_symlink(entry: &zip::read::ZipFile<'_>) -> bool {
    entry
        .unix_mode()
        .is_some_and(|mode| (mode & 0o170000) == 0o120000)
}

pub(crate) fn relative_target_stays_within_root(
    root: &Path,
    link_parent: &Path,
    target: &Path,
) -> bool {
    if target.is_absolute() {
        return false;
    }

    let Ok(stripped_parent) = link_parent.strip_prefix(root) else {
        return false;
    };

    let mut parts: Vec<std::ffi::OsString> = stripped_parent
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name.to_os_string()),
            _ => None,
        })
        .collect();

    for component in target.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(name) => parts.push(name.to_os_string()),
            std::path::Component::ParentDir => {
                if parts.pop().is_none() {
                    return false;
                }
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => return false,
        }
    }

    true
}

#[cfg(unix)]
pub(crate) fn extract_zip_symlink(
    entry: &mut zip::read::ZipFile<'_>,
    output: &Path,
    destination: &Path,
) -> Result<(), String> {
    use std::os::unix::fs::symlink;

    let mut target = String::new();
    entry
        .read_to_string(&mut target)
        .map_err(|e| format!("读取 runtime 符号链接失败: {e}"))?;
    let target = target.trim_end_matches('\0').trim();
    if target.is_empty() {
        return Err("runtime zip 内包含空符号链接目标".to_string());
    }
    let target_path = Path::new(target);
    let Some(parent) = output.parent() else {
        return Err("runtime 符号链接缺少父目录".to_string());
    };
    if !relative_target_stays_within_root(destination, parent, target_path) {
        return Err("runtime zip 内包含越界符号链接".to_string());
    }
    symlink(target_path, output).map_err(|e| format!("创建 runtime 符号链接失败: {e}"))
}

#[cfg(not(unix))]
pub(crate) fn extract_zip_symlink(
    entry: &mut zip::read::ZipFile<'_>,
    output: &Path,
    _destination: &Path,
) -> Result<(), String> {
    let mut out = fs::File::create(output).map_err(|e| format!("写入 runtime 文件失败: {e}"))?;
    io::copy(entry, &mut out).map_err(|e| format!("解压 runtime 文件失败: {e}"))?;
    out.flush()
        .map_err(|e| format!("写入 runtime 文件失败: {e}"))?;
    Ok(())
}

#[cfg(unix)]
pub(crate) fn make_runtime_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|e| format!("读取 runtime 可执行权限失败: {e}"))?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    fs::set_permissions(path, permissions).map_err(|e| format!("设置 runtime 可执行权限失败: {e}"))
}

#[cfg(not(unix))]
pub(crate) fn make_runtime_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub(crate) fn extract_zip_with_root(
    zip_path: &Path,
    destination: &Path,
    root: &str,
) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("无法打开 runtime 压缩包: {e}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("无法读取 runtime zip 压缩包: {e}"))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("读取 runtime zip 条目失败: {e}"))?;
        let Some(relative) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            return Err("runtime zip 内包含非法路径".to_string());
        };
        if !zip_has_valid_root(&relative, root) {
            return Err(format!("runtime zip 必须以 {root}/ 作为根目录"));
        }

        let output = destination.join(relative);
        if entry.name().ends_with('/') {
            fs::create_dir_all(&output).map_err(|e| format!("创建 runtime 目录失败: {e}"))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建 runtime 目录失败: {e}"))?;
        }
        if zip_entry_is_unix_symlink(&entry) {
            extract_zip_symlink(&mut entry, &output, destination)?;
            continue;
        }
        let mut out =
            fs::File::create(&output).map_err(|e| format!("写入 runtime 文件失败: {e}"))?;
        io::copy(&mut entry, &mut out).map_err(|e| format!("解压 runtime 文件失败: {e}"))?;
        out.flush()
            .map_err(|e| format!("写入 runtime 文件失败: {e}"))?;
    }

    Ok(())
}

pub(crate) fn extract_runtime_zip(zip_path: &Path, destination: &Path) -> Result<(), String> {
    extract_zip_with_root(zip_path, destination, runtime_bundle_root())?;
    let exe = destination
        .join(runtime_bundle_root())
        .join(runtime_exe_name());
    if !exe.is_file() {
        return Err(format!(
            "runtime zip 内未找到 {}/{}",
            runtime_bundle_root(),
            runtime_exe_name()
        ));
    }
    make_runtime_executable(&exe)?;
    Ok(())
}

pub(crate) fn extract_code_zip(zip_path: &Path, destination: &Path) -> Result<(), String> {
    extract_zip_with_root(zip_path, destination, code_bundle_root())?;
    let package = destination.join(code_bundle_root()).join("mash_cv");
    if !package.is_dir() {
        return Err(format!("code zip 内未找到 {}/mash_cv/", code_bundle_root()));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeBundleKind {
    Runtime,
    Code,
}

pub(crate) fn import_runtime_bundle_from_zip_path(
    zip_path: &Path,
    manifest: &RuntimeManifest,
    runtime_root: &Path,
    platform: &str,
) -> Result<RuntimeInstallResult, String> {
    if !zip_path.is_file() {
        return Err("选择的 runtime 压缩包不存在".to_string());
    }
    let artifact = manifest
        .platforms
        .get(platform)
        .ok_or_else(|| format!("当前平台不支持独立 CV runtime: {platform}"))?;
    let actual_sha = sha256_file(zip_path)?;
    let kind = if actual_sha.eq_ignore_ascii_case(&artifact.runtime_sha256) {
        RuntimeBundleKind::Runtime
    } else if actual_sha.eq_ignore_ascii_case(&artifact.code_sha256) {
        RuntimeBundleKind::Code
    } else {
        return Err(format!(
            "runtime 压缩包 sha256 不匹配，期望 runtime={} 或 code={}，实际 {}",
            artifact.runtime_sha256, artifact.code_sha256, actual_sha
        ));
    };

    fs::create_dir_all(runtime_root).map_err(|e| format!("创建 runtime 目录失败: {e}"))?;
    let (kind_name, version, target, version_path) = match kind {
        RuntimeBundleKind::Runtime => (
            "runtime",
            manifest.mash_cv_runtime_version.as_str(),
            runtime_version_dir(runtime_root, &manifest.mash_cv_runtime_version),
            runtime_version_path(runtime_root),
        ),
        RuntimeBundleKind::Code => (
            "code",
            manifest.mash_cv_code_version.as_str(),
            code_version_dir(runtime_root, &manifest.mash_cv_code_version),
            code_version_path(runtime_root),
        ),
    };
    let staging = runtime_root.join(format!(
        ".install-{kind_name}-{version}-{}",
        uuid::Uuid::new_v4()
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| format!("清理 runtime 临时目录失败: {e}"))?;
    }
    fs::create_dir_all(&staging).map_err(|e| format!("创建 runtime 临时目录失败: {e}"))?;

    let extract_result = match kind {
        RuntimeBundleKind::Runtime => extract_runtime_zip(zip_path, &staging),
        RuntimeBundleKind::Code => extract_code_zip(zip_path, &staging),
    };
    if let Err(err) = extract_result {
        fs::remove_dir_all(&staging).ok();
        return Err(err);
    }

    if target.exists() {
        fs::remove_dir_all(&target).map_err(|e| format!("替换旧 runtime 失败: {e}"))?;
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 runtime 目录失败: {e}"))?;
    }
    fs::rename(&staging, &target).map_err(|e| format!("安装 runtime 失败: {e}"))?;

    let record = RuntimeVersionRecord {
        version: version.to_string(),
        platform: platform.to_string(),
    };
    fs::write(
        version_path,
        serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入 runtime 版本记录失败: {e}"))?;

    let exe = runtime_executable_path(runtime_root, &manifest.mash_cv_runtime_version);
    let code_path = runtime_code_path(runtime_root, &manifest.mash_cv_code_version);
    Ok(RuntimeInstallResult {
        installed_kind: kind_name.to_string(),
        installed_version: version.to_string(),
        platform: platform.to_string(),
        install_dir: target.to_string_lossy().into_owned(),
        executable_path: matches!(kind, RuntimeBundleKind::Runtime)
            .then(|| exe.to_string_lossy().into_owned()),
        code_path: matches!(kind, RuntimeBundleKind::Code)
            .then(|| code_path.to_string_lossy().into_owned()),
    })
}

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

    if !status.runtime_installed {
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

    if !status.code_installed {
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

#[tauri::command]
pub(crate) fn get_runtime_status(app: tauri::AppHandle) -> Result<RuntimeStatus, String> {
    runtime_status_for_app(&app)
}

#[tauri::command]
pub(crate) async fn pick_runtime_bundle(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("ZIP", &["zip"])
        .set_title("选择 CV runtime 压缩包")
        .blocking_pick_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub(crate) async fn import_runtime_bundle(
    app: tauri::AppHandle,
    zip_path: String,
) -> Result<RuntimeInstallResult, String> {
    let manifest = runtime_manifest(&app)?;
    let platform = runtime_platform_key();
    import_runtime_bundle_from_zip_path(
        &PathBuf::from(zip_path),
        &manifest,
        &runtime_root_dir(&app),
        &platform,
    )
}

#[tauri::command]
pub(crate) async fn download_runtime_bundles(
    app: tauri::AppHandle,
    cancel_state: tauri::State<'_, Arc<ResourceDownloadCancelState>>,
) -> Result<RuntimeDownloadInstallResult, String> {
    let cancel_state = cancel_state.inner().clone();
    cancel_state.runtime.store(false, Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        download_and_install_runtime_bundles_inner(app, cancel_state)
    })
    .await
    .map_err(|e| format!("runtime 下载任务失败: {e}"))?
}

// ---------------------------------------------------------------------------
// Shared resource-path resolvers (used by both automation + debug paths)
// ---------------------------------------------------------------------------

/// Resolve the bundled templates directory for the given server. Each
/// server (JP/CN) ships its own subtree under
/// `resources/servers/{token}/templates/` so flipping the global server
/// setting hands the sidecar a different template set without touching
/// any JP fixture.
pub(crate) fn resolve_templates_dir(app: &tauri::AppHandle, server: Server) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("servers")
            .join(server.dir_token())
            .join("templates");
        if dev.is_dir() {
            return Some(dev);
        }
    }

    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("servers")
            .join(server.dir_token())
            .join("templates"),
    )
}

/// Resolve shared templates that are loaded before server-specific templates.
pub(crate) fn resolve_shared_templates_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("servers")
            .join("shared")
            .join("templates");
        if dev.is_dir() {
            return Some(dev);
        }
    }

    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("servers")
            .join("shared")
            .join("templates"),
    )
}

pub(crate) fn resolve_template_dirs(
    app: &tauri::AppHandle,
    server: Server,
) -> Vec<screen::TemplateLoadSpec> {
    let mut dirs = Vec::new();
    if let Some(shared) = resolve_shared_templates_dir(app) {
        dirs.push(screen::TemplateLoadSpec {
            dir: shared,
            key_prefix: Some("shared".to_string()),
        });
    }
    if let Some(server_dir) = resolve_templates_dir(app, server) {
        dirs.push(screen::TemplateLoadSpec {
            dir: server_dir,
            key_prefix: None,
        });
    }
    dirs
}

/// Resolve the bundled cv.json path for the given server.
pub(crate) fn resolve_cv_config_path(app: &tauri::AppHandle, server: Server) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("servers")
            .join(server.dir_token())
            .join("cv.json");
        if dev.is_file() {
            return Some(dev);
        }
    }

    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("servers")
            .join(server.dir_token())
            .join("cv.json"),
    )
}

pub(crate) fn resolve_shared_cv_config_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("servers")
            .join("shared")
            .join("cv.json");
        if dev.is_file() {
            return Some(dev);
        }
    }

    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("servers")
            .join("shared")
            .join("cv.json"),
    )
}

pub(crate) fn resolve_cv_config_paths(app: &tauri::AppHandle, server: Server) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(shared) = resolve_shared_cv_config_path(app) {
        paths.push(shared);
    }
    if let Some(server_path) = resolve_cv_config_path(app, server) {
        paths.push(server_path);
    }
    paths
}

/// Resolve the bundled scrcpy-server.jar path.
pub(crate) fn resolve_scrcpy_jar(app: &tauri::AppHandle) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("scrcpy")
            .join("scrcpy-server.jar"),
    )
}

/// Resolve the per-servant assets directory (containing
/// `{servant_id}/card_servant_*.png`). This is the `servants/` subtree
/// of the broader `assets/` tree (which also holds `ces/` for craft
/// essences). The dir is intentionally NOT bundled into the app yet
/// (production bundling is a future decision); in dev we read it
/// directly from the source tree.
///
/// Lookup order:
/// 1. `<resource_dir>/assets/servants/` — present once the user opts to bundle it.
/// 2. `<CARGO_MANIFEST_DIR>/assets/servants/` — the dev-time source location.
///
/// Returns ``None`` if neither exists; callers should treat that as
/// "no per-servant identification available" rather than an error.
pub(crate) fn resolve_servant_assets_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let imported = app_assets_dir(app).join("servants");
    if imported.is_dir() {
        return Some(imported);
    }
    if let Ok(base) = app.path().resource_dir() {
        let bundled = base.join("assets").join("servants");
        if bundled.is_dir() {
            return Some(bundled);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("servants");
    if dev.is_dir() {
        return Some(dev);
    }
    None
}

/// Resolve the per-craft-essence assets directory (containing
/// `{ce_id}/card_ce.png`). Mirrors `resolve_servant_assets_dir` — the
/// runner uses these templates to verify support rows on the fly, so we
/// look up bundled assets first, then fall back to the dev-time source
/// tree. Returns `None` when neither path exists; callers fall back to
/// the legacy behaviour (pick the first OCR match) in that case.
pub(crate) fn resolve_ce_assets_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let imported = app_assets_dir(app).join("ces");
    if imported.is_dir() {
        return Some(imported);
    }
    if let Ok(base) = app.path().resource_dir() {
        let bundled = base.join("assets").join("ces");
        if bundled.is_dir() {
            return Some(bundled);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("ces");
    if dev.is_dir() {
        return Some(dev);
    }
    None
}

/// Resolve the installed mash-cv runtime executable path. The sidecar is no
/// longer bundled inside the app; users install the PyInstaller --onedir zip
/// under `app_data_dir()/runtime/mash-cv/<version>/mash-cv/`.
pub(crate) fn resolve_sidecar_exe(app: &tauri::AppHandle) -> Option<PathBuf> {
    let manifest = runtime_manifest(app).ok()?;
    Some(runtime_executable_path(
        &runtime_root_dir(app),
        &manifest.mash_cv_runtime_version,
    ))
}

pub(crate) fn resolve_sidecar_code_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|root| root.join("sidecar").join("mash_cv"));
        if let Some(dev) = dev {
            if dev.join("mash_cv").is_dir() {
                return Some(dev);
            }
        }
    }

    let manifest = runtime_manifest(app).ok()?;
    Some(runtime_code_path(
        &runtime_root_dir(app),
        &manifest.mash_cv_code_version,
    ))
}

pub(crate) fn resolve_sidecar_models_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let manifest = runtime_manifest(app).ok()?;
    Some(runtime_models_path(
        &runtime_root_dir(app),
        &manifest.mash_cv_runtime_version,
    ))
}
