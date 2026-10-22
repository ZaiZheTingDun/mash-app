//! Hash allowlist for manual imports. Online installs use the remote manifest separately.

use super::*;
use sha2::{Digest, Sha256};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum VerifiedManualRelease {
    Base(u32),
    Patch { from: u32, to: u32 },
}

impl VerifiedManualRelease {
    pub(super) fn verify_metadata(&self, actual: Self) -> Result<(), String> {
        if *self == actual {
            Ok(())
        } else {
            Err("手动素材包的版本信息与校验配置不一致".to_string())
        }
    }
}

// Hash the exact snapshot that will be extracted, so changing the selected file
// during import cannot substitute unverified contents after the hash check.
pub(super) fn stage_verified_archive(
    source: &Path,
    snapshot: &Path,
    manifest: &ManualAssetImportManifest,
    cancel: &AtomicBool,
) -> Result<VerifiedManualRelease, String> {
    ensure_asset_import_not_cancelled(cancel)?;
    let mut input = fs::File::open(source).map_err(|e| format!("无法打开素材压缩包: {e}"))?;
    let mut output =
        fs::File::create(snapshot).map_err(|e| format!("创建素材校验临时文件失败: {e}"))?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        ensure_asset_import_not_cancelled(cancel)?;
        let read = input
            .read(&mut buffer)
            .map_err(|e| format!("读取素材压缩包失败: {e}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|e| format!("暂存素材压缩包失败: {e}"))?;
        size += read as u64;
    }
    output
        .flush()
        .map_err(|e| format!("暂存素材压缩包失败: {e}"))?;
    let actual = format!("{:x}", hasher.finalize());
    if let Some(base) = manifest
        .bases
        .iter()
        .find(|base| base.sha256.eq_ignore_ascii_case(&actual))
    {
        return Ok(VerifiedManualRelease::Base(base.version));
    }
    if let Some(patch) = manifest
        .patches
        .iter()
        .find(|patch| patch.sha256.eq_ignore_ascii_case(&actual))
    {
        if patch.size != size {
            return Err(format!(
                "升级包大小与校验配置不一致，期望 {} bytes，实际 {size} bytes",
                patch.size
            ));
        }
        return Ok(VerifiedManualRelease::Patch {
            from: patch.from,
            to: patch.to,
        });
    }
    let name = source.file_name().unwrap_or_default().to_string_lossy();
    Err(format!("手动素材包 SHA-256 校验失败：{name} 未匹配允许导入的完整包或升级包，实际 SHA-256: {actual}"))
}

#[cfg(test)]
pub(crate) fn fixture_manual_import_manifest(paths: &[PathBuf]) -> ManualAssetImportManifest {
    // Installer tests explicitly trust their generated fixtures; validation tests
    // supply an independent allowlist to exercise tampering and unknown packages.
    fn record(archive: &mut ZipArchive<fs::File>, names: &[&str]) -> Option<serde_json::Value> {
        for name in names {
            if let Ok(mut entry) = archive.by_name(name) {
                let mut raw = String::new();
                entry.read_to_string(&mut raw).unwrap();
                return serde_json::from_str(&raw).ok();
            }
        }
        None
    }
    let mut manifest = ManualAssetImportManifest::default();
    for path in paths {
        let mut archive = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        let sha256 = sha256_file(path).unwrap();
        if let Some(patch) = record(
            &mut archive,
            &["assets/patch-manifest.json", "patch-manifest.json"],
        ) {
            manifest.patches.push(AssetsPatchRelease {
                from: patch["fromVersion"].as_u64().unwrap_or(0) as u32,
                to: patch["toVersion"].as_u64().unwrap_or(0) as u32,
                sha256,
                size: fs::metadata(path).unwrap().len(),
                file: path.to_string_lossy().into_owned(),
            });
        } else {
            let version = record(
                &mut archive,
                &["assets/assets-version.json", "assets-version.json"],
            )
            .and_then(|record| record["version"].as_u64())
            .unwrap_or(0) as u32;
            manifest
                .bases
                .push(ManualAssetBaseRelease { version, sha256 });
        }
    }
    manifest
}

#[cfg(test)]
mod tests;
