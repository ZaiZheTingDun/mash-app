use super::super::tests::encoded_shell_output;
use super::*;
use std::collections::HashSet;

#[test]
fn channel_capture_uses_the_chosen_package_and_rejects_another_clients_config() {
    let (result, commands, _) = fake(Scenario {
        package: Some("com.bilibili.fgo.mi"),
        ..Default::default()
    });
    assert!(result.is_ok());
    assert!(commands
        .iter()
        .any(|c| c == "shell dumpsys package com.bilibili.fgo.mi"));
    assert!(!commands.iter().any(|c| c.contains("com.bilibili.fatego")));
    let (result, _, requests) = fake(Scenario {
        package: Some("com.bilibili.fgo.mi"),
        configured_package: Some("com.bilibili.fatego"),
        ..Default::default()
    });
    assert!(result.unwrap_err().contains("未覆盖所选客户端"));
    assert!(!requests.contains(&Request::Start));
}

#[test]
fn jp_capture_checks_the_selected_package_and_pins_the_response_watch_server() {
    let (result, commands, _) = fake(Scenario {
        server: Some(Server::Jp),
        ..Default::default()
    });
    let result = result.unwrap();
    assert_eq!(result.server, Server::Jp);
    assert!(result.message.contains("日服"));
    assert!(commands
        .iter()
        .any(|c| c == "shell dumpsys package com.aniplex.fategrandorder"));
    assert!(!commands.iter().any(|c| c.contains("com.bilibili.fatego")));
    let store = response::ResponseStore::default();
    result.begin_response_watch(&store);
    assert!(!serde_json::to_string(&result).unwrap().contains("server"));
}

#[test]
fn changing_servers_requires_matching_configuration_and_vpn_isolation() {
    for (server, configured_server) in [(Server::Cn, Server::Jp), (Server::Jp, Server::Cn)] {
        let (result, _, requests) = fake(Scenario {
            server: Some(server),
            configured_server: Some(configured_server),
            ..Default::default()
        });
        assert!(result.unwrap_err().contains("未覆盖所选客户端"));
        assert!(!requests.contains(&Request::Start));
    }
    let (result, _, requests) = fake(Scenario {
        server: Some(Server::Jp),
        missing_game: true,
        ..Default::default()
    });
    assert!(result.is_ok());
    assert!(requests.contains(&Request::Start));
    let (result, _, requests) = fake(Scenario {
        server: Some(Server::Jp),
        excluded_vpn: true,
        ..Default::default()
    });
    assert!(result.unwrap_err().contains("未覆盖所选客户端"));
    assert!(requests.contains(&Request::Stop));
}

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
    assert!(vpn_only_game(&vpn("10081-10081, 20081-20081"), 10081));
    assert!(vpn_only_game(&vpn("10081"), 10081));
    for ranges in [
        "0-99999",
        "10081-10082",
        "10080",
        "20081",
        "10081, 10082",
        "",
    ] {
        assert!(!vpn_only_game(&vpn(ranges), 10081));
    }
    assert!(!vpn_only_game(b"unknown format", 10081));
    assert_eq!(package_uid(b"    userId=10081\n").unwrap(), 10081);
    assert!(package_uid(b"userId=0").is_err());
}

#[derive(Default)]
struct Scenario {
    package: Option<&'static str>,
    configured_package: Option<&'static str>,
    server: Option<Server>,
    configured_server: Option<Server>,
    missing_game: bool,
    auth_failure: bool,
    auth_cancel: bool,
    version: Option<&'static str>,
    app_id_field: bool,
    unconfigured: bool,
    pip_allowed: bool,
    pip_set_failure: bool,
    pip_unconfirmed: bool,
    pip_read_failure: bool,
    legacy_pip_key: bool,
    already_active: bool,
    never_active: bool,
    unavailable: bool,
    api_error: Option<&'static str>,
    wide_vpn: bool,
    excluded_vpn: bool,
    unknown_vpn: bool,
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
    let server = s.server.unwrap_or(Server::Cn);
    let package = s.package.unwrap_or(target::package(server));
    let mut commands = Vec::new();
    let mut requests = Vec::new();
    let mut started = s.already_active;
    let mut stopped = false;
    let mut disconnected = false;
    let mut pip_disabled = !s.pip_allowed;
    let prepared = std::cell::Cell::new(false);
    let result = start_controlled_using(
        server,
        package,
        Some("device"),
        &mut |args| {
            if args == ["devices", "-l"] {
                return Ok(b"List of devices attached\ndevice device\n".to_vec());
            }
            assert_eq!(&args[..2], &["-s", "device"]);
            let command = args[2..].join(" ");
            commands.push(command.clone());
            match command.as_str() {
                "shell dumpsys package com.reqable.android" => Ok(format!(
                    "userId=10082\nversionName={}\n",
                    s.version.unwrap_or("3.3.5")
                )
                .into_bytes()),
                _ if command == format!("shell dumpsys package {package}") => {
                    Ok(if s.missing_game {
                        Vec::new()
                    } else if s.app_id_field {
                        b"appId=10081\r\n".to_vec()
                    } else {
                        b"userId=10081\n".to_vec()
                    })
                }
                "shell am start -W -n com.reqable.android/.MainActivity" => {
                    Ok(b"Status: ok\n".to_vec())
                }
                "forward tcp:0 tcp:9016" => Ok(b"41001\n".to_vec()),
                "forward --remove tcp:41001"
                | "forward tcp:41001 tcp:9016"
                | "wait-for-device"
                | "shell am force-stop com.reqable.android" => Ok(Vec::new()),
                "shell dumpsys connectivity" if s.unknown_vpn => {
                    Ok(b"unknown dump format".to_vec())
                }
                "shell dumpsys connectivity" => Ok(vpn(if s.excluded_vpn {
                    "10080"
                } else if s.wide_vpn {
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
                    "androidApps": if s.unconfigured { vec!["other.app"] } else { vec![s.configured_package.unwrap_or_else(|| s.configured_server.map(target::package).unwrap_or(package))] },
                    "androidAppsBypass": false, "vpnMode": true, "hostMode": false, "autoCapture": false,
                    });
                    if s.legacy_pip_key {
                        config["pipMode"] = serde_json::json!(true);
                    }
                    Ok(encoded_shell_output(&serde_json::to_vec(&config).unwrap()))
                }
                _ => panic!("unexpected command: {command}"),
            }
        },
        &mut |serial, port, version| {
            assert_eq!(serial, "device");
            assert_eq!(port, 41001);
            assert_eq!(version, s.version.unwrap_or("3.3.5"));
            if s.auth_cancel {
                cancellation.cancel();
            }
            if s.auth_failure {
                Err("等待 API key 超时".into())
            } else {
                Ok(())
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
            if let Some(error) = s.api_error {
                return Err(error.into());
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
            Ok(HashSet::from(["previous-response".into()]).into())
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
fn android_15_app_id_is_recognized_and_can_start_capture() {
    let info = b"  Package [com.bilibili.fatego] (5552338):\r\n    appId=10051\r\n    versionName=2.129.0\r\n";
    assert_eq!(package_uid(info).unwrap(), 10051);
    assert_eq!(package_uid(b"    appId=10051 flags=0\n").unwrap(), 10051);
    for invalid in [
        b"appId=0".as_slice(),
        b"appId=20000",
        b"appId=unknown",
        b"appId=",
        b"otherAppId=10051",
    ] {
        assert!(package_uid(invalid).is_err());
    }
    assert!(fake(Scenario {
        version: Some("3.3.9"),
        app_id_field: true,
        ..Default::default()
    })
    .0
    .is_ok());
}

#[test]
fn api_key_failure_or_cancel_never_starts_capture_and_removes_the_forward() {
    for scenario in [
        Scenario {
            version: Some("3.3.9"),
            auth_failure: true,
            ..Default::default()
        },
        Scenario {
            version: Some("3.3.10"),
            auth_cancel: true,
            ..Default::default()
        },
    ] {
        let (result, commands, requests) = fake(scenario);
        assert!(result.is_err());
        assert!(requests.is_empty());
        assert_eq!(commands.last().unwrap(), "forward --remove tcp:41001");
    }
}

#[test]
fn capture_accepts_newer_versions_and_rejects_versions_below_the_minimum() {
    for version in ["3.3.5", "3.3.9", "3.3.10", "3.4.0", "4.0.0"] {
        assert!(fake(Scenario {
            version: Some(version),
            ..Default::default()
        })
        .0
        .is_ok());
    }
    let (result, commands, requests) = fake(Scenario {
        version: Some("3.3.4"),
        ..Default::default()
    });
    assert!(result.unwrap_err().contains("或以上版本"));
    assert!(requests.is_empty());
    assert!(!commands
        .iter()
        .any(|command| command.starts_with("forward ")));
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
fn failed_or_unconfirmed_system_pip_change_warns_but_allows_capture() {
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
        let (result, _, requests) = fake(scenario);
        assert!(result
            .unwrap()
            .warnings
            .iter()
            .any(|warning| warning.contains("系统画中画权限")));
        assert!(requests.contains(&Request::Start));
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
            excluded_vpn: true,
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
        excluded_vpn: true,
        stop_failed: true,
        ..Default::default()
    });
    assert!(result.unwrap_err().contains("已退出 Reqable"));
    assert!(commands
        .iter()
        .any(|c| c == "shell am force-stop com.reqable.android"));
}

#[test]
fn api_configuration_errors_are_not_hidden_by_the_startup_timeout() {
    let (result, commands, requests) = fake(Scenario {
        api_error: Some("Reqable API 服务未启用，请在 Reqable 设置中开启 API 服务后重试"),
        ..Default::default()
    });
    assert!(result.unwrap_err().contains("API 服务未启用"));
    assert!(!requests.contains(&Request::Start));
    assert_eq!(commands.last().unwrap(), "forward --remove tcp:41001");
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
        Server::Cn,
        target::package(Server::Cn),
        &path,
        Some(&serial),
        &crate::device_operation::OperationCancellation::default(),
        &ApiKeyStore::new(tempfile::tempdir().unwrap().path().join("api-keys.json")),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/scrcpy/scrcpy-server.jar"),
        &credentials::AuthorizationStore::default(),
        &mut |_| {},
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

#[test]
fn wider_or_unreadable_vpn_ranges_warn_without_stopping_capture() {
    for scenario in [
        Scenario {
            wide_vpn: true,
            ..Default::default()
        },
        Scenario {
            unknown_vpn: true,
            ..Default::default()
        },
        Scenario {
            missing_game: true,
            ..Default::default()
        },
    ] {
        let (result, _, requests) = fake(scenario);
        assert!(!result.unwrap().warnings.is_empty());
        assert!(!requests.contains(&Request::Stop));
    }
    assert_eq!(vpn_scope(&vpn("10080"), 10081, None), VpnScope::Unknown);
    assert_eq!(
        vpn_scope(&vpn("10080"), 10081, Some(99999)),
        VpnScope::Unknown
    );
}
