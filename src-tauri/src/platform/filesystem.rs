//! Host link and executable-permission semantics.
//! Archive root and path validation remain in the runtime installer.

use std::fs;
#[cfg(unix)]
use std::io::Read;
#[cfg(not(unix))]
use std::io::{self, Write};
use std::path::Path;

#[cfg(any(unix, test))]
fn relative_target_stays_within_root(root: &Path, link_parent: &Path, target: &Path) -> bool {
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

#[cfg(unix)]
pub(crate) fn copy_symlink(src: &Path, dst: &Path) -> Result<(), String> {
    use std::os::unix::fs::symlink;

    let target = fs::read_link(src).map_err(|e| format!("read symlink failed: {e}"))?;
    symlink(target, dst).map_err(|e| format!("create symlink failed: {e}"))
}

#[cfg(not(unix))]
pub(crate) fn copy_symlink(src: &Path, dst: &Path) -> Result<(), String> {
    fs::copy(src, dst)
        .map(|_| ())
        .map_err(|e| format!("copy symlink target failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_link_targets_allow_internal_parent_traversal() {
        let root = Path::new("runtime");
        assert!(relative_target_stays_within_root(
            root,
            &root.join("framework/Versions"),
            Path::new("../Current/library"),
        ));
        assert!(relative_target_stays_within_root(
            root,
            root,
            Path::new("./library"),
        ));
    }

    #[test]
    fn archive_link_targets_reject_escape_and_foreign_parent() {
        let root = Path::new("runtime");
        for target in ["../escape", "child/../../escape", "/escape"] {
            assert!(!relative_target_stays_within_root(
                root,
                root,
                Path::new(target)
            ));
        }
        assert!(!relative_target_stays_within_root(
            root,
            Path::new("outside"),
            Path::new("library"),
        ));
    }
}
