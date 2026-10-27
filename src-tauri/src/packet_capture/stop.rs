use super::{capture::decode_status, client, run_adb, AdbCommand};
use std::{path::Path, time::Duration};

/// Called only after the cancelled worker has released its device lease. A
/// force-stop also dismisses pending VPN consent, so late approval cannot restart it.
pub(crate) fn stop_interrupted_capture(adb_path: &Path, serial: &str) -> Result<(), String> {
    stop_interrupted_using(serial, &mut |args| run_adb(adb_path, args))
}

fn stop_interrupted_using(serial: &str, run: &mut AdbCommand<'_>) -> Result<(), String> {
    let stop = ["-s", serial, "shell", "am force-stop com.reqable.android"];
    if run(&stop).is_err() {
        run(&["-s", serial, "wait-for-device"])?;
        run(&stop).map_err(|_| "无法停止 Reqable，请检查设备连接后重试")?;
    }
    run(&["-s", serial, "wait-for-device"])?;
    let output = run(&["-s", serial, "shell", "command -v pidof >/dev/null || exit 1; if pidof com.reqable.android >/dev/null; then echo MASH_REQABLE_RUNNING; else echo MASH_REQABLE_STOPPED; fi"])?;
    if output.trim_ascii() != b"MASH_REQABLE_STOPPED" {
        return Err("无法确认 Reqable 已停止，请在设备上关闭 Reqable 后重试".into());
    }
    Ok(())
}

pub(super) fn stop_capture(adb_path: &Path, serial: &str) -> Result<(), String> {
    let client = client::Client::new()?;
    stop_using(
        serial,
        &mut |args| run_adb(adb_path, args),
        &mut |port, route| client.request(port, route, None, 4096),
        &mut || std::thread::sleep(Duration::from_millis(250)),
    )
}

fn stop_using(
    serial: &str,
    run: &mut AdbCommand<'_>,
    request: &mut dyn FnMut(u16, &str) -> Result<Vec<u8>, String>,
    pause: &mut dyn FnMut(),
) -> Result<(), String> {
    client::forwarded(serial, run, |port, run| {
        // A successful POST only queues the stop. Even a lost POST response
        // must be followed by status verification before reporting success.
        let _ = request(port, "off");
        for _ in 0..12 {
            match request(port, "status").and_then(|bytes| decode_status(&bytes)) {
                Ok(false) => return Ok(()),
                Ok(true) => {}
                Err(_) => {
                    // VPN teardown can briefly drop the emulator's ADB forward.
                    let _ = run(&["-s", serial, "forward", &format!("tcp:{port}"), "tcp:9016"]);
                }
            }
            pause();
        }
        Err("无法确认 Reqable 抓包已停止，请在设备上手动关闭抓包".into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_task_stop_keeps_the_same_device_and_requires_confirmed_process_exit() {
        for disconnected in [false, true] {
            for running in [false, true] {
                let mut commands = Vec::new();
                let mut attempts = 0;
                let result = stop_interrupted_using("pinned", &mut |args| {
                    assert_eq!(&args[..2], &["-s", "pinned"]);
                    commands.push(args.join(" "));
                    if args.last() == Some(&"am force-stop com.reqable.android") {
                        attempts += 1;
                        if disconnected && attempts == 1 {
                            return Err("offline".into());
                        }
                    }
                    Ok(if args
                        .last()
                        .is_some_and(|arg| arg.contains("MASH_REQABLE_STOPPED"))
                    {
                        if running {
                            "MASH_REQABLE_RUNNING"
                        } else {
                            "MASH_REQABLE_STOPPED"
                        }
                    } else {
                        ""
                    }
                    .as_bytes()
                    .to_vec())
                });
                assert_eq!(result.is_err(), running);
                assert_eq!(attempts, if disconnected { 2 } else { 1 });
                assert!(commands
                    .last()
                    .unwrap()
                    .contains("pidof com.reqable.android"));
            }
        }
    }

    #[test]
    fn stop_waits_for_inactive_and_restores_its_forward_after_transport_loss() {
        for lost_post in [false, true] {
            let mut commands = Vec::new();
            let mut routes = Vec::new();
            let mut status = 0;
            let mut pauses = 0;
            stop_using(
                "device",
                &mut |args| {
                    commands.push(args.join(" "));
                    Ok(b"12345\n".to_vec())
                },
                &mut |port, route| {
                    assert_eq!(port, 12345);
                    routes.push(route.to_string());
                    if route == "off" {
                        return if lost_post {
                            Err("disconnected".into())
                        } else {
                            Ok(vec![])
                        };
                    }
                    status += 1;
                    match status {
                        1 => Ok(br#"{"status":"active"}"#.to_vec()),
                        2 => Err("disconnected".into()),
                        _ => Ok(br#"{"status":"inactive"}"#.to_vec()),
                    }
                },
                &mut || pauses += 1,
            )
            .unwrap();
            assert_eq!(routes, ["off", "status", "status", "status"]);
            assert_eq!(pauses, 2);
            assert_eq!(
                commands,
                [
                    "-s device forward tcp:0 tcp:9016",
                    "-s device forward tcp:12345 tcp:9016",
                    "-s device forward --remove tcp:12345",
                ]
            );
        }
    }

    #[test]
    fn successful_post_never_counts_as_a_confirmed_stop_and_cleanup_runs_on_failure() {
        for status in [br#"{"status":"active"}"#.as_slice(), b"invalid"] {
            let mut commands = Vec::new();
            let mut polls = 0;
            let error = stop_using(
                "device",
                &mut |args| {
                    commands.push(args.join(" "));
                    Ok(b"12345".to_vec())
                },
                &mut |_, route| {
                    if route == "status" {
                        polls += 1;
                    }
                    Ok(status.to_vec())
                },
                &mut || {},
            )
            .unwrap_err();
            assert!(error.contains("手动关闭抓包"));
            assert_eq!(polls, 12);
            assert_eq!(
                commands.last().unwrap(),
                "-s device forward --remove tcp:12345"
            );
        }
    }
}
