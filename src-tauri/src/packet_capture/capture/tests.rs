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
    pip_allowed: bool,
    pip_set_failure: bool,
    pip_unconfirmed: bool,
    pip_read_failure: bool,
    legacy_pip_key: bool,
    already_active: bool,
    never_active: bool,
    unavailable: bool,
    wide_vpn: bool,
    post_failed: bool,
    status_disconnect: bool,
    stop_failed: bool,
    baseline_failed: bool,
    cancel_after_start: bool,
    cancel_during_baseline: bool,
    cancel_while_waiting: bool,
}

fn fake(s: Scenario) -> (Result<CaptureStarted, String>, Vec<String>, Vec<Request>) {
    fake_with_cancellation(s, &OperationCancellation::default())
}

fn fake_with_cancellation(
    s: Scenario,
    cancellation: &OperationCancellation,
) -> (Result<CaptureStarted, String>, Vec<String>, Vec<Request>) {
    let mut commands = Vec::new();
    let mut requests = Vec::new();
    let mut started = s.already_active;
    let mut stopped = false;
    let mut disconnected = false;
    let mut pip_disabled = !s.pip_allowed;
    let prepared = std::cell::Cell::new(false);
    let result = start_controlled_using(
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
                "shell am start -W -n com.reqable.android/.MainActivity" => {
                    Ok(b"Status: ok\n".to_vec())
                }
                "forward tcp:0 tcp:9016" => Ok(b"41001\n".to_vec()),
                "forward --remove tcp:41001"
                | "forward tcp:41001 tcp:9016"
                | "wait-for-device"
                | "shell am force-stop com.reqable.android" => Ok(Vec::new()),
                "shell dumpsys connectivity" => Ok(vpn(if s.wide_vpn {
                    "0-99999"
                } else {
                    "10081-10081, 20081-20081"
                })),
                _ if command
                    .contains("cmd appops get --user 0 com.reqable.android PICTURE_IN_PICTURE") =>
                {
                    if s.pip_read_failure {
                        return Err("disconnected".into());
                    }
                    Ok(if pip_disabled && !s.pip_unconfirmed {
                        "PICTURE_IN_PICTURE: ignore"
                    } else {
                        "PICTURE_IN_PICTURE: allow"
                    }
                    .as_bytes()
                    .to_vec())
                }
                _ if command.contains(
                    "cmd appops set --user 0 com.reqable.android PICTURE_IN_PICTURE ignore",
                ) =>
                {
                    if s.pip_set_failure {
                        return Err("denied".into());
                    }
                    pip_disabled = true;
                    Ok(Vec::new())
                }
                _ if command.contains("id -u") => Ok(b"0\n".to_vec()),
                _ if command.contains("head -c") => {
                    let mut config = serde_json::json!({
                    "androidApps": if s.unconfigured { vec!["other.app"] } else { vec!["com.bilibili.fatego"] },
                    "androidAppsBypass": false, "vpnMode": true, "hostMode": false, "autoCapture": false,
                    });
                    if s.legacy_pip_key {
                        config["pipMode"] = serde_json::json!(true);
                    }
                    Ok(serde_json::to_vec(&config).unwrap())
                }
                _ => panic!("unexpected command: {command}"),
            }
        },
        &mut |port, request| {
            assert_eq!(port, 41001);
            requests.push(request);
            if s.cancel_while_waiting {
                cancellation.cancel();
                return Err("waiting for API".into());
            }
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
                    if s.cancel_after_start {
                        cancellation.cancel();
                    }
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
            if s.cancel_during_baseline {
                cancellation.cancel();
            }
            if s.baseline_failed {
                return Err("baseline unavailable".into());
            }
            prepared.set(true);
            Ok(HashSet::from(["previous-response".into()]))
        },
        &mut || {},
        cancellation,
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
    assert!(result.unwrap_err().contains("环境配置"));
    assert!(requests.is_empty());
    assert!(!commands
        .iter()
        .any(|c| c.starts_with("forward") || c.contains("appops")));
}

#[test]
fn start_disables_system_pip_before_launch_and_accepts_legacy_config_without_reconfiguration() {
    let (result, commands, requests) = fake(Scenario {
        pip_allowed: true,
        legacy_pip_key: true,
        ..Default::default()
    });
    assert!(result.is_ok());
    let set = commands
        .iter()
        .position(|c| c.contains("cmd appops set"))
        .unwrap();
    let launch = commands
        .iter()
        .position(|c| c.contains("am start -W"))
        .unwrap();
    assert!(set < launch);
    assert!(commands[set + 1].contains("cmd appops get"));
    assert_eq!(
        commands
            .iter()
            .filter(|c| c.contains("cmd appops set"))
            .count(),
        1
    );
    assert!(requests.contains(&Request::Start));
}

#[test]
fn failed_or_unconfirmed_system_pip_change_does_not_launch_or_start_capture() {
    for scenario in [
        Scenario {
            pip_read_failure: true,
            ..Default::default()
        },
        Scenario {
            pip_allowed: true,
            pip_set_failure: true,
            ..Default::default()
        },
        Scenario {
            pip_unconfirmed: true,
            ..Default::default()
        },
    ] {
        let (result, commands, requests) = fake(scenario);
        assert!(result.unwrap_err().contains("系统画中画权限"));
        assert!(requests.is_empty());
        assert!(!commands
            .iter()
            .any(|c| c.starts_with("forward") || c.contains("am start -W")));
    }
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
    let result = start_capture(
        &path,
        Some(&serial),
        &crate::device_operation::OperationCancellation::default(),
    )
    .unwrap();
    println!("{}: {}", result.serial, result.message);
}

#[test]
fn stopping_startup_interrupts_api_waits_and_baseline_and_prevents_late_confirmation() {
    for scenario in [
        Scenario {
            cancel_while_waiting: true,
            ..Default::default()
        },
        Scenario {
            cancel_during_baseline: true,
            ..Default::default()
        },
        Scenario {
            cancel_after_start: true,
            ..Default::default()
        },
    ] {
        let posted = scenario.cancel_after_start;
        let cancellation = OperationCancellation::default();
        let (result, commands, requests) = fake_with_cancellation(scenario, &cancellation);
        assert!(result.is_err());
        assert!(cancellation.is_cancelled());
        assert_eq!(cancellation.device().as_deref(), Some("device"));
        assert_eq!(requests.contains(&Request::Start), posted);
        assert!(!commands.iter().any(|c| c == "shell dumpsys connectivity"));
        assert_eq!(commands.last().unwrap(), "forward --remove tcp:41001");
        if posted {
            assert_eq!(requests.last(), Some(&Request::Start));
        }
    }
}
