//! Verified runtime bundle installation and version-record updates.

use super::{
    extract_runtime_zip, runtime_executable_path, runtime_version_dir, runtime_version_path,
    sha256_file, RuntimeInstallResult, RuntimeManifest, RuntimeVersionRecord,
};
use std::fs;
use std::path::Path;

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
    if !actual_sha.eq_ignore_ascii_case(&artifact.runtime_sha256) {
        return Err(format!(
            "runtime 压缩包 sha256 不匹配，期望 {}，实际 {}",
            artifact.runtime_sha256, actual_sha
        ));
    }
    fs::create_dir_all(runtime_root).map_err(|e| format!("创建 runtime 目录失败: {e}"))?;
    let version = artifact.runtime_version.as_str();
    let target = runtime_version_dir(runtime_root, version);
    let version_path = runtime_version_path(runtime_root);
    let staging = runtime_root.join(format!(
        ".install-runtime-{version}-{}",
        uuid::Uuid::new_v4()
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| format!("清理 runtime 临时目录失败: {e}"))?;
    }
    fs::create_dir_all(&staging).map_err(|e| format!("创建 runtime 临时目录失败: {e}"))?;

    let extract_result = extract_runtime_zip(zip_path, &staging);
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

    let exe = runtime_executable_path(runtime_root, &artifact.runtime_version);
    Ok(RuntimeInstallResult {
        installed_kind: "runtime".to_string(),
        installed_version: version.to_string(),
        platform: platform.to_string(),
        install_dir: target.to_string_lossy().into_owned(),
        executable_path: Some(exe.to_string_lossy().into_owned()),
    })
}
