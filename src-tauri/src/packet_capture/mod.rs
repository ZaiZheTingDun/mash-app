//! Device-only preparation for the independent packet capture tool.
use crate::device_operation::OperationCancellation;
use crate::runner::LogLevel;
use crate::{adb::Adb, platform};
use std::path::Path;
use std::time::Duration;

mod capture;
mod certificate;
mod client;
mod configuration;
mod history;
mod inventory;
mod lifecycle;
mod onboarding;
mod picture_in_picture;
mod response;
mod stop;
pub(crate) use capture::{start_capture, CaptureStarted};
pub(crate) use configuration::{configure, CaptureConfiguration};
pub(crate) use history::{CaptureHistoryState, CaptureRecord, HistoryStore};
pub(crate) use lifecycle::stop_task;
pub(crate) use response::{ResponseState, ResponseStatus, ResponseStore};
#[cfg(test)]
mod root_tests;
#[cfg(test)]
mod tests;

const CERT_PATHS: [&str; 2] = [
    "/data/user/0/com.reqable.android/files/certificate/reqable-root.crt",
    "/sdcard/Android/data/com.reqable.android/files/certificate/reqable-root.crt",
];
const INSTALL_SCRIPT: &str = include_str!("install_certificate.sh");
const APEX_INSTALL_SCRIPT: &str = include_str!("install_certificate_apex.sh");
type AdbCommand<'a> = dyn FnMut(&[&str]) -> Result<Vec<u8>, String> + 'a;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CertificateInstallation {
    serial: String,
    already_installed: bool,
    fingerprint: String,
    message: String,
}

pub(crate) fn install(
    adb_path: &Path,
    selected: Option<&str>,
    cancellation: &OperationCancellation,
) -> Result<CertificateInstallation, String> {
    install_with_log_using(
        selected,
        &mut |args| run_adb(adb_path, args),
        &mut |_, _| {},
        cancellation,
    )
}

/// Keep the same device selected across both preparation steps.
pub(crate) fn setup(
    adb_path: &Path,
    selected: Option<&str>,
    log: &mut dyn FnMut(LogLevel, &str),
    cancellation: &OperationCancellation,
) -> Result<CaptureConfiguration, String> {
    setup_controlled_using(
        selected,
        &mut |args| run_adb(adb_path, args),
        log,
        cancellation,
    )
}

fn setup_controlled_using(
    selected: Option<&str>,
    run: &mut AdbCommand<'_>,
    log: &mut dyn FnMut(LogLevel, &str),
    cancellation: &OperationCancellation,
) -> Result<CaptureConfiguration, String> {
    cancellation.check()?;
    log(LogLevel::Info, "开始抓包环境配置");
    let result: Result<CaptureConfiguration, String> = (|| {
        let installed = install_with_log_using(selected, run, log, cancellation)?;
        cancellation.check()?;
        log(
            LogLevel::Info,
            &format!("设备 {}：{}", installed.serial, installed.message),
        );
        log(LogLevel::Info, "正在配置 Reqable，仅抓国服 FGO…");
        let mut configured =
            configuration::configure_controlled_using(Some(&installed.serial), run, cancellation)
                .map_err(|error| format!("证书已安装，环境配置未完成：{error}"))?;
        log(LogLevel::Info, "已关闭并验证 Reqable 的系统画中画权限");
        configured.message = "环境配置完成，请点击开始按钮（设备重启后可能需要重新配置）".into();
        Ok(configured)
    })();
    if cancellation.is_cancelled() {
        // Cancellation must not hide a failed rollback; preserve its backup path
        // in the operation log even though this request's result is superseded.
        if let Err(error) = &result {
            if error.contains("未能确认回滚") {
                log(
                    LogLevel::Warn,
                    &format!("停止环境配置时回滚未完成：{error}"),
                );
            }
        }
        return Err("环境配置已停止".into());
    }
    match &result {
        Ok(configured) => log(LogLevel::Info, &configured.message),
        Err(error) => log(LogLevel::Warn, &format!("抓包环境配置失败：{error}")),
    }
    result
}

fn run_adb(adb_path: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = platform::output_with_timeout(
        crate::adb::adb_command(adb_path).args(args),
        Duration::from_secs(20),
    )
    .map_err(|error| format!("ADB 命令执行失败：{error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .trim()
        .chars()
        .take(1500)
        .collect());
    }
    Ok(output.stdout)
}

fn find_root(
    shell: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<&'static str, String> {
    ["", "su -c", "su 0 sh -c", "/system/xbin/bstk/su -c"]
        .into_iter()
        .find(|prefix| {
            shell(&root_shell(prefix, "id -u"))
                .is_ok_and(|output| String::from_utf8_lossy(&output).trim() == "0")
        })
        .ok_or_else(|| "无法取得 root 权限，请确认设备已 root 并允许 ADB 使用 root".into())
}

fn select_serial(output: &str, selected: Option<&str>) -> Result<String, String> {
    let devices = Adb::ready_devices_from_output(output);
    // A saved explicit selection must never silently install onto another device.
    if let Some(serial) = selected {
        if !devices.iter().any(|device| device.serial == serial) {
            return Err(format!("所选 ADB 设备不在线：{serial}"));
        }
    }
    Adb::select_ready_serial(&devices, selected)?
        .ok_or_else(|| "未找到在线 ADB 设备，请先连接设备".into())
}

fn prepare_root_using(
    serial: &str,
    run: &mut AdbCommand<'_>,
    log: &mut dyn FnMut(LogLevel, &str),
    cancellation: &OperationCancellation,
) -> Result<&'static str, String> {
    cancellation.check()?;
    log(LogLevel::Info, "正在检查设备 root 权限…");
    let existing = find_root(&mut |script| {
        cancellation.check()?;
        run(&["-s", serial, "shell", script])
    });
    cancellation.check()?;
    if let Ok(root) = existing {
        log(LogLevel::Info, "设备 root 权限已确认");
        return Ok(root);
    }

    log(LogLevel::Info, "未取得 root 权限，正在尝试 adb root…");
    // A daemon restart can lose its reply. Verify the actual UID after reconnect
    // even when this command fails, rather than trusting its text or exit status.
    let requested = run(&["-s", serial, "root"]);
    cancellation.check()?;
    log(LogLevel::Info, "正在等待原设备重新连接并验证 root 权限…");
    let reconnected = run(&["-s", serial, "wait-for-device"]);
    cancellation.check()?;
    reconnected.map_err(|error| format!("adb root 后等待设备 {serial} 重新连接失败：{error}"))?;
    let verified = find_root(&mut |script| {
        cancellation.check()?;
        run(&["-s", serial, "shell", script])
    });
    cancellation.check()?;
    match verified {
        Ok(root) => {
            log(LogLevel::Info, "设备 root 权限已确认");
            Ok(root)
        }
        Err(error) => {
            let detail = match requested {
                Ok(bytes) => String::from_utf8_lossy(&bytes).trim().to_owned(),
                Err(error) => error,
            };
            let detail: String = detail.chars().take(300).collect();
            Err(if detail.is_empty() {
                format!("adb root 后仍{error}")
            } else {
                format!("adb root 后仍{error}；ADB 返回：{detail}")
            })
        }
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn root_shell(prefix: &str, script: &str) -> String {
    if prefix.is_empty() {
        format!("sh -c {}", shell_quote(script))
    } else {
        format!("{prefix} {}", shell_quote(script))
    }
}

/// Preserve file bytes even when an emulator's shell translates LF to CRLF.
/// Callers bound the source read before encoding it. Keep command failures via
/// pipefail instead of treating an unreadable file as empty successful output.
fn read_shell_bytes(
    root: &str,
    script: &str,
    shell: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let script = format!("set -o pipefail; (\n{script}\n) | base64");
    let mut encoded = shell(&root_shell(root, &script))?;
    encoded.retain(|byte| !byte.is_ascii_whitespace());
    STANDARD
        .decode(encoded)
        .map_err(|_| "设备文件编码无效，无法读取原始内容".into())
}

#[cfg(test)]
fn install_using(
    selected: Option<&str>,
    run: &mut AdbCommand<'_>,
) -> Result<CertificateInstallation, String> {
    install_with_log_using(
        selected,
        run,
        &mut |_, _| {},
        &OperationCancellation::default(),
    )
}

fn install_with_log_using(
    selected: Option<&str>,
    run: &mut AdbCommand<'_>,
    log: &mut dyn FnMut(LogLevel, &str),
    cancellation: &OperationCancellation,
) -> Result<CertificateInstallation, String> {
    cancellation.check()?;
    let devices = run(&["devices", "-l"])?;
    let serial = select_serial(&String::from_utf8_lossy(&devices), selected)?;
    cancellation.bind_device(&serial)?;
    let sdk: u32 = {
        let mut shell = |script: &str| {
            cancellation.check()?;
            run(&["-s", &serial, "shell", script])
        };
        let package = shell("pm path com.reqable.android")?;
        if !String::from_utf8_lossy(&package).contains("package:") {
            return Err("请先在设备上安装 Reqable".into());
        }
        let sdk = shell("getprop ro.build.version.sdk")?;
        String::from_utf8_lossy(&sdk)
            .trim()
            .parse()
            .map_err(|_| "无法读取设备的 Android 版本")?
    };
    if sdk < 21 {
        return Err("当前证书安装需要 Android 5 或以上版本".into());
    }
    let root = prepare_root_using(&serial, run, log, cancellation)?;
    log(LogLevel::Info, "正在安装并验证系统证书…");
    let mut shell = |script: &str| {
        cancellation.check()?;
        run(&["-s", &serial, "shell", script])
    };
    shell("am start -W -n com.reqable.android/.MainActivity")
        .map_err(|error| format!("无法打开 Reqable：{error}"))?;
    let (source_path, bytes) =
        onboarding::certificate_after_confirmation(root, &mut shell, log, cancellation)?;
    let cert = certificate::parse(&bytes)?;
    let script = if sdk >= 34 {
        APEX_INSTALL_SCRIPT
    } else {
        INSTALL_SCRIPT
    }
    .replace("\r\n", "\n")
    .replace("__CERT_PATH__", source_path)
    .replace("__SOURCE_SHA256__", &cert.source_sha256)
    .replace("__SUBJECT_HASH__", &cert.subject_hash)
    .replace("__SESSION_ID__", &uuid::Uuid::new_v4().simple().to_string());
    let result =
        shell(&root_shell(root, &script)).map_err(|error| format!("安装系统证书失败：{error}"))?;
    let result = String::from_utf8_lossy(&result);
    let already_installed = match result.trim() {
        "MASH_CA_ALREADY_INSTALLED" => true,
        "MASH_CA_INSTALLED" => false,
        _ => return Err("证书安装未返回验证结果，请重试".into()),
    };
    Ok(CertificateInstallation {
        serial,
        already_installed,
        fingerprint: cert.fingerprint,
        message: if already_installed {
            "系统证书已验证，无需重复安装"
        } else {
            "证书已安装，设备重启后需重新安装"
        }
        .into(),
    })
}
