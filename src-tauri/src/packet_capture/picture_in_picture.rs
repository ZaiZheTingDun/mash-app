//! Control Android's PiP app-op, rather than an unverified Reqable config key.
use super::root_shell;

// Match the Android user whose Reqable configuration and certificates we manage.
const GET: &str = "cmd appops get --user 0 com.reqable.android PICTURE_IN_PICTURE";
const SET: &str = "cmd appops set --user 0 com.reqable.android PICTURE_IN_PICTURE ignore";
const READ_ERROR: &str = "无法读取 Reqable 的系统画中画权限，请检查设备连接后重试";
const SET_ERROR: &str =
    "无法关闭 Reqable 的系统画中画权限，请在 Android 设置中关闭 Reqable 的画中画后重试";
const VERIFY_ERROR: &str =
    "未能确认 Reqable 的系统画中画权限已关闭，请在 Android 设置中关闭 Reqable 的画中画后重试";

fn is_disabled(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let mut found = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("Error:") || line.contains("Exception") {
            return false;
        }
        // AppOps reports a UID override separately. Never accept package-level
        // ignore alongside an allow override (the UID setting takes precedence).
        let line = line.strip_prefix("Uid mode:").unwrap_or(line).trim();
        let mode = line
            .strip_prefix("PICTURE_IN_PICTURE:")
            .or_else(|| line.strip_prefix("android:picture_in_picture:"));
        if let Some(mode) = mode {
            found = true;
            if mode.split(';').next().unwrap_or_default().trim() != "ignore" {
                return false;
            }
        }
    }
    found
}

pub(super) fn disable(
    root: &str,
    shell: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    let current = shell(&root_shell(root, GET)).map_err(|_| READ_ERROR)?;
    if is_disabled(&current) {
        return Ok(());
    }
    shell(&root_shell(root, SET)).map_err(|_| SET_ERROR)?;
    let actual = shell(&root_shell(root, GET)).map_err(|_| VERIFY_ERROR)?;
    if !is_disabled(&actual) {
        return Err(VERIFY_ERROR.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
