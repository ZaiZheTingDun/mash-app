use super::super::tests::encoded_shell_output;
use super::*;
use crate::runner::LogLevel;

fn original() -> Value {
    serde_json::json!({
        "androidApps": ["other.app"], "androidAppsBypass": true,
        "vpnMode": false, "hostMode": true, "autoCapture": true,
        "proxyPort": 9123, "enhancedMode": {"isEnabled": false, "socksProxy": true},
        "futureSetting": {"text": "a'b\n$(touch /should-never-run)", "values": [1, 2]}
    })
}

#[test]
fn version_requirement_compares_numbers_and_rejects_old_or_unreadable_versions() {
    for version in ["3.3.5", "3.3.9", "3.3.10", "3.4.0", "4.0.0"] {
        assert_eq!(
            reqable_version(format!("  versionName={version}\r\n").as_bytes()).unwrap(),
            version
        );
    }
    for version in ["3.3.4", "3.2.99", "2.99.99"] {
        assert!(
            reqable_version(format!("versionName={version}\n").as_bytes())
                .unwrap_err()
                .contains("或以上版本")
        );
    }
    for info in [
        "",
        "versionName=",
        "versionName=3.3",
        "versionName=3.3.9.1",
        "versionName=unknown",
    ] {
        assert!(reqable_version(info.as_bytes()).is_err());
    }
}

#[test]
fn newer_versions_can_be_configured_and_report_the_installed_version() {
    for version in ["3.3.5", "3.3.9", "3.3.10", "3.4.0", "4.0.0"] {
        let (result, _) = fake(Scenario {
            version: Some(version),
            ..Default::default()
        });
        assert_eq!(result.unwrap().reqable_version, version);
    }
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
    assert!(new.get("pipMode").is_none());
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

#[test]
fn unverified_legacy_pip_key_is_preserved_without_becoming_a_capture_requirement() {
    for value in [
        serde_json::json!(true),
        serde_json::json!(false),
        Value::Null,
        serde_json::json!("legacy"),
    ] {
        let mut old = original();
        old["pipMode"] = value.clone();
        assert!(parse_config(&serde_json::to_vec(&old).unwrap()).is_ok());
        let configured = cn_config(old);
        assert_eq!(configured["pipMode"], value);
        assert_eq!(cn_config(configured.clone()), configured);
    }
}

#[derive(Default)]
struct Scenario {
    setup: bool,
    apex: bool,
    certificate_rollback_failure: bool,
    offline_after_install: bool,
    old_version: bool,
    version: Option<&'static str>,
    missing_game: bool,
    no_root: bool,
    requires_adb_root: bool,
    invalid_config: bool,
    write_failure: bool,
    no_marker: bool,
    launch_failure: bool,
    mismatch: bool,
    pip_write_failure: bool,
    pip_unconfirmed: bool,
    rollback_failure: bool,
    stop_disconnect: bool,
    absent_config: bool,
    cancel_onboarding: bool,
    cancel_certificate: bool,
    cancel_write: bool,
}

fn fake(scenario: Scenario) -> (Result<CaptureConfiguration, String>, Vec<String>) {
    let (result, commands, _) = fake_with_logs(scenario);
    (result, commands)
}

fn fake_with_logs(
    scenario: Scenario,
) -> (
    Result<CaptureConfiguration, String>,
    Vec<String>,
    Vec<(LogLevel, String)>,
) {
    let cancellation = OperationCancellation::default();
    let mut logs = Vec::new();
    let mut commands = Vec::new();
    let mut written = false;
    let mut pip_disabled = false;
    let mut reads = 0;
    let mut disconnected = false;
    let mut device_reads = 0;
    let mut root_ready = !scenario.requires_adb_root;
    let mut prepare = |selected: Option<&str>, run: &mut AdbCommand<'_>| {
        if scenario.setup {
            super::super::setup_controlled_using(
                selected,
                run,
                &mut |level, message| logs.push((level, message.into())),
                &cancellation,
            )
        } else {
            configure_controlled_using(selected, run, &cancellation)
        }
    };
    let result = prepare(Some("test-device"), &mut |args| {
        if args == ["devices", "-l"] {
            device_reads += 1;
            if scenario.offline_after_install && device_reads > 1 {
                return Ok(b"List of devices attached\nother-device device\n".to_vec());
            }
            return Ok(b"List of devices attached\ntest-device device\n".to_vec());
        }
        if args == ["-s", "test-device", "wait-for-device"] {
            commands.push("wait-for-device".into());
            return Ok(Vec::new());
        }
        if args == ["-s", "test-device", "root"] {
            commands.push("adb root".into());
            root_ready = !scenario.no_root;
            return Ok(if root_ready {
                b"restarting adbd as root\n".to_vec()
            } else {
                b"adbd cannot run as root in production builds\n".to_vec()
            });
        }
        assert_eq!(&args[..3], &["-s", "test-device", "shell"]);
        let command = args[3];
        commands.push(command.into());
        if command.contains("uiautomator dump") {
            if scenario.cancel_onboarding {
                cancellation.cancel();
                return Ok(include_bytes!(
                    "../../../tests/fixtures/packet-capture/reqable-license-zh-cn.xml"
                )
                .to_vec());
            }
            return Ok(br#"<hierarchy><node package="com.reqable.android" content-desc="Reqable" /></hierarchy>"#.to_vec());
        }
        if command == "getprop ro.build.version.sdk" {
            return Ok(if scenario.apex { b"34" } else { b"33" }.to_vec());
        }
        if command.contains(
            "head -c 65537 /data/user/0/com.reqable.android/files/certificate/reqable-root.crt",
        ) {
            return Ok(encoded_shell_output(include_bytes!(
                "../../../tests/fixtures/packet-capture/ca.pem"
            )));
        }
        if command.contains("MASH_CA_INSTALLED") {
            if scenario.cancel_certificate {
                cancellation.cancel();
            }
            if scenario.certificate_rollback_failure {
                return Err("证书安装失败，未能确认回滚，保留备份：/data/local/tmp/mash-capture-ca-test；请重启设备后重试".into());
            }
            return Ok(b"MASH_CA_INSTALLED".to_vec());
        }
        if command.starts_with("pm path") {
            if command.ends_with(PACKAGE) && scenario.missing_game {
                return Ok(Vec::new());
            }
            return Ok(b"package:/data/app/example/base.apk\n".to_vec());
        }
        if command.starts_with("dumpsys package") {
            let version = if scenario.old_version {
                "3.2.5"
            } else {
                scenario.version.unwrap_or("3.3.5")
            };
            return Ok(format!("versionName={version}\n").into_bytes());
        }
        if command.contains("id -u") {
            return if scenario.no_root || !root_ready {
                Err("denied".into())
            } else {
                Ok(b"0\n".to_vec())
            };
        }
        if command.contains("head -c") {
            reads += 1;
            if scenario.absent_config {
                return if reads <= 2 {
                    Ok(encoded_shell_output(b"MASH_CONFIG_ABSENT\n"))
                } else {
                    Ok(encoded_shell_output(
                        &serde_json::to_vec(&cn_config(serde_json::json!({}))).unwrap(),
                    ))
                };
            }
            if scenario.invalid_config {
                return Ok(encoded_shell_output(b"{}"));
            }
            let value = if reads > 2 && written && !scenario.mismatch {
                cn_config(original())
            } else {
                original()
            };
            return Ok(encoded_shell_output(&serde_json::to_vec(&value).unwrap()));
        }
        if command.contains("cmd appops get --user 0 com.reqable.android PICTURE_IN_PICTURE") {
            return Ok(if pip_disabled && !scenario.pip_unconfirmed {
                "PICTURE_IN_PICTURE: ignore"
            } else {
                "PICTURE_IN_PICTURE: allow"
            }
            .as_bytes()
            .to_vec());
        }
        if command.contains("cmd appops set --user 0 com.reqable.android PICTURE_IN_PICTURE ignore")
        {
            if scenario.pip_write_failure {
                return Err("denied".into());
            }
            pip_disabled = true;
            return Ok(Vec::new());
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
            if scenario.cancel_write {
                cancellation.cancel();
            }
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
    (result, commands, logs)
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
    assert_eq!(result["reqableVersion"], "3.3.5");
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
    let disable_pip = commands
        .iter()
        .position(|s| s.contains("cmd appops set"))
        .unwrap();
    assert!(write < disable_pip && disable_pip < launch);
    assert!(commands[disable_pip + 1].contains("cmd appops get"));
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
        Scenario {
            pip_write_failure: true,
            ..Default::default()
        },
        Scenario {
            pip_unconfirmed: true,
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
    let result = configure(
        &path,
        Some(&serial),
        &crate::device_operation::OperationCancellation::default(),
    )
    .unwrap();
    assert_eq!(result.package_name, PACKAGE);
    println!("{}: {}", result.serial, result.message);
}

#[test]
fn combined_setup_installs_before_configuring_and_keeps_the_original_device_pinned() {
    let (result, commands) = fake(Scenario {
        setup: true,
        ..Default::default()
    });
    assert_eq!(
        result.unwrap().message,
        "环境配置完成，请点击开始按钮（设备重启后可能需要重新配置）"
    );
    let install = commands
        .iter()
        .position(|c| c.contains("MASH_CA_INSTALLED"))
        .unwrap();
    let configure = commands
        .iter()
        .position(|c| c.contains("MASH_CONFIG_WRITTEN"))
        .unwrap();
    assert!(install < configure);
    let (result, commands) = fake(Scenario {
        setup: true,
        offline_after_install: true,
        ..Default::default()
    });
    assert!(result.unwrap_err().contains("所选 ADB 设备不在线"));
    assert!(!commands.iter().any(|c| c.contains("MASH_CONFIG_WRITTEN")));
    let (result, commands) = fake(Scenario {
        setup: true,
        no_root: true,
        ..Default::default()
    });
    assert!(result.is_err());
    assert!(!commands.iter().any(|c| c.contains("MASH_CONFIG_WRITTEN")));
}

#[test]
fn setup_prepares_adb_root_before_installing_the_certificate_and_configuring_reqable() {
    let (result, commands, logs) = fake_with_logs(Scenario {
        setup: true,
        requires_adb_root: true,
        ..Default::default()
    });
    assert!(result.is_ok());
    let root = commands
        .iter()
        .position(|command| command == "adb root")
        .unwrap();
    assert_eq!(commands[root + 1], "wait-for-device");
    assert!(commands[root + 2].contains("id -u"));
    let install = commands
        .iter()
        .position(|command| command.contains("MASH_CA_INSTALLED"))
        .unwrap();
    assert!(root + 2 < install);
    assert!(logs
        .iter()
        .any(|(_, message)| message.contains("正在尝试 adb root")));
    assert!(logs
        .iter()
        .any(|(_, message)| message == "设备 root 权限已确认"));
}

#[test]
fn setup_logs_each_stage_and_reports_failures_without_claiming_success() {
    let (result, _, logs) = fake_with_logs(Scenario {
        setup: true,
        ..Default::default()
    });
    assert!(result.is_ok());
    assert_eq!(logs.len(), 8);
    assert!(logs.iter().all(|(level, _)| *level == LogLevel::Info));
    assert_eq!(logs[0].1, "开始抓包环境配置");
    assert!(logs[1].1.contains("正在检查设备 root 权限"));
    assert_eq!(logs[2].1, "设备 root 权限已确认");
    assert!(logs[3].1.contains("安装并验证系统证书"));
    assert!(logs[4].1.contains("设备 test-device：证书已安装"));
    assert!(logs[5].1.contains("正在配置 Reqable"));
    assert_eq!(logs[6].1, "已关闭并验证 Reqable 的系统画中画权限");
    assert_eq!(
        logs[7].1,
        "环境配置完成，请点击开始按钮（设备重启后可能需要重新配置）"
    );
    for scenario in [
        Scenario {
            setup: true,
            no_root: true,
            ..Default::default()
        },
        Scenario {
            setup: true,
            offline_after_install: true,
            ..Default::default()
        },
        Scenario {
            setup: true,
            write_failure: true,
            ..Default::default()
        },
        Scenario {
            setup: true,
            pip_unconfirmed: true,
            ..Default::default()
        },
    ] {
        let (result, _, logs) = fake_with_logs(scenario);
        let error = result.unwrap_err();
        assert_eq!(
            logs.last().unwrap(),
            &(LogLevel::Warn, format!("抓包环境配置失败：{error}"))
        );
        assert!(!logs
            .iter()
            .any(|(_, message)| message.starts_with("环境配置完成")));
        if error.contains("无法取得 root") {
            assert!(!logs
                .iter()
                .any(|(_, message)| message.contains("正在配置 Reqable")));
        }
    }
}

#[test]
fn stopping_setup_interrupts_onboarding_and_certificate_stages_and_rolls_back_an_applied_write() {
    for scenario in [
        Scenario {
            setup: true,
            cancel_onboarding: true,
            ..Default::default()
        },
        Scenario {
            setup: true,
            cancel_certificate: true,
            ..Default::default()
        },
        Scenario {
            setup: true,
            cancel_write: true,
            ..Default::default()
        },
    ] {
        let wrote = scenario.cancel_write;
        let certificate = scenario.cancel_certificate;
        let (result, commands, logs) = fake_with_logs(scenario);
        assert_eq!(result.unwrap_err(), "环境配置已停止");
        assert!(!logs.iter().any(
            |(level, message)| *level == LogLevel::Warn || message.starts_with("环境配置完成")
        ));
        assert_eq!(
            commands.iter().any(|c| c.contains("MASH_CONFIG_WRITTEN")),
            wrote
        );
        assert_eq!(
            commands.iter().any(|c| c.contains("MASH_CONFIG_RESTORED")),
            wrote
        );
        if wrote {
            let write = commands
                .iter()
                .position(|c| c.contains("MASH_CONFIG_WRITTEN"))
                .unwrap();
            assert!(!commands[write + 1..]
                .iter()
                .any(|c| c.starts_with("am start")));
        } else if !certificate {
            assert!(!commands.iter().any(|c| c.contains("MASH_CA_INSTALLED")));
        }
    }
}

#[test]
fn stopping_setup_preserves_a_warning_if_rollback_cannot_be_confirmed() {
    let (result, commands, logs) = fake_with_logs(Scenario {
        setup: true,
        cancel_write: true,
        rollback_failure: true,
        ..Default::default()
    });
    assert_eq!(result.unwrap_err(), "环境配置已停止");
    assert!(commands
        .iter()
        .any(|command| command.contains("MASH_CONFIG_RESTORED")));
    assert!(logs.iter().any(|(level, message)| *level == LogLevel::Warn
        && message.contains("回滚未完成")
        && message.contains("备份位置")));
    assert!(!logs
        .iter()
        .any(|(_, message)| message.starts_with("环境配置完成")));
}

#[test]
fn stopping_apex_setup_preserves_the_certificate_rollback_warning() {
    let (result, _, logs) = fake_with_logs(Scenario {
        setup: true,
        apex: true,
        cancel_certificate: true,
        certificate_rollback_failure: true,
        ..Default::default()
    });
    assert_eq!(result.unwrap_err(), "环境配置已停止");
    assert!(logs.iter().any(|(level, message)| *level == LogLevel::Warn
        && message.contains("未能确认回滚")
        && message.contains("/data/local/tmp/mash-capture-ca-test")));
    assert!(!logs
        .iter()
        .any(|(_, message)| message.starts_with("环境配置完成")));
}
