use super::{client, configuration, response, run_adb, select_serial, AdbCommand};
use crate::device_operation::OperationCancellation;
use std::{collections::HashSet, path::Path, time::Duration};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureStarted {
    pub(super) serial: String,
    already_active: bool,
    message: String,
    #[serde(skip)]
    pub(super) baseline: HashSet<String>,
}

impl CaptureStarted {
    pub(crate) fn begin_response_watch(&self, store: &response::ResponseStore) {
        store.begin(self.serial.clone(), self.baseline.clone());
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
    adb_path: &Path,
    selected: Option<&str>,
    cancellation: &OperationCancellation,
) -> Result<CaptureStarted, String> {
    let client = client::Client::new()?;
    start_controlled_using(
        selected,
        &mut |args| run_adb(adb_path, args),
        &mut |port, request| {
            let route = match request {
                Request::Status => "status",
                Request::Start => "on",
                Request::Stop => "off",
            };
            let bytes = client.request(port, route, None, 4096)?;
            if request == Request::Status {
                Ok(Some(decode_status(&bytes)?))
            } else {
                Ok(None)
            }
        },
        &mut |port| response::baseline(&client, port),
        &mut || cancellation.wait(Duration::from_millis(250)),
        cancellation,
    )
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
        .ok_or_else(|| "未找到国服 FGO，请先安装国服游戏".into())
}

// Android 13 also assigns the application's SDK sandbox UID (app UID + 10000).
// Reject broad ranges or unknown dump formats rather than claiming app isolation.
fn vpn_only_cn(output: &[u8], uid: u32) -> bool {
    let text = String::from_utf8_lossy(output);
    let mut vpns = text
        .lines()
        .filter(|line| line.contains("ni{VPN CONNECTED"));
    let Some(line) = vpns.next() else {
        return false;
    };
    if vpns.next().is_some() {
        return false;
    }
    let Some(ranges) = line
        .split_once("Uids: <{")
        .and_then(|(_, rest)| rest.split_once("}>"))
    else {
        return false;
    };
    let mut includes_game = false;
    for range in ranges.0.split(',') {
        let range = range.trim();
        let (start, end) = range.split_once('-').unwrap_or((range, range));
        let (Ok(start), Ok(end)) = (start.parse::<u32>(), end.parse::<u32>()) else {
            return false;
        };
        if start != end || (start != uid && start != uid + 10000) {
            return false;
        }
        includes_game |= start == uid;
    }
    includes_game
}

fn start_controlled_using(
    selected: Option<&str>,
    run: &mut AdbCommand<'_>,
    api: &mut Api<'_>,
    prepare: &mut dyn FnMut(u16) -> Result<HashSet<String>, String>,
    pause: &mut dyn FnMut(),
    cancellation: &OperationCancellation,
) -> Result<CaptureStarted, String> {
    cancellation.check()?;
    let mut forward = |args: &[&str]| {
        cancellation.check()?;
        run(args)
    };
    let devices = forward(&["devices", "-l"])?;
    let serial = select_serial(&String::from_utf8_lossy(&devices), selected)?;
    cancellation.bind_device(&serial)?;
    configuration::prepare_capture_configuration(&mut |command| {
        forward(&["-s", &serial, "shell", command])
    })?;
    let uid = package_uid(&forward(&[
        "-s",
        &serial,
        "shell",
        "dumpsys package com.bilibili.fatego",
    ])?)?;
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
                    let vpn = forward(&["-s", &serial, "shell", "dumpsys connectivity"])?;
                    cancellation.check()?;
                    if !vpn_only_cn(&vpn, uid) {
                        return Err("无法确认仅抓国服，请重新进行环境配置后重试".into());
                    }
                    return Ok(CaptureStarted {
                        serial: serial.clone(),
                        already_active,
                        baseline,
                        message: if already_active {
                            "国服抓包已在运行"
                        } else {
                            "已开始抓取国服 FGO 流量"
                        }
                        .into(),
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
