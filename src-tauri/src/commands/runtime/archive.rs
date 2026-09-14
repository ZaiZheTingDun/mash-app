//! Runtime ZIP validation and extraction helpers.

use super::{code_bundle_root, runtime_bundle_root, runtime_exe_name};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use zip::ZipArchive;

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
