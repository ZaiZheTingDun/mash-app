use super::*;

fn vpn(ranges: &str) -> Vec<u8> {
    format!("NetworkAgentInfo{{ ni{{VPN CONNECTED extra: }} nc{{ Uids: <{{{ranges}}}> OwnerUid: 10082 }} }}").into_bytes()
}

#[test]
fn state_and_application_isolation_require_explicit_valid_values() {
    assert!(decode_status(br#"{"status":"active"}"#).unwrap());
    assert!(!decode_status(br#"{"status":"inactive"}"#).unwrap());
    for invalid in [
        b"{}".as_slice(),
        br#"{"status":"pending"}"#,
        b"HTTP/1.1 200",
    ] {
        assert!(decode_status(invalid).is_err());
    }
    assert!(vpn_only_cn(&vpn("10081-10081, 20081-20081"), 10081));
    assert!(vpn_only_cn(&vpn("10081"), 10081));
    for ranges in [
        "0-99999",
        "10081-10082",
        "10080",
        "20081",
        "10081, 10082",
        "",
    ] {
        assert!(!vpn_only_cn(&vpn(ranges), 10081));
    }
    assert!(!vpn_only_cn(b"unknown format", 10081));
    assert_eq!(package_uid(b"    userId=10081\n").unwrap(), 10081);
    assert!(package_uid(b"userId=0").is_err());
}

#[derive(Default)]
struct Scenario {
    unconfigured: bool,
    already_active: bool,
    never_active: bool,
    unavailable: bool,
    wide_vpn: bool,
    post_failed: bool,
    status_disconnect: bool,
    stop_failed: bool,
    baseline_failed: bool,
}

fn fake(s: Scenario) -> (Result<CaptureStarted, String>, Vec<String>, Vec<Request>) {
    let mut commands = Vec::new();
    let mut requests = Vec::new();
    let mut started = s.already_active;
    let mut stopped = false;
    let mut disconnected = false;
    let prepared = std::cell::Cell::new(false);
    let result = start_using(
        Some("device"),
        &mut |args| {
            if args == ["devices", "-l"] {
                return Ok(b"List of devices attached\ndevice device\n".to_vec());
            }
            assert_eq!(&args[..2], &["-s", "device"]);
            let command = args[2..].join(" ");
            commands.push(command.clone());
            match command.as_str() {
            "shell dumpsys package com.reqable.android" => Ok(b"versionName=3.3.5\n".to_vec()),
            "shell dumpsys package com.bilibili.fatego" => Ok(b"userId=10081\n".to_vec()),
            "shell am start -W -n com.reqable.android/.MainActivity" => Ok(b"Status: ok\n".to_vec()),
            "forward tcp:0 tcp:9016" => Ok(b"41001\n".to_vec()),
            "forward --remove tcp:41001" | "forward tcp:41001 tcp:9016" | "wait-for-device" | "shell am force-stop com.reqable.android" => Ok(Vec::new()),
            "shell dumpsys connectivity" => Ok(vpn(if s.wide_vpn { "0-99999" } else { "10081-10081, 20081-20081" })),
            _ if command.contains("id -u") => Ok(b"0\n".to_vec()),
            _ if command.contains("head -c") => Ok(serde_json::to_vec(&serde_json::json!({
                "androidApps": if s.unconfigured { vec!["other.app"] } else { vec!["com.bilibili.fatego"] },
                "androidAppsBypass": false, "vpnMode": true, "hostMode": false, "autoCapture": false,
            })).unwrap()),
            _ => panic!("unexpected command: {command}"),
        }
        },
        &mut |port, request| {
            assert_eq!(port, 41001);
            requests.push(request);
            if s.unavailable {
                return Err("unavailable".into());
            }
            match request {
                Request::Status => {
                    if s.status_disconnect && started && !disconnected {
                        disconnected = true;
                        return Err("connection reset".into());
                    }
                    Ok(Some(
                        (started && !s.never_active && !stopped) || (s.stop_failed && stopped),
                    ))
                }
                Request::Start => {
                    assert!(
                        prepared.get(),
                        "old response baseline must be captured before Start"
                    );
                    started = true;
                    if s.post_failed {
                        Err("connection reset after POST".into())
                    } else {
                        Ok(None)
                    }
                }
                Request::Stop => {
                    stopped = true;
                    Ok(None)
                }
            }
        },
        &mut |_| {
            if s.baseline_failed {
                return Err("baseline unavailable".into());
            }
            prepared.set(true);
            Ok(HashSet::from(["previous-response".into()]))
        },
        &mut || {},
    );
    (result, commands, requests)
}

#[test]
fn failed_baseline_read_does_not_start_capture_or_reuse_previous_account_data() {
    let (result, commands, requests) = fake(Scenario {
        baseline_failed: true,
        ..Default::default()
    });
    assert!(result.is_err());
    assert!(!requests.contains(&Request::Start));
    assert!(!requests.contains(&Request::Stop));
    assert_eq!(commands.last().unwrap(), "forward --remove tcp:41001");
}

#[test]
fn start_requires_confirmed_active_state_and_only_cn_vpn_then_removes_own_forwarding() {
    let (result, commands, requests) = fake(Scenario::default());
    let result = serde_json::to_value(result.unwrap()).unwrap();
    assert_eq!(result["alreadyActive"], false);
    assert_eq!(result["serial"], "device");
    assert_eq!(
        &requests[..3],
        &[Request::Status, Request::Start, Request::Status]
    );
    assert_eq!(commands.last().unwrap(), "forward --remove tcp:41001");
    assert!(commands.iter().any(|c| c == "shell dumpsys connectivity"));
    assert!(!requests.contains(&Request::Stop));
}

#[test]
fn existing_capture_is_verified_without_sending_another_start() {
    let (result, _, requests) = fake(Scenario {
        already_active: true,
        ..Default::default()
    });
    assert!(result.unwrap().already_active);
    assert!(!requests.contains(&Request::Start));
}

#[test]
fn wrong_configuration_never_opens_the_api_or_starts_capture() {
    let (result, commands, requests) = fake(Scenario {
        unconfigured: true,
        ..Default::default()
    });
    assert!(result.unwrap_err().contains("自动配置"));
    assert!(requests.is_empty());
    assert!(!commands.iter().any(|c| c.starts_with("forward")));
}

#[test]
fn post_acknowledgement_is_not_success_and_failed_or_uncertain_starts_are_cancelled() {
    for scenario in [
        Scenario {
            never_active: true,
            ..Default::default()
        },
        Scenario {
            post_failed: true,
            ..Default::default()
        },
        Scenario {
            wide_vpn: true,
            ..Default::default()
        },
    ] {
        let (result, commands, requests) = fake(scenario);
        assert!(result.unwrap_err().contains("抓包已关闭"));
        assert!(requests.contains(&Request::Stop));
        assert!(commands
            .iter()
            .any(|c| c == "shell am force-stop com.reqable.android"));
        assert_eq!(commands.last().unwrap(), "forward --remove tcp:41001");
    }
    let (result, commands, _) = fake(Scenario {
        already_active: true,
        wide_vpn: true,
        stop_failed: true,
        ..Default::default()
    });
    assert!(result.unwrap_err().contains("已退出 Reqable"));
    assert!(commands
        .iter()
        .any(|c| c == "shell am force-stop com.reqable.android"));
}

#[test]
fn unavailable_api_is_reported_without_sending_start_and_temporary_forward_is_cleaned() {
    let (result, commands, requests) = fake(Scenario {
        unavailable: true,
        ..Default::default()
    });
    assert!(result.unwrap_err().contains("尚未就绪"));
    assert!(!requests.contains(&Request::Start));
    assert_eq!(commands.last().unwrap(), "forward --remove tcp:41001");
}

#[test]
fn vpn_transport_disconnect_recreates_only_its_own_forward_and_polls_again() {
    let (result, commands, _) = fake(Scenario {
        status_disconnect: true,
        ..Default::default()
    });
    assert!(result.is_ok());
    assert!(commands.iter().any(|c| c == "forward tcp:41001 tcp:9016"));
}

#[test]
#[ignore = "requires MASH_CAPTURE_DEVICE and an explicitly authorized configured Android device"]
fn start_on_authorized_device() {
    let serial = std::env::var("MASH_CAPTURE_DEVICE").expect("set MASH_CAPTURE_DEVICE");
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/adb")
        .join(crate::platform::adb_executable_name());
    let result = start_capture(&path, Some(&serial)).unwrap();
    println!("{}: {}", result.serial, result.message);
}
