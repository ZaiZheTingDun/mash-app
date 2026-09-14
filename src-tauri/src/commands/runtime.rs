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
    force: Option<bool>,
) -> Result<RuntimeDownloadInstallResult, String> {
    let cancel_state = cancel_state.inner().clone();
    cancel_state.runtime.store(false, Ordering::Relaxed);
    let force = force.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        download_and_install_runtime_bundles_inner(app, cancel_state, force)
    })
    .await
    .map_err(|e| format!("runtime 下载任务失败: {e}"))?
}

mod archive;
pub(crate) use archive::*;
mod download;
pub(crate) use download::*;
mod resolution;
pub(crate) use resolution::*;
