use super::*;

#[test]
fn existing_root_or_su_is_reused_without_restarting_adb() {
    for prefix in ["", "su -c", "su 0 sh -c", "/system/xbin/bstk/su -c"] {
        let mut commands = Vec::new();
        let root = prepare_root_using(
            "chosen",
            &mut |args| {
                assert_eq!(&args[..3], &["-s", "chosen", "shell"]);
                commands.push(args[3].to_string());
                Ok(if args[3] == root_shell(prefix, "id -u") {
                    b"0\r\n".as_slice()
                } else {
                    b"2000"
                }
                .to_vec())
            },
            &mut |_, _| {},
            &OperationCancellation::default(),
        )
        .unwrap();
        assert_eq!(root, prefix);
        assert!(!commands.is_empty());
    }
}

#[test]
fn adb_root_reconnects_the_same_device_and_requires_a_verified_uid_even_if_the_reply_is_lost() {
    for lost_reply in [false, true] {
        let mut requested = false;
        let mut reconnected = false;
        let mut commands = Vec::new();
        let mut logs = Vec::new();
        let root = prepare_root_using(
            "chosen",
            &mut |args| {
                assert_eq!(&args[..2], &["-s", "chosen"]);
                commands.push(args[2].to_string());
                match args[2] {
                    "root" => {
                        requested = true;
                        if lost_reply {
                            Err("connection reset".into())
                        } else {
                            Ok(b"restarting adbd as root\n".to_vec())
                        }
                    }
                    "wait-for-device" => {
                        assert!(requested);
                        reconnected = true;
                        Ok(Vec::new())
                    }
                    "shell" => {
                        if requested {
                            assert!(reconnected);
                        }
                        Ok(if reconnected {
                            b"0".as_slice()
                        } else {
                            b"2000"
                        }
                        .to_vec())
                    }
                    _ => panic!("must never select a different device"),
                }
            },
            &mut |_, message| logs.push(message.to_string()),
            &OperationCancellation::default(),
        )
        .unwrap();
        assert_eq!(root, "");
        assert_eq!(
            commands.iter().filter(|command| *command == "root").count(),
            1
        );
        assert_eq!(&commands[4..], &["root", "wait-for-device", "shell"]);
        assert_eq!(logs.last().unwrap(), "设备 root 权限已确认");
    }
}

#[test]
fn a_successful_root_command_is_not_enough_when_the_uid_stays_unprivileged() {
    let mut verified = false;
    let error = prepare_root_using(
        "chosen",
        &mut |args| {
            assert_eq!(&args[..2], &["-s", "chosen"]);
            Ok(match args[2] {
                "shell" => b"2000".to_vec(),
                "root" => b"adbd cannot run as root in production builds".to_vec(),
                "wait-for-device" => Vec::new(),
                _ => panic!(),
            })
        },
        &mut |_, message| verified |= message == "设备 root 权限已确认",
        &OperationCancellation::default(),
    )
    .unwrap_err();
    assert!(error.contains("无法取得 root 权限"));
    assert!(error.contains("adbd cannot run as root"));
    assert!(!verified);
}

#[test]
fn reconnect_failure_stops_before_verification_or_installation() {
    let mut commands = Vec::new();
    let error = prepare_root_using(
        "chosen",
        &mut |args| {
            commands.push(args[2].to_string());
            match args[2] {
                "shell" => Ok(b"2000".to_vec()),
                "root" => Ok(Vec::new()),
                "wait-for-device" => Err("timeout".into()),
                _ => panic!(),
            }
        },
        &mut |_, _| {},
        &OperationCancellation::default(),
    )
    .unwrap_err();
    assert!(error.contains("设备 chosen 重新连接失败"));
    assert!(error.contains("timeout"));
    assert_eq!(commands.last().unwrap(), "wait-for-device");
}

#[test]
fn cancellation_before_or_during_root_preparation_prevents_followup_commands() {
    for cancel_at in ["before", "root", "wait-for-device"] {
        let cancellation = OperationCancellation::default();
        if cancel_at == "before" {
            cancellation.cancel();
        }
        let mut commands = Vec::new();
        let result = prepare_root_using(
            "chosen",
            &mut |args| {
                assert!(!cancellation.is_cancelled());
                commands.push(args[2].to_string());
                if args[2] == cancel_at {
                    cancellation.cancel();
                }
                Ok(if args[2] == "shell" {
                    b"2000".to_vec()
                } else {
                    Vec::new()
                })
            },
            &mut |_, _| {},
            &cancellation,
        );
        assert!(result.is_err());
        if cancel_at == "before" {
            assert!(commands.is_empty());
        } else {
            assert_eq!(commands.last().unwrap(), cancel_at);
        }
    }
}
