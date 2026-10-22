//! Offline mash-data patches: validate the entire chain before changing installed files.

use super::*;
use std::collections::BTreeMap;
use std::path::Component;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchManifest {
    from_version: u32,
    to_version: u32,
}

#[derive(serde::Deserialize)]
struct PatchDeletes {
    files: Vec<String>,
}

pub(super) struct PreparedPatch {
    root: PathBuf,
    manifest: PatchManifest,
    deletes: Vec<PathBuf>,
}

impl PreparedPatch {
    pub(super) fn from_version(&self) -> u32 {
        self.manifest.from_version
    }
    pub(super) fn to_version(&self) -> u32 {
        self.manifest.to_version
    }
}

fn metadata_path(extracted: &Path, root: &Path, name: &str) -> PathBuf {
    let nested = root.join(name);
    if nested.exists() {
        nested
    } else {
        extracted.join(name)
    }
}

fn asset_relative_path(value: &str) -> Result<PathBuf, String> {
    let parts: Vec<_> = value.split('/').collect();
    if parts.len() < 2
        || !ASSET_DIRS.contains(&parts[0])
        || parts.iter().any(|part| {
            part.is_empty() || *part == "." || *part == ".." || part.contains(['\\', ':'])
        })
    {
        return Err(format!("升级包包含无效素材路径: {value}"));
    }
    Ok(parts.iter().collect())
}

pub(super) fn read_patch(extracted: &Path, root: &Path) -> Result<Option<PreparedPatch>, String> {
    let path = metadata_path(extracted, root, "patch-manifest.json");
    if !path.exists() {
        return Ok(None);
    }
    let manifest: PatchManifest =
        serde_json::from_slice(&fs::read(path).map_err(|e| format!("读取升级包版本失败: {e}"))?)
            .map_err(|e| format!("解析升级包版本失败: {e}"))?;
    if manifest.to_version <= manifest.from_version {
        return Err("升级包的目标版本必须高于起始版本".to_string());
    }
    let deletes: PatchDeletes = serde_json::from_slice(
        &fs::read(metadata_path(extracted, root, "deletes.json"))
            .map_err(|e| format!("读取升级包删除清单失败: {e}"))?,
    )
    .map_err(|e| format!("解析升级包删除清单失败: {e}"))?;
    Ok(Some(PreparedPatch {
        root: root.to_path_buf(),
        manifest,
        deletes: deletes
            .files
            .iter()
            .map(|path| asset_relative_path(path))
            .collect::<Result<_, _>>()?,
    }))
}

// Reject symlinks and file/directory collisions before either backup or installation.
fn validate_destination(root: &Path, relative: &Path) -> Result<(), String> {
    let mut path = root.to_path_buf();
    for part in relative.components() {
        if !matches!(part, Component::Normal(_)) {
            return Err("升级包包含无效素材路径".to_string());
        }
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta)
                if meta.file_type().is_symlink()
                    || (path == root.join(relative) && !meta.is_file())
                    || (path != root.join(relative) && !meta.is_dir()) =>
            {
                return Err(format!(
                    "升级包目标路径不是普通文件或目录: {}",
                    path.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("检查升级包目标路径失败: {error}")),
        }
    }
    Ok(())
}

fn collect_files(
    root: &Path,
    relative: &Path,
    changes: &mut BTreeMap<PathBuf, Option<PathBuf>>,
) -> Result<(), String> {
    for entry in
        fs::read_dir(root.join(relative)).map_err(|e| format!("读取升级包素材失败: {e}"))?
    {
        let entry = entry.map_err(|e| format!("读取升级包素材失败: {e}"))?;
        let path = relative.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            collect_files(root, &path, changes)?;
        } else {
            let value = path
                .to_str()
                .ok_or("升级包素材路径无效")?
                .replace(std::path::MAIN_SEPARATOR, "/");
            asset_relative_path(&value)?;
            changes.insert(path, Some(entry.path()));
        }
    }
    Ok(())
}

pub(super) fn install_patches(
    mut patches: Vec<PreparedPatch>,
    assets_root: &Path,
    staging: &Path,
    cancel: &AtomicBool,
) -> Result<AssetBundleImportResult, String> {
    let mut version = read_asset_version(assets_root)
        .ok_or("无法确定当前素材版本，请先安装完整素材包")?
        .version;
    if count_files_recursive(&assets_root.join("servants"))? == 0
        || count_files_recursive(&assets_root.join("ces"))? == 0
    {
        return Err("当前素材包不完整，请先安装完整素材包".to_string());
    }
    patches.sort_by_key(|patch| patch.manifest.from_version);
    let mut changes = BTreeMap::new();
    for patch in patches {
        ensure_asset_import_not_cancelled(cancel)?;
        if patch.manifest.from_version != version {
            return Err(format!(
                "升级包无法连续衔接：需要从 v{version} 升级，但所选包为 v{} → v{}",
                patch.manifest.from_version, patch.manifest.to_version
            ));
        }
        for name in ASSET_DIRS {
            if patch.root.join(name).is_dir() {
                collect_files(&patch.root, Path::new(name), &mut changes)?;
            }
        }
        for path in patch.deletes {
            changes.insert(path, None);
        }
        version = patch.manifest.to_version;
    }
    let mut result = AssetBundleImportResult {
        installed_version: version,
        install_dir: assets_root.to_string_lossy().into_owned(),
        ..Default::default()
    };
    for (path, source) in &changes {
        validate_destination(assets_root, path)?;
        if path.starts_with("servants") {
            result.imported_servants = true;
            result.servant_files += u64::from(source.is_some());
        } else if path.starts_with("ces") {
            result.imported_craft_essences = true;
            result.craft_essence_files += u64::from(source.is_some());
        }
    }
    // Include the version record in the same rollback boundary as the changed files.
    let version_root = staging.join("version");
    write_asset_version(&version_root, version)?;
    // Commit the version last so readers never see the new version before its files.
    let changes = changes.into_iter().chain(std::iter::once((
        PathBuf::from("assets-version.json"),
        Some(asset_version_path(&version_root)),
    )));
    let backup_root = staging.join("backup");
    fs::create_dir_all(&backup_root).map_err(|e| e.to_string())?;
    let mut operations = Vec::new();
    for (index, (relative, source)) in changes.enumerate() {
        ensure_asset_import_not_cancelled(cancel)?;
        validate_destination(assets_root, &relative)?;
        let target = assets_root.join(relative);
        let backup = if target.exists() {
            let path = backup_root.join(index.to_string());
            fs::copy(&target, &path).map_err(|e| format!("备份升级素材失败: {e}"))?;
            Some(path)
        } else {
            None
        };
        operations.push((target, source, backup));
    }
    let _cache_reset = crate::commands::catalog::reset_picker_cache_on_drop();
    let mut attempted = 0;
    let outcome = (|| {
        for (target, source, _) in &operations {
            ensure_asset_import_not_cancelled(cancel)?;
            attempted += 1;
            if let Some(source) = source {
                fs::create_dir_all(target.parent().ok_or("无法定位素材目录")?)
                    .map_err(|e| e.to_string())?;
                fs::copy(source, target).map_err(|e| format!("安装升级素材失败: {e}"))?;
            } else if target.exists() {
                fs::remove_file(target).map_err(|e| format!("删除旧素材失败: {e}"))?;
            }
        }
        Ok(())
    })();
    if let Err(error) = outcome {
        let mut restore_errors = Vec::new();
        for (target, _, backup) in operations[..attempted].iter().rev() {
            let restore = (|| -> std::io::Result<()> {
                if target.exists() {
                    fs::remove_file(target)?;
                }
                if let Some(backup) = backup {
                    fs::rename(backup, target)?;
                }
                Ok(())
            })();
            if let Err(restore_error) = restore {
                restore_errors.push(format!("{}: {restore_error}", target.display()));
            }
        }
        if !restore_errors.is_empty() {
            return Err(format!(
                "{error}；恢复素材失败: {}，请重新安装完整素材包",
                restore_errors.join("；")
            ));
        }
        return Err(error);
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
