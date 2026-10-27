use super::{find_root, root_shell, run_adb, select_serial, shell_quote, AdbCommand};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

const PACKAGE: &str = "com.bilibili.fatego";
const VERSION: &str = "3.3.5";
const CONFIG: &str = "/data/user/0/com.reqable.android/files/config/capture_config";
const MAX_CONFIG_SIZE: usize = 64 * 1024;
const ABSENT: &[u8] = b"MASH_CONFIG_ABSENT";

pub(super) fn check_capture_configuration(
    shell: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    let info = shell("dumpsys package com.reqable.android")?;
    if !String::from_utf8_lossy(&info)
        .lines()
        .any(|line| line.trim() == format!("versionName={VERSION}"))
    {
        return Err(format!("当前抓包仅支持已验证的 Reqable {VERSION}"));
    }
    let root = find_root(shell)?;
    let bytes = shell(&root_shell(
        root,
        &format!("head -c {} {CONFIG}", MAX_CONFIG_SIZE + 1),
    ))
    .map_err(|_| "请先点击自动配置，将 Reqable 设置为仅抓国服 FGO")?;
    let config = parse_config(&bytes)?;
    if cn_config(config.clone()) != config {
        return Err("请先点击自动配置，将 Reqable 设置为仅抓国服 FGO".into());
    }
    Ok(())
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureConfiguration {
    serial: String,
    package_name: String,
    reqable_version: String,
    message: String,
}

pub(crate) fn configure(
    adb_path: &Path,
    selected: Option<&str>,
) -> Result<CaptureConfiguration, String> {
    configure_using(selected, &mut |args| run_adb(adb_path, args))
}

fn parse_config(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > MAX_CONFIG_SIZE {
        return Err("Reqable 配置过大，未修改原配置".into());
    }
    let config: Value =
        serde_json::from_slice(bytes).map_err(|_| "Reqable 配置格式无效，未修改原配置")?;
    let object = config.as_object().ok_or("Reqable 配置格式无效")?;
    for key in ["vpnMode", "hostMode", "androidAppsBypass", "autoCapture"] {
        if !object.get(key).is_some_and(Value::is_boolean) {
            return Err(format!("Reqable 配置字段 {key} 不兼容，未修改原配置"));
        }
    }
    if !object.get("androidApps").is_some_and(|apps| {
        apps.as_array()
            .is_some_and(|apps| apps.iter().all(Value::is_string))
    }) {
        return Err("Reqable 应用选择配置不兼容，未修改原配置".into());
    }
    Ok(config)
}

fn cn_config(mut config: Value) -> Value {
    config["androidApps"] = serde_json::json!([PACKAGE]);
    config["androidAppsBypass"] = false.into();
    config["vpnMode"] = true.into();
    config["hostMode"] = false.into();
    // Restarting Reqable must not start collecting traffic as a side effect.
    config["autoCapture"] = false.into();
    config
}

fn snapshot(bytes: Vec<u8>) -> Result<Option<Vec<u8>>, String> {
    if bytes.trim_ascii() == ABSENT {
        Ok(None)
    } else {
        parse_config(&bytes)?;
        Ok(Some(bytes))
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn opened(output: &[u8]) -> bool {
    String::from_utf8_lossy(output)
        .lines()
        .any(|line| line.trim() == "Status: ok")
        && !String::from_utf8_lossy(output).contains("Error:")
}

fn configure_using(
    selected: Option<&str>,
    run: &mut AdbCommand<'_>,
) -> Result<CaptureConfiguration, String> {
    let devices = run(&["devices", "-l"])?;
    let serial = select_serial(&String::from_utf8_lossy(&devices), selected)?;
    let mut shell = |script: &str| {
        let result = run(&["-s", &serial, "shell", script]);
        if script == "am force-stop com.reqable.android" {
            // Stopping its VPN can briefly take BlueStacks' ADB transport offline.
            // Wait on this exact serial; never reconnect to a different device.
            run(&["-s", &serial, "wait-for-device"])?;
            if result.is_err() {
                return run(&["-s", &serial, "shell", script]);
            }
        }
        result
    };
    for (package, message) in [
        ("com.reqable.android", "请先在设备上安装 Reqable"),
        (PACKAGE, "请先在设备上安装国服 FGO"),
    ] {
        let output = shell(&format!("pm path {package}"))?;
        if !String::from_utf8_lossy(&output).contains("package:") {
            return Err(message.into());
        }
    }
    let info = shell("dumpsys package com.reqable.android")?;
    let supported = String::from_utf8_lossy(&info)
        .lines()
        .any(|line| line.trim() == format!("versionName={VERSION}"));
    if !supported {
        return Err(format!(
            "当前自动配置仅支持已验证的 Reqable {VERSION}，请确认设备上的版本"
        ));
    }
    let root = find_root(&mut shell)?;
    let read = format!(
        "test -d /data/user/0/com.reqable.android/files/config || exit 1; if test -f {CONFIG}; then head -c {} {CONFIG}; elif test ! -e {CONFIG}; then echo MASH_CONFIG_ABSENT; else exit 1; fi",
        MAX_CONFIG_SIZE + 1
    );
    // Check compatibility before stopping a running capture.
    snapshot(
        shell(&root_shell(root, &read))
            .map_err(|_| "无法读取 Reqable 配置，请先完成 Reqable 的首次初始化")?,
    )?;
    shell("am force-stop com.reqable.android").map_err(|_| "无法停止 Reqable，请关闭抓包后重试")?;
    // Read again after shutdown so we preserve its last saved settings.
    let original = snapshot(
        shell(&root_shell(root, &read))
            .map_err(|_| "无法读取 Reqable 配置，请先完成 Reqable 的首次初始化")?,
    )?;
    let desired = cn_config(match &original {
        Some(bytes) => parse_config(bytes)?,
        // Reqable lazily creates this file after its first settings change.
        // Omitted fields continue to use Reqable's own defaults.
        None => serde_json::json!({}),
    });
    let bytes = serde_json::to_vec(&desired).map_err(|_| "无法生成 Reqable 配置")?;
    if bytes.len() > MAX_CONFIG_SIZE {
        return Err("生成的 Reqable 配置过大，未修改原配置".into());
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let backup = format!("{CONFIG}.mash-{id}.bak");
    let stage = format!("{CONFIG}.mash-{id}.tmp");
    let old_hash = hash(original.as_deref().unwrap_or_default());
    let new_hash = hash(&bytes);
    let prepare = if original.is_some() {
        format!(
            r#"test "$(sha256sum "$p" | cut -d ' ' -f 1)" = {old_hash}
cp -p "$p" "$backup"
cp -p "$p" "$stage""#
        )
    } else {
        r#"test ! -e "$p"
umask 077
: > "$backup"
: > "$stage"
chown "$(stat -c '%u:%g' /data/user/0/com.reqable.android/files/config)" "$stage""#
            .into()
    };
    let apply = format!(
        r#"set -eu
p={CONFIG}
backup={backup}
stage={stage}
test ! -L "$p"
test ! -e "$backup"
test ! -e "$stage"
if pidof com.reqable.android >/dev/null; then exit 1; fi
{prepare}
trap 'rm -f "$stage"' EXIT
printf %s {json} > "$stage"
test "$(sha256sum "$stage" | cut -d ' ' -f 1)" = {new_hash}
mv -f "$stage" "$p"
echo MASH_CONFIG_WRITTEN"#,
        json = shell_quote(std::str::from_utf8(&bytes).unwrap()),
    );
    let attempt = (|| -> Result<(), String> {
        let result = shell(&root_shell(root, &apply)).map_err(|_| "写入 Reqable 配置失败")?;
        if result.trim_ascii() != b"MASH_CONFIG_WRITTEN" {
            return Err("Reqable 配置写入未返回验证结果".into());
        }
        let launch = shell("am start -W -n com.reqable.android/.MainActivity")
            .map_err(|_| "无法重新打开 Reqable")?;
        if !opened(&launch) {
            return Err("无法重新打开 Reqable".into());
        }
        let actual = shell(&root_shell(root, &read)).map_err(|_| "无法回读 Reqable 配置")?;
        let actual = parse_config(&actual)?;
        if !desired
            .as_object()
            .unwrap()
            .iter()
            .all(|(key, value)| actual.get(key) == Some(value))
        {
            return Err("Reqable 配置回读不一致".into());
        }
        Ok(())
    })();
    if let Err(error) = attempt {
        let stopped = shell("am force-stop com.reqable.android").is_ok();
        let restore = if original.is_some() {
            format!(
                r#"if test -f {backup}; then
  test "$(sha256sum {backup} | cut -d ' ' -f 1)" = {old_hash}
  mv -f {backup} {CONFIG}
fi
test "$(sha256sum {CONFIG} | cut -d ' ' -f 1)" = {old_hash}"#
            )
        } else {
            format!(
                r#"if test -f {backup}; then
  test "$(sha256sum {backup} | cut -d ' ' -f 1)" = {old_hash}
  rm -f {CONFIG} {backup}
fi
test ! -e {CONFIG}"#
            )
        };
        let rollback = format!(
            r#"set -eu
if pidof com.reqable.android >/dev/null; then exit 1; fi
{restore}
rm -f {stage}
echo MASH_CONFIG_RESTORED"#
        );
        let restored = stopped
            && shell(&root_shell(root, &rollback))
                .is_ok_and(|output| output.trim_ascii() == b"MASH_CONFIG_RESTORED");
        if restored {
            let _ = shell("am start -W -n com.reqable.android/.MainActivity");
            return Err(format!("{error}，已恢复原配置"));
        }
        return Err(format!(
            "{error}；未能确认回滚，请检查 Reqable 配置，备份位置：{backup}"
        ));
    }
    // The backup contains only settings, never captured requests or certificate keys.
    let _ = shell(&root_shell(root, &format!("rm -f {backup} {stage}")));
    Ok(CaptureConfiguration {
        serial,
        package_name: PACKAGE.into(),
        reqable_version: VERSION.into(),
        message: "Reqable 已配置为仅抓国服 FGO，抓包未开启".into(),
    })
}

#[cfg(test)]
mod tests;
