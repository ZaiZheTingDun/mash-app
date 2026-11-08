use super::response::Baseline;
use super::target;
use super::{
    client, configuration, credentials, response, run_adb, select_serial, AdbCommand, ApiKeyStore,
};
use crate::device_operation::OperationCancellation;
use crate::Server;
use std::{path::Path, time::Duration};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureStarted {
    pub(super) serial: String,
    #[serde(skip)]
    server: Server,
    already_active: bool,
    message: String,
    warnings: Vec<String>,
    #[serde(skip)]
    pub(super) baseline: Baseline,
    #[serde(skip)]
    api_key: Option<String>,
}

impl CaptureStarted {
    pub(crate) fn begin_response_watch(&self, store: &response::ResponseStore) {
        let mut baseline = self.baseline.clone();
        baseline.warnings.extend(
            self.warnings
                .iter()
                .filter(|warning| !baseline.warnings.contains(warning))
                .cloned()
                .collect::<Vec<_>>(),
        );
        store.begin_authenticated(
            self.server,
            self.serial.clone(),
            baseline,
            self.api_key.clone(),
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Request {
    Status,
    Start,
    Stop,
}

type Api<'a> = dyn FnMut(u16, Request) -> Result<Option<bool>, String> + 'a;

pub(super) fn decode_status(bytes: &[u8]) -> Result<bool, String> {
    #[derive(serde::Deserialize)]
    struct Status {
        status: String,
    }
    let status: Status = serde_json::from_slice(bytes).map_err(|_| "Reqable 抓包状态无效")?;
    match status.status.as_str() {
        "active" => Ok(true),
        "inactive" => Ok(false),
        _ => Err("Reqable 返回了未知抓包状态".into()),
    }
}

pub(crate) fn start_capture(
    server: Server,
    package: &str,
    adb_path: &Path,
    selected: Option<&str>,
    cancellation: &OperationCancellation,
    keys: &ApiKeyStore,
    jar_path: &Path,
    authorization: &credentials::AuthorizationStore,
    progress: &mut dyn FnMut(&str),
) -> Result<CaptureStarted, String> {
    let client = std::cell::RefCell::new(client::Client::new()?);
    let api_key = std::cell::RefCell::new(None);
    let progress = std::cell::RefCell::new(progress);
    let mut started = start_controlled_using(
        server,
        package,
        selected,
        &mut |args| run_adb(adb_path, args),
        &mut |serial, port, version| {
            let requires_auth = if super::version::parse(version).is_none() {
                let probe = credentials::probe_access(port, cancellation);
                match probe {
                    Ok(()) => false,
                    Err(error) if error == client::AUTHENTICATION_FAILED => true,
                    Err(error) => return Err(error),
                }
            } else {
                credentials::needs_api_key(version)
            };
            if requires_auth {
                let key = credentials::authorize(
                    keys,
                    adb_path,
                    jar_path,
                    serial,
                    port,
                    cancellation,
                    authorization,
                    &mut |message| progress.borrow_mut()(message),
                )?;
                *client.borrow_mut() = client::Client::with_api_key(Some(&key))?;
                *api_key.borrow_mut() = Some(key);
            }
            Ok(())
        },
        &mut |port, request| {
            let route = match request {
                Request::Status => "status",
                Request::Start => "on",
                Request::Stop => "off",
            };
            let bytes = client.borrow().request(port, route, None, 4096)?;
            if request == Request::Status {
                Ok(Some(decode_status(&bytes)?))
            } else {
                Ok(None)
            }
        },
        &mut |port| {
            response::baseline_controlled(
                server,
                &client.borrow(),
                port,
                &mut || cancellation.check(),
                &mut |message| progress.borrow_mut()(message),
            )
        },
        &mut || cancellation.wait(Duration::from_millis(250)),
        cancellation,
    )?;
    started.api_key = api_key.into_inner();
    Ok(started)
}

fn package_uid(info: &[u8]) -> Result<u32, String> {
    String::from_utf8_lossy(info)
        .lines()
        .find_map(|line| {
            // Newer Android package dumps label the application UID as appId.
            let line = line.trim();
            line.strip_prefix("userId=")
                .or_else(|| line.strip_prefix("appId="))?
                .split_whitespace()
                .next()?
                .parse::<u32>()
                .ok()
        })
        .filter(|uid| (10000..20000).contains(uid))
        .ok_or_else(|| "未找到所选服务器的 FGO，请先安装游戏".into())
}

#[derive(Debug, PartialEq, Eq)]
enum VpnScope {
    OnlyGame,
    Wider,
    ExcludesGame,
    Unknown,
}

fn vpn_scope(output: &[u8], uid: u32, owner: Option<u32>) -> VpnScope {
    let text = String::from_utf8_lossy(output);
    let candidates: Vec<_> = text
        .lines()
        .filter(|line| {
            line.contains("VPN") && line.contains("CONNECTED") && !line.contains("DISCONNECTED")
        })
        .filter(|line| {
            owner.is_none_or(|owner| {
                line.split_once("OwnerUid:")
                    .and_then(|(_, rest)| rest.split_whitespace().next())
                    .and_then(|value| value.parse::<u32>().ok())
                    == Some(owner)
            })
        })
        .collect();
    if candidates.len() != 1 {
        return VpnScope::Unknown;
    }
    let Some(ranges) = candidates[0]
        .split_once("Uids: <{")
        .and_then(|(_, rest)| rest.split_once("}>"))
    else {
        return VpnScope::Unknown;
    };
    let mut includes_game = false;
    let mut wider = false;
    for range in ranges.0.split(',') {
        let range = range.trim();
        let (start, end) = range.split_once('-').unwrap_or((range, range));
        let (Ok(start), Ok(end)) = (start.parse::<u32>(), end.parse::<u32>()) else {
            return VpnScope::Unknown;
        };
        if start > end {
            return VpnScope::Unknown;
        }
        includes_game |= (start..=end).contains(&uid);
        wider |= start != end || (start != uid && Some(start) != uid.checked_add(10000));
    }
    if !includes_game {
        // Do not reject based on another VPN's dump or unverified ownership.
        if owner.is_some() {
            VpnScope::ExcludesGame
        } else {
            VpnScope::Unknown
        }
    } else if wider {
        VpnScope::Wider
    } else {
        VpnScope::OnlyGame
    }
}

#[cfg(test)]
fn vpn_only_game(output: &[u8], uid: u32) -> bool {
    vpn_scope(output, uid, Some(10082)) == VpnScope::OnlyGame
}

fn start_controlled_using(
    server: Server,
    package: &str,
    selected: Option<&str>,
    run: &mut AdbCommand<'_>,
    authorize: &mut dyn FnMut(&str, u16, &str) -> Result<(), String>,
    api: &mut Api<'_>,
    prepare: &mut dyn FnMut(u16) -> Result<Baseline, String>,
    pause: &mut dyn FnMut(),
    cancellation: &OperationCancellation,
) -> Result<CaptureStarted, String> {
    if !target::matches_server(server, package) {
        return Err("客户端包名与服务器不匹配".into());
    }
    cancellation.check()?;
    let mut forward = |args: &[&str]| {
        cancellation.check()?;
        run(args)
    };
    let devices = forward(&["devices", "-l"])?;
    let serial = select_serial(&String::from_utf8_lossy(&devices), selected)?;
    cancellation.bind_device(&serial)?;
    let (version, mut warnings) =
        configuration::prepare_capture_configuration(server, package, &mut |command| {
            forward(&["-s", &serial, "shell", command])
        })?;
    let uid = forward(&[
        "-s",
        &serial,
        "shell",
        &format!("dumpsys package {package}"),
    ])
    .ok()
    .and_then(|bytes| package_uid(&bytes).ok());
    let owner = forward(&[
        "-s",
        &serial,
        "shell",
        "dumpsys package com.reqable.android",
    ])
    .ok()
    .and_then(|bytes| package_uid(&bytes).ok());
    cancellation.check()?;
    let launch = forward(&[
        "-s",
        &serial,
        "shell",
        "am start -W -n com.reqable.android/.MainActivity",
    ])?;
    if !String::from_utf8_lossy(&launch)
        .lines()
        .any(|line| line.trim() == "Status: ok")
    {
        return Err("无法打开 Reqable，请在设备上完成首次初始化后重试".into());
    }
    let port = forward(&["-s", &serial, "forward", "tcp:0", "tcp:9016"])?;
    let port: u16 = String::from_utf8_lossy(&port)
        .trim()
        .parse()
        .ok()
        .filter(|port| *port != 0)
        .ok_or("无法建立 Reqable 本机控制连接")?;
    let local = format!("tcp:{port}");
    let mut may_be_active = false;
    let mut requested_start = false;
    let result = (|| {
        authorize(&serial, port, &version)?;
        cancellation.check()?;
        let mut initial = None;
        let mut last_error = None;
        for _ in 0..12 {
            cancellation.check()?;
            match api(port, Request::Status) {
                Ok(Some(active)) => {
                    initial = Some(active);
                    break;
                }
                Err(error) => last_error = Some(error),
                Ok(None) => {}
            }
            pause();
        }
        let already_active = initial.ok_or_else(|| match last_error {
            Some(error) => format!("Reqable 控制接口尚未就绪：{error}"),
            None => "Reqable 控制接口尚未就绪，请稍后重试".into(),
        })?;
        cancellation.check()?;
        let baseline = prepare(port)?;
        cancellation.check()?;
        may_be_active = true;
        if !already_active {
            // POST only queues the change; its HTTP success is not capture success.
            requested_start = true;
            api(port, Request::Start)?;
        }
        for _ in 0..120 {
            cancellation.check()?;
            match api(port, Request::Status) {
                Ok(Some(true)) => {
                    // VPN toggling may briefly interrupt this emulator's ADB.
                    let _ = forward(&["-s", &serial, "wait-for-device"]);
                    pause();
                    let mut scope = VpnScope::Unknown;
                    if let Some(uid) = uid {
                        for _ in 0..4 {
                            cancellation.check()?;
                            scope = forward(&["-s", &serial, "shell", "dumpsys connectivity"])
                                .map(|bytes| vpn_scope(&bytes, uid, owner))
                                .unwrap_or(VpnScope::Unknown);
                            if matches!(scope, VpnScope::OnlyGame | VpnScope::Wider) {
                                break;
                            }
                            pause();
                        }
                    }
                    cancellation.check()?;
                    match scope {
                        VpnScope::ExcludesGame => {
                            return Err(
                                "Reqable VPN 未覆盖所选客户端，请重新进行环境配置后重试".into()
                            )
                        }
                        VpnScope::Wider => warnings.push("VPN 范围包含其他应用，已继续抓包".into()),
                        VpnScope::Unknown => {
                            warnings.push("无法确认 VPN 的应用范围，已继续抓包".into())
                        }
                        VpnScope::OnlyGame => {}
                    }
                    warnings.extend(baseline.warnings.iter().cloned());
                    return Ok(CaptureStarted {
                        serial: serial.clone(),
                        server,
                        already_active,
                        baseline,
                        api_key: None,
                        warnings,
                        message: if already_active {
                            format!("{}抓包已在运行", target::label(server))
                        } else {
                            format!("已开始抓取{} FGO 流量", target::label(server))
                        },
                    });
                }
                Err(_) => {
                    let _ = forward(&["-s", &serial, "forward", &local, "tcp:9016"]);
                }
                _ => {}
            }
            pause();
        }
        Err("抓包尚未启动，请在设备上允许 Reqable 的 VPN 授权后重试".into())
    })();
    let result = if cancellation.is_cancelled() {
        // The stop command takes this lease after the worker exits, then closes
        // the pinned Reqable process, including any pending VPN authorization.
        Err("抓包启动已停止".into())
    } else if result.is_err() && may_be_active {
        // Cancel an enqueued start as well. Force-stop is the fallback when
        // VPN permissions or a broken API prevent confirming an HTTP stop.
        let _ = api(port, Request::Stop);
        let stopped = (0..8).any(|_| {
            let inactive = matches!(api(port, Request::Status), Ok(Some(false)));
            if !inactive {
                pause();
            }
            inactive
        });
        if requested_start {
            // An inactive API status does not dismiss a pending permission
            // dialog. Cancel its continuation so a later Allow cannot revive
            // a start that Mash already reported as failed.
            let exited =
                run(&["-s", &serial, "shell", "am force-stop com.reqable.android"]).is_ok();
            result.map_err(|error: String| {
                format!(
                    "{error}；{}",
                    if exited {
                        "抓包已关闭，已取消启动"
                    } else if stopped {
                        "抓包已关闭，请在设备上取消待确认的 VPN 弹窗"
                    } else {
                        "无法确认抓包已停止，请在设备上检查 Reqable"
                    }
                )
            })
        } else if stopped {
            result.map_err(|error: String| format!("{error}；抓包已关闭"))
        } else {
            let stopped =
                run(&["-s", &serial, "shell", "am force-stop com.reqable.android"]).is_ok();
            result.map_err(|error: String| {
                format!(
                    "{error}；{}",
                    if stopped {
                        "已退出 Reqable，请重试"
                    } else {
                        "无法确认抓包已停止，请在设备上检查 Reqable"
                    }
                )
            })
        }
    } else {
        result
    };
    // Only remove the ephemeral forwarding created by this invocation.
    let _ = run(&["-s", &serial, "wait-for-device"]);
    let _ = run(&["-s", &serial, "forward", "--remove", &local]);
    result
}

#[cfg(test)]
mod tests;
