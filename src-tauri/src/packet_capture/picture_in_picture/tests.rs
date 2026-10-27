use super::*;

#[test]
fn accepts_only_explicit_disabled_pip_and_rejects_uid_overrides_or_unknown_output() {
    for text in [
        "PICTURE_IN_PICTURE: ignore\n",
        "  PICTURE_IN_PICTURE: ignore; time=+1s ago; rejectTime=+2s ago\n",
        "Uid mode: PICTURE_IN_PICTURE: ignore\nPICTURE_IN_PICTURE: ignore\n",
        "android:picture_in_picture: ignore\n",
    ] {
        assert!(is_disabled(text.as_bytes()), "{text}");
    }
    for text in [
        "",
        "No operations.\nDefault mode: allow\n",
        "ignore",
        "PICTURE_IN_PICTURE: allow",
        "PICTURE_IN_PICTURE: default",
        "SYSTEM_ALERT_WINDOW: ignore",
        "PICTURE_IN_PICTURE: ignored",
        "Uid mode: PICTURE_IN_PICTURE: allow\nPICTURE_IN_PICTURE: ignore\n",
        "PICTURE_IN_PICTURE: ignore\nError: permission denied\n",
    ] {
        assert!(!is_disabled(text.as_bytes()), "{text}");
    }
    assert!(!is_disabled(&[0xff]));
}

#[test]
fn changes_only_reqable_pip_for_the_managed_user_then_reads_back_before_success() {
    let mut commands = Vec::new();
    let mut disabled = false;
    let result = disable("su -c", &mut |command| {
        commands.push(command.to_string());
        if command == root_shell("su -c", SET) {
            disabled = true;
            return Ok(Vec::new());
        }
        assert_eq!(command, root_shell("su -c", GET));
        Ok(if disabled {
            "PICTURE_IN_PICTURE: ignore"
        } else {
            "PICTURE_IN_PICTURE: allow"
        }
        .as_bytes()
        .to_vec())
    });
    assert!(result.is_ok());
    assert_eq!(
        commands,
        [
            root_shell("su -c", GET),
            root_shell("su -c", SET),
            root_shell("su -c", GET)
        ]
    );
    commands.clear();
    disable("", &mut |command| {
        commands.push(command.to_string());
        Ok(b"PICTURE_IN_PICTURE: ignore".to_vec())
    })
    .unwrap();
    assert_eq!(commands, [root_shell("", GET)]);
}

#[test]
fn command_failure_success_without_effect_and_failed_readback_never_count_as_disabled() {
    for fail_at in [Some(1), Some(2), Some(3), None] {
        let mut calls = 0;
        let error = disable("", &mut |command| {
            calls += 1;
            if fail_at == Some(calls) {
                return Err("private-device-output".into());
            }
            assert_eq!(command, root_shell("", if calls == 2 { SET } else { GET }));
            Ok(b"PICTURE_IN_PICTURE: allow".to_vec())
        })
        .unwrap_err();
        assert!(error.contains("系统画中画权限"));
        assert!(!error.contains("private-device-output"));
        assert_eq!(calls, fail_at.unwrap_or(3));
    }
}
