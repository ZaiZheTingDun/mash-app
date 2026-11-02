use super::*;
use std::{cell::Cell, collections::VecDeque};

#[test]
fn version_boundary_uses_numeric_comparison() {
    for version in ["3.3.5", "3.3.8"] {
        assert!(!needs_api_key(version));
    }
    for version in ["3.3.9", "3.3.10", "3.4.0", "4.0.0"] {
        assert!(needs_api_key(version));
    }
}

#[test]
fn only_reqables_focused_window_can_supply_a_key() {
    assert_eq!(reqable_has_focus(
        b"  mCurrentFocus=Window{abc u0 com.reqable.android/com.reqable.android.MainActivity}\r\n"
    ), Some(true));
    for dump in [
        "mCurrentFocus=Window{abc u0 other.app/.Main}\nWindow{def u0 com.reqable.android/.Main}",
        "mCurrentFocus=null",
        "mCurrentFocus=Window{abc u0 fake.com.reqable.android/.Main}",
        "mCurrentFocus=Window{abc u0 com.reqable.android.evil/.Main}",
    ] {
        assert_eq!(reqable_has_focus(dump.as_bytes()), Some(false));
    }
    assert_eq!(
        reqable_has_focus(b"mTopFocusedDisplayId=0\nWindow #0 ..."),
        None
    );
}

#[test]
fn clipboard_focus_queries_displays_instead_of_the_window_list() {
    let mut commands = Vec::new();
    let focused = check_reqable_focus(&mut |command| {
        commands.push(command.to_owned());
        assert_eq!(command, "dumpsys window displays");
        Ok(b"WINDOW MANAGER DISPLAY CONTENTS (dumpsys window displays)\nDisplay: mDisplayId=0\n  mCurrentFocus=Window{abc u0 com.reqable.android/.MainActivity}".to_vec())
    }).unwrap();
    assert!(focused);
    assert_eq!(commands.len(), 1);
}

#[test]
fn missing_focus_metadata_falls_back_then_fails_explicitly_instead_of_dropping_the_key() {
    for fallback in [Some(true), Some(false), None] {
        let mut commands = Vec::new();
        let result = check_reqable_focus(&mut |command| {
            commands.push(command.to_owned());
            Ok(
                if command == "dumpsys window displays" || fallback.is_none() {
                    b"mTopFocusedDisplayId=0\nWindow #0 ...".to_vec()
                } else if fallback == Some(true) {
                    b"mCurrentFocus=Window{abc u0 com.reqable.android/.Main}".to_vec()
                } else {
                    b"mCurrentFocus=Window{abc u0 other.app/.Main}".to_vec()
                },
            )
        });
        assert_eq!(commands, ["dumpsys window displays", "dumpsys window"]);
        match fallback {
            Some(expected) => assert_eq!(result.unwrap(), expected),
            None => assert!(result.unwrap_err().contains("前台窗口信息")),
        }
    }
    let error =
        check_reqable_focus(&mut |_| Err("private output must not be echoed".into())).unwrap_err();
    assert!(!error.contains("private output"));
}

#[test]
fn device_scoped_keys_survive_restart_and_malformed_storage_is_not_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("keys.json");
    let store = ApiKeyStore::new(path.clone());
    assert_eq!(store.get("a").unwrap(), None);
    store.save("a", "test-key-aaaaaaaa").unwrap();
    store.save("b", "test-key-bbbbbbbb").unwrap();
    let reopened = ApiKeyStore::new(path.clone());
    assert_eq!(
        reopened.get("a").unwrap().as_deref(),
        Some("test-key-aaaaaaaa")
    );
    assert_eq!(
        reopened.get("b").unwrap().as_deref(),
        Some("test-key-bbbbbbbb")
    );
    assert_eq!(reopened.get("other-device").unwrap(), None);
    std::fs::write(&path, "private malformed data").unwrap();
    let error = store.save("c", "test-key-cccccccc").unwrap_err();
    assert!(!error.contains("private malformed data"));
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "private malformed data"
    );
}

#[test]
fn a_saved_valid_key_is_reused_without_starting_adb_or_a_clipboard_listener() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        assert!(String::from_utf8(request)
            .unwrap()
            .to_ascii_lowercase()
            .contains("authorization: bearer test-saved-key\r\n"));
        let body = r#"{"status":"inactive"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let directory = tempfile::tempdir().unwrap();
    let store = ApiKeyStore::new(directory.path().join("keys.json"));
    store.save("device", "test-saved-key").unwrap();
    let key = authorize(
        &store,
        Path::new("no-adb"),
        Path::new("no-jar"),
        "device",
        port,
        &OperationCancellation::default(),
        &mut |_| panic!("must not ask to copy again"),
    )
    .unwrap();
    assert_eq!(key, "test-saved-key");
    server.join().unwrap();
}

#[test]
fn unrelated_duplicate_and_rejected_clipboard_values_are_not_imported() {
    let mut messages = VecDeque::from([
        None,
        Some("".into()),
        Some("not an API key".into()),
        Some("old-invalid-key".into()),
        Some("new-invalid-key".into()),
        Some("new-invalid-key".into()),
        Some("  valid-copied-key  ".into()),
    ]);
    let mut attempted = Vec::new();
    let key = wait_for_key(
        Some("old-invalid-key"),
        &mut || {
            Ok(messages
                .pop_front()
                .expect("valid key should finish the workflow"))
        },
        &mut |key| {
            attempted.push(key.to_owned());
            if key == "valid-copied-key" {
                Ok(())
            } else {
                Err(client::AUTHENTICATION_FAILED.into())
            }
        },
        &mut || Ok(()),
    )
    .unwrap();
    assert_eq!(key, "valid-copied-key");
    assert_eq!(attempted, ["new-invalid-key", "valid-copied-key"]);
}

#[test]
fn cancellation_after_validation_prevents_committing_a_late_key() {
    let cancelled = Cell::new(false);
    let result = wait_for_key(
        None,
        &mut || Ok(Some("valid-copied-key".into())),
        &mut |_| {
            cancelled.set(true);
            Ok(())
        },
        &mut || {
            if cancelled.get() {
                Err("操作已停止".into())
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(result.unwrap_err(), "操作已停止");
}

#[test]
fn timeout_disconnect_and_api_service_errors_stop_waiting_without_echoing_keys() {
    let error = wait_for_key(None, &mut || Ok(None), &mut |_| panic!(), &mut || {
        Err("等待超时".into())
    })
    .unwrap_err();
    assert_eq!(error, "等待超时");
    let error = wait_for_key(
        None,
        &mut || Err("剪贴板断开".into()),
        &mut |_| panic!(),
        &mut || Ok(()),
    )
    .unwrap_err();
    assert_eq!(error, "剪贴板断开");
    let error = wait_for_key(
        None,
        &mut || Ok(Some("private-copied-key".into())),
        &mut |_| Err("API 服务未启用".into()),
        &mut || Ok(()),
    )
    .unwrap_err();
    assert_eq!(error, "API 服务未启用");
}

#[test]
fn bearer_candidates_are_bounded_and_reject_header_injection() {
    for text in [
        "short",
        "key with spaces",
        "key-12345\r\nAuthorization: attack",
        "分享码：abcdef123456",
    ] {
        assert!(candidate(text).is_none());
    }
    assert!(candidate(&"a".repeat(4097)).is_none());
    assert_eq!(candidate(" key-12345._~+/= "), Some("key-12345._~+/="));
}
