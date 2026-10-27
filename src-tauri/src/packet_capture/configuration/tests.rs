use super::*;

fn original() -> Value {
    serde_json::json!({
        "androidApps": ["other.app"], "androidAppsBypass": true,
        "vpnMode": false, "hostMode": true, "autoCapture": true,
        "proxyPort": 9123, "enhancedMode": {"isEnabled": false, "socksProxy": true},
        "futureSetting": {"text": "a'b\n$(touch /should-never-run)", "values": [1, 2]}
    })
}

#[test]
fn cn_configuration_preserves_other_settings_and_disables_automatic_capture() {
    let old = original();
    let new = cn_config(old.clone());
    assert_eq!(new["androidApps"], serde_json::json!([PACKAGE]));
    assert_eq!(new["androidAppsBypass"], false);
    assert_eq!(new["vpnMode"], true);
    assert_eq!(new["hostMode"], false);
    assert_eq!(new["autoCapture"], false);
    for field in ["proxyPort", "enhancedMode", "futureSetting"] {
        assert_eq!(new[field], old[field]);
    }
    assert_eq!(cn_config(new.clone()), new);
}

#[test]
fn malformed_unknown_or_oversized_configuration_is_rejected() {
    assert!(parse_config(b"invalid").is_err());
    assert!(parse_config(b"[]").is_err());
    assert!(parse_config(&vec![b' '; MAX_CONFIG_SIZE + 1]).is_err());
    for field in [
        "androidApps",
        "vpnMode",
        "autoCapture",
        "hostMode",
        "androidAppsBypass",
    ] {
        let mut config = original();
        config[field] = serde_json::Value::Null;
        assert!(parse_config(&serde_json::to_vec(&config).unwrap()).is_err());
    }
}

#[derive(Default)]
struct Scenario {
    old_version: bool,
    missing_game: bool,
    no_root: bool,
    invalid_config: bool,
    write_failure: bool,
    no_marker: bool,
    launch_failure: bool,
    mismatch: bool,
    rollback_failure: bool,
    stop_disconnect: bool,
    absent_config: bool,
}

fn fake(scenario: Scenario) -> (Result<CaptureConfiguration, String>, Vec<String>) {
    let mut commands = Vec::new();
    let mut written = false;
    let mut reads = 0;
    let mut disconnected = false;
    let result = configure_using(Some("test-device"), &mut |args| {
        if args == ["devices", "-l"] {
            return Ok(b"List of devices attached\ntest-device device\n".to_vec());
        }
        if args == ["-s", "test-device", "wait-for-device"] {
            commands.push("wait-for-device".into());
            return Ok(Vec::new());
        }
        assert_eq!(&args[..3], &["-s", "test-device", "shell"]);
        let command = args[3];
        commands.push(command.into());
        if command.starts_with("pm path") {
            if command.ends_with(PACKAGE) && scenario.missing_game {
                return Ok(Vec::new());
            }
            return Ok(b"package:/data/app/example/base.apk\n".to_vec());
        }
        if command.starts_with("dumpsys package") {
            return Ok(if scenario.old_version {
                b"versionName=3.2.5\n"
            } else {
                b"versionName=3.3.5\n"
            }
            .to_vec());
        }
        if command.contains("id -u") {
            return if scenario.no_root {
                Err("denied".into())
            } else {
                Ok(b"0\n".to_vec())
            };
        }
        if command.contains("head -c") {
            reads += 1;
            if scenario.absent_config {
                return if reads <= 2 {
                    Ok(b"MASH_CONFIG_ABSENT\n".to_vec())
                } else {
                    Ok(serde_json::to_vec(&cn_config(serde_json::json!({}))).unwrap())
                };
            }
            if scenario.invalid_config {
                return Ok(b"{}".to_vec());
            }
            let value = if reads > 2 && written && !scenario.mismatch {
                cn_config(original())
            } else {
                original()
            };
            return Ok(serde_json::to_vec(&value).unwrap());
        }
        if command == "am force-stop com.reqable.android" {
            if scenario.stop_disconnect && !disconnected {
                disconnected = true;
                return Err("device offline".into());
            }
            return Ok(Vec::new());
        }
        if command.contains("MASH_CONFIG_WRITTEN") {
            if scenario.absent_config {
                assert!(command.contains("test ! -e \"$p\""));
                assert!(command.contains("chown"));
            } else {
                assert!(command.contains(&hash(&serde_json::to_vec(&original()).unwrap())));
                assert!(
                    command.contains(&hash(&serde_json::to_vec(&cn_config(original())).unwrap()))
                );
                assert!(command.contains("cp -p"));
            }
            assert!(command.contains("mv -f"));
            assert!(command.contains("pidof com.reqable.android"));
            written = true;
            return if scenario.write_failure {
                Err("failed".into())
            } else if scenario.no_marker {
                Ok(Vec::new())
            } else {
                Ok(b"MASH_CONFIG_WRITTEN\n".to_vec())
            };
        }
        if command.starts_with("am start") {
            return Ok(if scenario.launch_failure {
                b"Error: Activity not started\n".to_vec()
            } else {
                b"Status: ok\nActivity: com.reqable.android/.MainActivity\n".to_vec()
            });
        }
        if command.contains("MASH_CONFIG_RESTORED") {
            return if scenario.rollback_failure {
                Err("denied".into())
            } else {
                Ok(b"MASH_CONFIG_RESTORED\n".to_vec())
            };
        }
        if command.contains("rm -f") {
            return Ok(Vec::new());
        }
        panic!("unexpected command: {command}");
    });
    (result, commands)
}

#[test]
fn first_configuration_uses_reqable_defaults_and_can_restore_an_absent_file() {
    assert!(fake(Scenario {
        absent_config: true,
        ..Default::default()
    })
    .0
    .is_ok());
    let (result, commands) = fake(Scenario {
        absent_config: true,
        write_failure: true,
        ..Default::default()
    });
    assert!(result.unwrap_err().contains("已恢复原配置"));
    assert!(commands
        .iter()
        .any(|s| s.contains("MASH_CONFIG_RESTORED") && s.contains(&format!("test ! -e {CONFIG}"))));
}

#[test]
fn vpn_transport_disconnect_during_stop_waits_for_the_same_device_and_retries() {
    let (result, commands) = fake(Scenario {
        stop_disconnect: true,
        ..Default::default()
    });
    assert!(result.is_ok());
    let stop = commands
        .iter()
        .position(|s| s.contains("force-stop"))
        .unwrap();
    assert_eq!(commands[stop + 1], "wait-for-device");
    assert_eq!(commands[stop + 2], "am force-stop com.reqable.android");
}

#[test]
fn configuration_checks_prerequisites_before_stopping_or_writing() {
    for scenario in [
        Scenario {
            old_version: true,
            ..Default::default()
        },
        Scenario {
            missing_game: true,
            ..Default::default()
        },
        Scenario {
            no_root: true,
            ..Default::default()
        },
        Scenario {
            invalid_config: true,
            ..Default::default()
        },
    ] {
        let (result, commands) = fake(scenario);
        assert!(result.is_err());
        assert!(!commands.iter().any(
            |command| command.contains("force-stop") || command.contains("MASH_CONFIG_WRITTEN")
        ));
    }
}

#[test]
fn success_requires_restart_and_matching_readback_without_starting_capture() {
    let (result, commands) = fake(Scenario::default());
    let result = serde_json::to_value(result.unwrap()).unwrap();
    assert_eq!(result["serial"], "test-device");
    assert_eq!(result["packageName"], PACKAGE);
    assert_eq!(result["reqableVersion"], VERSION);
    let stop = commands
        .iter()
        .position(|s| s.contains("force-stop"))
        .unwrap();
    let write = commands
        .iter()
        .position(|s| s.contains("MASH_CONFIG_WRITTEN"))
        .unwrap();
    let launch = commands
        .iter()
        .position(|s| s.starts_with("am start"))
        .unwrap();
    assert!(stop < write && write < launch);
    assert!(commands[launch + 1].contains("head -c"));
    assert!(!commands
        .iter()
        .any(|s| s.contains("/capture/live/on") || s.contains("startservice")));
}

#[test]
fn failed_write_restart_or_readback_rolls_back_and_reports_uncertain_recovery() {
    for scenario in [
        Scenario {
            write_failure: true,
            ..Default::default()
        },
        Scenario {
            no_marker: true,
            ..Default::default()
        },
        Scenario {
            launch_failure: true,
            ..Default::default()
        },
        Scenario {
            mismatch: true,
            ..Default::default()
        },
    ] {
        let (result, commands) = fake(scenario);
        assert!(result.unwrap_err().contains("已恢复原配置"));
        assert!(commands.iter().any(|s| s.contains("MASH_CONFIG_RESTORED")));
    }
    let (result, _) = fake(Scenario {
        mismatch: true,
        rollback_failure: true,
        ..Default::default()
    });
    assert!(result.unwrap_err().contains("未能确认回滚"));
}

#[test]
#[ignore = "requires MASH_CAPTURE_DEVICE and an explicitly authorized rooted Android device"]
fn configure_on_authorized_device() {
    let serial = std::env::var("MASH_CAPTURE_DEVICE").expect("set MASH_CAPTURE_DEVICE");
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/adb")
        .join(crate::platform::adb_executable_name());
    let result = configure(&path, Some(&serial)).unwrap();
    assert_eq!(result.package_name, PACKAGE);
    println!("{}: {}", result.serial, result.message);
}
