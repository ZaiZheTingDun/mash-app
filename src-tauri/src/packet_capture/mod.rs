//! Device-only preparation for the independent packet capture tool.
use crate::{adb::Adb, platform};
use std::path::Path;
use std::time::Duration;

mod capture;
mod certificate;
mod client;
mod configuration;
mod response;
mod stop;
pub(crate) use capture::{start_capture, CaptureStarted};
pub(crate) use configuration::{configure, CaptureConfiguration};
pub(crate) use response::{ResponseState, ResponseStore};
#[cfg(test)]
mod tests;

const CERT_PATHS: [&str; 2] = [
    "/data/user/0/com.reqable.android/files/certificate/reqable-root.crt",
    "/sdcard/Android/data/com.reqable.android/files/certificate/reqable-root.crt",
];
const INSTALL_SCRIPT: &str = include_str!("install_certificate.sh");
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
) -> Result<CertificateInstallation, String> {
    install_using(selected, &mut |args| run_adb(adb_path, args))
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

fn install_using(
    selected: Option<&str>,
    run: &mut AdbCommand<'_>,
) -> Result<CertificateInstallation, String> {
    let devices = run(&["devices", "-l"])?;
    let serial = select_serial(&String::from_utf8_lossy(&devices), selected)?;
    let mut shell = |script: &str| run(&["-s", &serial, "shell", script]);
    let package = shell("pm path com.reqable.android")?;
    if !String::from_utf8_lossy(&package).contains("package:") {
        return Err("请先在设备上安装 Reqable".into());
    }
    let sdk = shell("getprop ro.build.version.sdk")?;
    let sdk: u32 = String::from_utf8_lossy(&sdk)
        .trim()
        .parse()
        .map_err(|_| "无法读取设备的 Android 版本")?;
    if !(21..=33).contains(&sdk) {
        return Err("当前证书安装仅支持 Android 5–13；Android 14 及以上暂未适配".into());
    }
    let root = find_root(&mut shell)?;
    shell("am start -W -n com.reqable.android/.MainActivity")
        .map_err(|error| format!("无法打开 Reqable：{error}"))?;
    let mut source = None;
    for attempt in 0..6 {
        for path in CERT_PATHS {
            if let Ok(bytes) = shell(&root_shell(root, &format!("cat {path}"))) {
                if !bytes.is_empty() {
                    source = Some((path, bytes));
                    break;
                }
            }
        }
        if source.is_some() {
            break;
        }
        if attempt < 5 {
            std::thread::sleep(Duration::from_millis(300));
        }
    }
    let (source_path, bytes) =
        source.ok_or("未找到 Reqable CA，请先完成 Reqable 的首次初始化后重试")?;
    let cert = certificate::parse(&bytes)?;
    let script = INSTALL_SCRIPT
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
