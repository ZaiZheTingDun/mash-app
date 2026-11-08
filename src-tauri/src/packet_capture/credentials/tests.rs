use super::*;

#[test]
fn version_boundary_uses_numeric_comparison() {
    for version in ["3.3.5", "3.3.8"] {
        assert!(!needs_api_key(version));
    }
    for version in [
        "3.3.9",
        "3.3.9.1",
        "v3.3.9-beta.1",
        "3.3.10",
        "3.4.0",
        "4.0.0",
    ] {
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
    check_saved_key_reuse(false, Duration::ZERO);
}

#[test]
fn a_saved_key_survives_a_cold_start_connection_failure_without_asking_to_copy_again() {
    check_saved_key_reuse(true, Duration::ZERO);
}

#[test]
fn saved_key_validation_still_accepts_a_slow_status_response() {
    check_saved_key_reuse(false, Duration::from_millis(2200));
}

fn check_saved_key_reuse(cold_start: bool, delay: Duration) {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        if cold_start {
            // ADB may forward to Reqable before its HTTP listener is ready.
            // Simulate the forwarded connection closing without a response.
            let (stream, _) = listener.accept().unwrap();
            drop(stream);
        }
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
        std::thread::sleep(delay);
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
        &AuthorizationStore::default(),
        &mut |_| panic!("must not ask to copy again"),
    )
    .unwrap();
    assert_eq!(key, "test-saved-key");
    server.join().unwrap();
}

#[test]
fn saved_key_validation_retries_transport_failures_until_the_api_is_ready() {
    let mut attempts = 0;
    let mut pauses = 0;
    validate_saved_key(
        &mut |timeout| {
            assert!(timeout <= Duration::from_secs(30));
            attempts += 1;
            if attempts < 3 {
                Err("无法连接设备上的 Reqable 控制接口".into())
            } else {
                Ok(())
            }
        },
        &mut || pauses += 1,
        &OperationCancellation::default(),
    )
    .unwrap();
    assert_eq!(attempts, 3);
    assert_eq!(pauses, 2);
}

#[test]
fn saved_key_validation_is_bounded_and_does_not_retry_authentication_or_invalid_responses() {
    for (error, expected_attempts) in [
        ("无法连接设备上的 Reqable 控制接口", 12),
        ("Reqable 控制接口响应超时，请保持 Reqable 运行后重试", 12),
        (client::AUTHENTICATION_FAILED, 1),
        ("Reqable API 服务未启用", 1),
        ("Reqable 抓包状态无效", 1),
    ] {
        let mut attempts = 0;
        let mut pauses = 0;
        let result = validate_saved_key(
            &mut |_| {
                attempts += 1;
                Err(error.into())
            },
            &mut || pauses += 1,
            &OperationCancellation::default(),
        );
        assert_eq!(result.unwrap_err(), error);
        assert_eq!(attempts, expected_attempts);
        assert_eq!(pauses, expected_attempts - 1);
    }
}

#[test]
fn saved_key_validation_observes_cancellation_during_retry_or_a_successful_request() {
    for cancel_in_request in [false, true] {
        let cancellation = OperationCancellation::default();
        let mut attempts = 0;
        let result = validate_saved_key(
            &mut |_| {
                attempts += 1;
                if cancel_in_request {
                    cancellation.cancel();
                    Ok(())
                } else {
                    Err("无法连接设备上的 Reqable 控制接口".into())
                }
            },
            &mut || cancellation.cancel(),
            &cancellation,
        );
        assert_eq!(result.unwrap_err(), "操作已停止");
        assert_eq!(attempts, 1);
    }
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

#[test]
fn manual_authorization_survives_a_rejected_key_without_clipboard_and_cleans_up() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        for status in [401, 200] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let body = r#"{"status":"inactive"}"#;
            write!(
                stream,
                "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("keys.json");
    let saved_path = path.clone();
    let authorization = AuthorizationStore::default();
    let worker_store = authorization.clone();
    let worker = std::thread::spawn(move || {
        authorize(
            &ApiKeyStore::new(path),
            Path::new("missing-adb"),
            Path::new("missing-jar"),
            "device",
            port,
            &OperationCancellation::default(),
            &worker_store,
            &mut |_| {},
        )
    });
    let wait_for = |ready: &dyn Fn(&serde_json::Value) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let prompt = serde_json::to_value(authorization.prompt()).unwrap();
            if ready(&prompt) {
                break prompt;
            }
            assert!(
                Instant::now() < deadline,
                "authorization prompt did not become ready"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let prompt = wait_for(&|prompt| {
        prompt["message"]
            .as_str()
            .is_some_and(|message| message.contains("手动输入"))
    });
    let id = prompt["requestId"].as_str().unwrap();
    authorization.submit(id, "manual-test-private-key").unwrap();
    wait_for(&|prompt| {
        prompt["message"]
            .as_str()
            .is_some_and(|message| message.contains("认证失败"))
    });
    authorization.submit(id, "manual-test-private-key").unwrap();
    assert_eq!(worker.join().unwrap().unwrap(), "manual-test-private-key");
    server.join().unwrap();
    assert!(authorization.prompt().is_none());
    assert!(authorization.submit(id, "manual-test-private-key").is_err());
    assert_eq!(
        ApiKeyStore::new(saved_path)
            .get("device")
            .unwrap()
            .as_deref(),
        Some("manual-test-private-key")
    );
}

#[test]
fn cancellation_while_awaiting_manual_input_clears_the_prompt_without_saving_a_key() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("keys.json");
    let saved_path = path.clone();
    let authorization = AuthorizationStore::default();
    let worker_store = authorization.clone();
    let cancellation = OperationCancellation::default();
    let worker_cancel = cancellation.clone();
    let worker = std::thread::spawn(move || {
        authorize(
            &ApiKeyStore::new(path),
            Path::new("missing-adb"),
            Path::new("missing-jar"),
            "device",
            1,
            &worker_cancel,
            &worker_store,
            &mut |_| {},
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while authorization.prompt().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    cancellation.cancel();
    assert_eq!(worker.join().unwrap().unwrap_err(), "操作已停止");
    assert!(authorization.prompt().is_none());
    assert!(!saved_path.exists());
}
