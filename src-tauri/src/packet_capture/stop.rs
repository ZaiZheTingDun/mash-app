use super::{capture::decode_status, client, run_adb, AdbCommand};
use std::{path::Path, time::Duration};

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
