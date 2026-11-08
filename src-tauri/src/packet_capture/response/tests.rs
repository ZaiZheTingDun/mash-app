use super::*;

#[test]
fn session_key_is_used_for_polling_and_stop_and_is_never_serialized() {
    let store = ResponseStore::default();
    store.begin_authenticated(
        Server::Cn,
        "device".into(),
        Baseline::default(),
        Some("synthetic-private-key".into()),
        false,
    );
    let state = store.poll_authenticated_using(
        |_, serial, _, key| {
            assert_eq!(serial, "device");
            assert_eq!(key, Some("synthetic-private-key"));
            Ok(ScanOutcome {
                response: Some(complete_response()),
                baseline: Baseline::default(),
            })
        },
        |serial, key| {
            assert_eq!(serial, "device");
            assert_eq!(key, Some("synthetic-private-key"));
            Ok(())
        },
    );
    assert_eq!(state.phase, ResponsePhase::Found);
    assert!(!serde_json::to_string(&state)
        .unwrap()
        .contains("synthetic-private-key"));
    store
        .stop_authenticated_using(|_, key| {
            assert_eq!(key, Some("synthetic-private-key"));
            Ok(())
        })
        .unwrap();
    store.reset();
    assert!(store.0.lock().unwrap().api_key.is_none());
}

#[test]
fn superseded_poll_uses_its_original_key_and_cannot_stop_the_new_session() {
    let store = ResponseStore::default();
    store.begin_authenticated(
        Server::Cn,
        "old".into(),
        Baseline::default(),
        Some("old-private-key".into()),
        false,
    );
    let state = store.poll_authenticated_using(
        |server, serial, _, key| {
            assert_eq!(server, Server::Cn);
            store.begin_authenticated(
                Server::Jp,
                "new".into(),
                Baseline::default(),
                Some("new-private-key".into()),
                false,
            );
            assert_eq!(serial, "old");
            assert_eq!(key, Some("old-private-key"));
            Ok(ScanOutcome {
                response: Some(complete_response()),
                baseline: Baseline::default(),
            })
        },
        |_, _| panic!("old poll must not stop a new capture"),
    );
    assert_eq!(store.0.lock().unwrap().server, Server::Jp);
    assert_eq!(state.serial.as_deref(), Some("new"));
    assert_eq!(state.phase, ResponsePhase::Waiting);
    assert_eq!(
        store.0.lock().unwrap().api_key.as_deref(),
        Some("new-private-key")
    );
}
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

const URL: &str = "https://le1-ios-fate.bilibiligame.net/rongame_beta//rgfate/60_1001/ac.php?_userId=private&_key=toplogin&token=secret";
const JP_URL: &str = "https://game.fate-go.jp/login/top?userId=private&authKey=secret#fragment";

#[test]
fn changing_clients_is_blocked_until_capture_or_failed_stop_is_resolved() {
    let store = ResponseStore::default();
    assert!(store.ensure_client_selection_allowed().is_ok());
    store.begin("device".into(), HashSet::new());
    assert!(store.ensure_client_selection_allowed().is_err());
    store.mark_stop_failed("device", "停止失败");
    assert!(store.ensure_client_selection_allowed().is_err());
    store.mark_stopped("抓包已停止");
    assert!(store.ensure_client_selection_allowed().is_ok());
}

#[test]
fn jp_matching_requires_the_exact_host_and_login_path_and_removes_private_queries() {
    assert_eq!(
        target_url(Server::Jp, JP_URL).unwrap(),
        "https://game.fate-go.jp/login/top"
    );
    assert!(target_url(Server::Jp, "https://game.fate-go.jp/login/top").is_some());
    for wrong in [
        JP_URL.replace("https:", "http:"),
        JP_URL.replace("game.fate-go.jp", "game.fate-go.jp.evil.test"),
        JP_URL.replace("game.fate-go.jp", "evilgame.fate-go.jp"),
        JP_URL.replace("game.fate-go.jp", "game.fate-go.us"),
        JP_URL.replace("game.fate-go.jp", "game.fate-go.jp:444"),
        JP_URL.replace("game.fate-go.jp", "user:secret@game.fate-go.jp"),
        JP_URL.replace("/login/top", "/login/topExtra"),
        JP_URL.replace("/login/top", "/login/top/"),
        JP_URL.replace("/login/top", "/other?_key=toplogin"),
        URL.into(),
    ] {
        assert!(target_url(Server::Jp, &wrong).is_none(), "{wrong}");
    }
    assert!(target_url(Server::Cn, JP_URL).is_none());
}

#[test]
fn jp_response_session_keeps_its_server_through_polling_and_automatic_stop() {
    for stop_fails in [false, true] {
        let store = ResponseStore::default();
        store.begin_authenticated(
            Server::Jp,
            "device".into(),
            Baseline::default(),
            None,
            false,
        );
        let state = store.poll_authenticated_using(
            |server, _, _, _| {
                assert_eq!(server, Server::Jp);
                let mut response = complete_response();
                response.url = target_url(server, JP_URL).unwrap();
                Ok(ScanOutcome {
                    response: Some(response),
                    baseline: Baseline::default(),
                })
            },
            |serial, _| {
                assert_eq!(serial, "device");
                if stop_fails {
                    Err("请在设备上手动关闭抓包".into())
                } else {
                    Ok(())
                }
            },
        );
        assert!(state.message.contains("日服 login/top"));
        assert!(!state.message.contains("国服"));
        assert!(state.response.is_some());
        assert_eq!(
            state.phase,
            if stop_fails {
                ResponsePhase::Error
            } else {
                ResponsePhase::Found
            }
        );
    }
}

#[test]
fn matching_requires_cn_ac_php_and_exact_query_selector_without_disclosing_other_parameters() {
    let matched = target_url(Server::Cn, URL).unwrap();
    assert!(matched.ends_with("ac.php?_key=toplogin"));
    assert!(!matched.contains("private"));
    assert!(!matched.contains("secret"));
    for wrong in [
        URL.replace("ac.php", "login.php"),
        URL.replace("toplogin", "otherAction"),
        URL.replace("toplogin", "toploginExtra"),
        URL.replace(".bilibiligame.net", ".bilibiligame.net.evil.test"),
        URL.replace("https:", "http:"),
        format!("{URL}&_key=another"),
        "https://game.fate-go.jp/login/top?_key=toplogin".into(),
    ] {
        assert!(target_url(Server::Cn, &wrong).is_none());
    }
    assert!(target_url(
        Server::Cn,
        &URL.replace("le1-ios-fate", "line3-s2-bili-fate")
    )
    .is_some());
    assert!(target_url(Server::Cn, &URL.replace("_key=toplogin", "_key=%74oplogin")).is_some());
}

#[test]
fn body_reading_handles_inline_base64_and_files_without_lossy_utf8() {
    let expected = "从者数据".as_bytes();
    for body in [
        Body {
            text: "从者数据".into(),
            encoding: "utf8".into(),
        },
        Body {
            text: STANDARD.encode(expected),
            encoding: "base64".into(),
        },
        Body {
            text: "/data/user/0/com.reqable.android/cache/body".into(),
            encoding: "file".into(),
        },
    ] {
        assert_eq!(
            body_bytes(body, &mut |_| Ok(expected.to_vec())).unwrap(),
            expected
        );
    }
    assert!(body_bytes(
        Body {
            text: "!".into(),
            encoding: "base64".into()
        },
        &mut |_| panic!()
    )
    .is_err());
    assert!(body_bytes(
        Body {
            text: "".into(),
            encoding: "other".into()
        },
        &mut |_| panic!()
    )
    .is_err());
    assert!(body_bytes(
        Body {
            text: "x".repeat(MAX_BODY + 1),
            encoding: "utf8".into()
        },
        &mut |_| panic!()
    )
    .is_err());
    assert_eq!(
        raw_body(vec![0xff, 0x00]).unwrap(),
        ("Base64（二进制响应）".into(), "/wA=".into())
    );
}

#[test]
fn only_reqable_cache_paths_are_readable_and_symlinks_are_resolved_before_reading() {
    for bad in [
        "/data/user/0/com.reqable.android/files/certificate/capture.bin",
        "/data/user/0/com.reqable.android/cache/../files/certificate/capture.bin",
        "/data/user/0/com.reqable.android/cache/%2e%2e/private",
        "/data/user/0/com.reqable.android/cache/a\nb",
        "file://external/data/user/0/com.reqable.android/cache/body",
        "/sdcard/unrelated/file",
        "/etc/passwd",
    ] {
        assert!(cache_path(bad).is_err());
    }
    assert!(cache_path("file:///data/user/0/com.reqable.android/cache/body").is_ok());
    assert!(cache_path("/data/user/0/com.reqable.android/files/tmp/body").is_ok());
    let mut read_binary = false;
    let result = read_cache(
        "device",
        "/data/user/0/com.reqable.android/cache/body",
        &mut |args| {
            let command = args.last().unwrap();
            if command.contains("id -u") {
                return Ok(b"0\n".to_vec());
            }
            if command.contains("readlink -f") {
                return Ok(
                    b"/data/user/0/com.reqable.android/files/certificate/capture.bin\n".to_vec(),
                );
            }
            read_binary = true;
            panic!("must not read a symlink outside the cache");
        },
    );
    assert!(result.is_err());
    assert!(!read_binary);
}

#[test]
fn cache_reads_use_binary_exec_out_and_reject_partial_files() {
    for partial in [false, true] {
        let mut commands = Vec::new();
        let result = read_cache(
            "device",
            "/data/user/0/com.reqable.android/cache/body",
            &mut |args| {
                commands.push(args.join(" "));
                let command = args.last().unwrap();
                if command.contains("id -u") {
                    Ok(b"0\n".to_vec())
                } else if command.contains("readlink -f") {
                    Ok(b"/data/user/0/com.reqable.android/cache/body\n".to_vec())
                } else if command.contains("stat -c") {
                    Ok(b"4\n".to_vec())
                } else {
                    assert_eq!(args[2], "exec-out");
                    Ok(if partial {
                        vec![0, 255]
                    } else {
                        vec![0, 255, 13, 10]
                    })
                }
            },
        );
        if partial {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap(), [0, 255, 13, 10]);
        }
        assert!(commands.iter().any(|command| command.contains("exec-out")));
    }
}

#[test]
fn raw_encoded_body_is_never_replaced_by_decoded_json_or_pretty_printing() {
    let json = r#"{"cache":{"userItem":[{"itemId":100,"num":5}]}}"#;
    let encoded = URL_SAFE.encode(json).replace('=', "%3D");
    for text in [json.to_string(), encoded] {
        let (format, raw) = raw_body(text.as_bytes().to_vec()).unwrap();
        assert_eq!(format, "原始响应");
        assert_eq!(raw, text);
        assert_eq!(
            decoded_json(&raw).unwrap(),
            serde_json::from_str::<serde_json::Value>(json).unwrap()
        );
    }
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(json.as_bytes()).unwrap();
    assert_eq!(raw_body(encoder.finish().unwrap()).unwrap().1, json);
    assert_eq!(
        raw_body(b"raw-response\r\n".to_vec()).unwrap().1,
        "raw-response\r\n"
    );
}

#[test]
fn state_survives_navigation_but_stale_polls_cannot_replace_a_new_session() {
    let store = ResponseStore::default();
    assert_eq!(
        store.poll_using(|_, _| panic!(), |_| panic!()).phase,
        ResponsePhase::Idle
    );
    store.begin("device".into(), HashSet::from(["old-uid".into()]));
    assert_eq!(
        store
            .poll_using(
                |serial, baseline| {
                    assert_eq!(serial, "device");
                    assert!(baseline.contains("old-uid"));
                    Ok(None)
                },
                |_| panic!("pending response must not stop capture")
            )
            .phase,
        ResponsePhase::Waiting
    );
    let late = store.poll_using(
        |_, _| {
            store.reset();
            Err("late error".into())
        },
        |_| panic!("stale poll must not stop capture"),
    );
    assert_eq!(late.phase, ResponsePhase::Idle);
    store.begin("new-device".into(), HashSet::new());
    assert_eq!(
        store
            .poll_using(|_, _| Err("连接断开".into()), |_| panic!())
            .phase,
        ResponsePhase::Waiting
    );
    assert_eq!(
        store.poll_using(|_, _| panic!(), |_| panic!()).phase,
        ResponsePhase::Waiting
    );
}

fn complete_response() -> LoginResponse {
    LoginResponse {
        capture_id: uuid::Uuid::new_v4().to_string(),
        captured_at: 1,
        items: vec![],
        record_id: 42,
        url: target_url(Server::Cn, URL).unwrap(),
        status_code: 200,
        byte_length: 12,
        format: "JSON".into(),
        body: "{\"cache\":{}}".into(),
    }
}

#[test]
fn workspace_status_never_serializes_the_complete_response() {
    let state = ResponseState {
        phase: ResponsePhase::Found,
        serial: Some("device".into()),
        message: "已获取响应".into(),
        warnings: vec![],
        retry_after_ms: 1000,
        response: Some(complete_response()),
    };
    let status = serde_json::to_value(state.status()).unwrap();
    assert_eq!(
        status,
        serde_json::json!({ "phase": "found", "serial": "device", "message": "已获取响应", "warnings": [], "retryAfterMs": 1000 })
    );
    assert!(state.response.is_some());
}

#[test]
fn complete_response_is_preserved_after_stop_success_or_failure_and_stop_runs_only_once() {
    for succeeds in [true, false] {
        let store = ResponseStore::default();
        store.begin("device".into(), HashSet::new());
        let state = store.poll_using(
            |_, _| Ok(Some(complete_response())),
            |serial| {
                assert_eq!(serial, "device");
                if succeeds {
                    Ok(())
                } else {
                    Err("请在设备上手动关闭抓包".into())
                }
            },
        );
        assert_eq!(
            state.response.as_ref().unwrap().body,
            complete_response().body
        );
        if succeeds {
            assert_eq!(state.phase, ResponsePhase::Found);
            assert!(state.message.contains("抓包已停止"));
        } else {
            assert_eq!(state.phase, ResponsePhase::Error);
            assert!(state.message.contains("手动关闭抓包"));
        }
        let cached = store.poll_using(|_, _| panic!(), |_| panic!("must not stop twice"));
        assert_eq!(cached.phase, state.phase);
        assert_eq!(cached.response.unwrap().body, complete_response().body);
    }
}

#[test]
fn a_late_complete_response_cannot_stop_a_new_capture_session() {
    let store = ResponseStore::default();
    store.begin("old-device".into(), HashSet::new());
    let state = store.poll_using(
        |_, _| {
            store.begin("new-device".into(), HashSet::new());
            Ok(Some(complete_response()))
        },
        |_| panic!("late completion must never stop the new capture"),
    );
    assert_eq!(state.phase, ResponsePhase::Waiting);
    assert_eq!(state.serial.as_deref(), Some("new-device"));
    assert!(state.response.is_none());
}

// A local HTTP fixture checks the actual Reqable JSON exchange, never game data.
fn server(
    replies: Vec<serde_json::Value>,
) -> (u16, thread::JoinHandle<Vec<(String, serde_json::Value)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for reply in replies {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut buffer = Vec::new();
            let header_end = loop {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                buffer.push(byte[0]);
                if buffer.ends_with(b"\r\n\r\n") {
                    break buffer.len();
                }
            };
            let headers = String::from_utf8(buffer.clone()).unwrap();
            let len = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|len| len.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            buffer.resize(header_end + len, 0);
            stream.read_exact(&mut buffer[header_end..]).unwrap();
            let body = if len == 0 {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&buffer[header_end..]).unwrap()
            };
            requests.push((headers.lines().next().unwrap().into(), body));
            let body = reply.to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
        requests
    });
    (port, handle)
}

fn captured(id: u64, uid: &str, body: Option<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({"id":id,"uid":uid,"url":URL,"response":{"code":200,"body":body}})
}

fn jp_captured(id: u64, uid: &str, body: Option<serde_json::Value>) -> serde_json::Value {
    let mut record = captured(id, uid, body);
    record["url"] = JP_URL.into();
    record
}

#[test]
fn jp_polling_excludes_old_and_cn_responses_and_preserves_raw_json_and_preview() {
    let api = client::Client::new().unwrap();
    let (port, worker) = server(vec![
        serde_json::json!([1, 2]),
        jp_captured(1, "old-jp", None),
        captured(2, "old-cn", None),
    ]);
    let old = baseline(Server::Jp, &api, port).unwrap();
    assert!(old.contains("old-jp"));
    assert!(!old.contains("old-cn"));
    assert_eq!(
        worker.join().unwrap()[0].1["filters"][0]["pattern"],
        "/login/top"
    );

    let (port, worker) = server(vec![
        serde_json::json!([1, 2, 3]),
        captured(
            3,
            "new-cn",
            Some(serde_json::json!({"text":"wrong-server","encoding":"utf8"})),
        ),
        jp_captured(2, "old-jp", None),
        jp_captured(1, "pending-jp", None),
        serde_json::json!({"status":"active"}),
    ]);
    assert!(
        scan(Server::Jp, &api, port, "device", &old, &mut |_| panic!())
            .unwrap()
            .is_none()
    );
    worker.join().unwrap();

    let raw = "{\n  \"response\": [{\"success\": {\"cache\": {\"replaced\": {\"userItem\": [{\"itemId\":1,\"num\":1234}]}}}}]\n}";
    let (port, worker) = server(vec![
        serde_json::json!([1]),
        jp_captured(
            1,
            "fresh-jp",
            Some(serde_json::json!({"text":raw,"encoding":"utf8"})),
        ),
    ]);
    let response = scan(Server::Jp, &api, port, "device", &old, &mut |_| panic!())
        .unwrap()
        .unwrap();
    worker.join().unwrap();
    assert_eq!(response.url, "https://game.fate-go.jp/login/top");
    assert_eq!(response.body, raw);
    assert_eq!(response.items[0].quantity, 1234);
    assert_eq!(response.items[0].name, "QP");
}

#[test]
fn polling_ignores_baseline_uids_but_accepts_reused_numeric_ids_and_pending_responses() {
    let api = client::Client::new().unwrap();
    let (port, worker) = server(vec![serde_json::json!([1]), captured(1, "old", None)]);
    let old = baseline(Server::Cn, &api, port).unwrap();
    let requests = worker.join().unwrap();
    assert_eq!(requests[0].1["filters"][0]["pattern"], "_key=toplogin");
    let (port, worker) = server(vec![
        serde_json::json!([1]),
        captured(1, "old", None),
        serde_json::json!({"status":"active"}),
    ]);
    assert!(
        scan(Server::Cn, &api, port, "device", &old, &mut |_| panic!())
            .unwrap()
            .is_none()
    );
    worker.join().unwrap();
    let (port, worker) = server(vec![
        serde_json::json!([1]),
        captured(1, "new", None),
        serde_json::json!({"status":"active"}),
    ]);
    assert!(
        scan(Server::Cn, &api, port, "device", &old, &mut |_| panic!())
            .unwrap()
            .is_none()
    );
    worker.join().unwrap();
    let (port, worker) = server(vec![
        serde_json::json!([1]),
        captured(
            1,
            "new",
            Some(serde_json::json!({"text":"{\"cache\":{}}","encoding":"utf8"})),
        ),
    ]);
    let response = scan(Server::Cn, &api, port, "device", &old, &mut |_| panic!())
        .unwrap()
        .unwrap();
    worker.join().unwrap();
    assert_eq!(response.record_id, 1);
    assert_eq!(response.format, "原始响应");
    assert_eq!(response.body, r#"{"cache":{}}"#);
    assert!(!response.url.contains("private"));
}

#[test]
#[ignore = "requires MASH_CAPTURE_DEVICE and a fresh CN game login on the authorized device"]
fn display_toplogin_on_authorized_device() {
    let serial = std::env::var("MASH_CAPTURE_DEVICE").expect("set MASH_CAPTURE_DEVICE");
    let adb = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/adb")
        .join(crate::platform::adb_executable_name());
    let started = super::super::start_capture(
        Server::Cn,
        target::package(Server::Cn),
        &adb,
        Some(&serial),
        &crate::device_operation::OperationCancellation::default(),
        &super::super::ApiKeyStore::new(tempfile::tempdir().unwrap().path().join("api-keys.json")),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/scrcpy/scrcpy-server.jar"),
        &super::super::AuthorizationStore::default(),
        &mut |_| {},
    )
    .unwrap();
    let store = ResponseStore::default();
    started.begin_response_watch(&store);
    println!("Waiting for a fresh CN toplogin response on {serial}");
    for _ in 0..600 {
        let state = store.poll(&adb);
        assert_ne!(state.phase, ResponsePhase::Error, "{}", state.message);
        if let Some(response) = state.response {
            // Only structural metadata enters test output. The account body
            // is available solely through the local UI command in production.
            assert_eq!(response.status_code, 200);
            assert!(!response.body.is_empty());
            assert_eq!(state.phase, ResponsePhase::Found);
            assert!(state.message.contains("抓包已停止"));
            println!(
                "toplogin received: {} bytes, {}, {} display characters",
                response.byte_length,
                response.format,
                response.body.chars().count()
            );
            return;
        }
        thread::sleep(std::time::Duration::from_millis(500));
    }
    panic!("Timed out waiting for a fresh CN login");
}

#[test]
#[ignore = "requires MASH_CAPTURE_DEVICE and an existing authorized CN toplogin capture"]
fn read_toplogin_cache_on_authorized_device() {
    let serial = std::env::var("MASH_CAPTURE_DEVICE").expect("set MASH_CAPTURE_DEVICE");
    let adb = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/adb")
        .join(crate::platform::adb_executable_name());
    let store = ResponseStore::default();
    // Test-only: validate cache decoding without forcing another game login.
    // Production always seeds the baseline from records present before Start.
    store.begin(serial, HashSet::new());
    let state = store.poll(&adb);
    assert_eq!(state.phase, ResponsePhase::Found, "{}", state.message);
    assert!(state.message.contains("抓包已停止"));
    let response = state.response.expect("an existing toplogin response");
    assert_eq!(response.status_code, 200);
    assert!(!response.body.is_empty());
    let dto = serde_json::to_value(&response).unwrap();
    assert_eq!(dto["body"].as_str(), Some(response.body.as_str()));
    assert_eq!(
        dto["byteLength"].as_u64(),
        Some(response.byte_length as u64)
    );
    println!(
        "toplogin cache verified and capture stopped: {} bytes, {}, {} display characters",
        response.byte_length,
        response.format,
        response.body.chars().count()
    );
}

#[test]
fn manual_stop_preserves_waiting_on_failure_and_supersedes_an_inflight_poll_on_success() {
    let store = ResponseStore::default();
    store.begin("device".into(), HashSet::new());
    assert!(store
        .stop_using(|serial| {
            assert_eq!(serial, "device");
            Err("stop failed".into())
        })
        .is_err());
    assert_eq!(store.0.lock().unwrap().state.phase, ResponsePhase::Waiting);
    let result = store.poll_using(
        |_, _| {
            store.stop_using(|_| Ok(())).unwrap();
            Ok(Some(complete_response()))
        },
        |_| panic!("a manually stopped session must not stop a newer session"),
    );
    assert_eq!(result.phase, ResponsePhase::Idle);
    assert!(result.response.is_none());
    assert_eq!(result.message, "抓包已停止");
}

#[test]
fn waiting_copy_is_generic_and_a_stopped_task_cannot_be_revived_by_a_late_poll() {
    let store = ResponseStore::default();
    store.begin("device".into(), HashSet::new());
    assert_eq!(
        store.0.lock().unwrap().state.message,
        "等待数据中，请重新进入游戏……"
    );
    let state = store.poll_using(
        |_, _| {
            store.mark_stopped("环境配置已停止");
            Ok(None)
        },
        |_| panic!("a stopped generation must not issue device actions"),
    );
    assert_eq!(state.phase, ResponsePhase::Idle);
    assert_eq!(state.message, "环境配置已停止");
}

#[test]
fn transient_scan_failures_back_off_then_recover_without_a_restart() {
    let store = ResponseStore::default();
    store.begin("device".into(), HashSet::new());
    let failed = store.poll_using(|_, _| Err("ADB temporarily offline".into()), |_| panic!());
    assert_eq!(failed.phase, ResponsePhase::Waiting);
    assert_eq!(failed.retry_after_ms, 2000);
    assert!(failed.message.contains("自动继续"));
    store.poll_using(|_, _| panic!("must respect retry deadline"), |_| panic!());
    store.0.lock().unwrap().next_poll = Some(Instant::now());
    let recovered = store.poll_using(|_, _| Ok(Some(complete_response())), |_| Ok(()));
    assert_eq!(recovered.phase, ResponsePhase::Found);
    assert!(recovered.response.is_some());
}

#[test]
fn authentication_failure_still_ends_waiting_without_claiming_capture_has_stopped() {
    let store = ResponseStore::default();
    store.begin("device".into(), HashSet::new());
    let result = store.poll_using(
        |_, _| Err(client::AUTHENTICATION_FAILED.into()),
        |_| panic!(),
    );
    assert_eq!(result.phase, ResponsePhase::Error);
    assert!(result.serial.is_some());
    assert!(store.ensure_client_selection_allowed().is_err());
}

#[test]
fn more_than_128_old_records_and_a_malformed_record_do_not_prevent_a_fresh_capture() {
    let api = client::Client::new().unwrap();
    let mut replies = vec![serde_json::json!((0..130).collect::<Vec<_>>())];
    replies.extend((0..129).map(|id| captured(id, &format!("old-{id}"), None)));
    replies.push(serde_json::json!({"invalid":"record"}));
    let (port, worker) = server(replies);
    let mut progress = Vec::new();
    let baseline = baseline_controlled(Server::Cn, &api, port, &mut || Ok(()), &mut |message| {
        progress.push(message.to_string())
    })
    .unwrap();
    worker.join().unwrap();
    assert_eq!(baseline.uids.len(), 129);
    assert!(baseline.ignored_ids.contains(&129));
    assert!(!baseline.warnings.is_empty());
    assert!(progress.len() > 1);
    // An unreadable old ID remains excluded even if it later becomes readable.
    let (port, worker) = server(vec![
        serde_json::json!([129, 130]),
        captured(
            130,
            "fresh",
            Some(serde_json::json!({"text":"complete","encoding":"utf8"})),
        ),
    ]);
    let response = scan(
        Server::Cn,
        &api,
        port,
        "device",
        &baseline,
        &mut |_| panic!(),
    )
    .unwrap()
    .unwrap();
    worker.join().unwrap();
    assert_eq!(response.body, "complete");
}

#[test]
fn failed_login_and_unsupported_body_do_not_hide_another_valid_response() {
    let api = client::Client::new().unwrap();
    let mut failed = captured(
        3,
        "failed",
        Some(serde_json::json!({"text":"error","encoding":"utf8"})),
    );
    failed["response"]["code"] = 503.into();
    let (port, worker) = server(vec![
        serde_json::json!([1, 2, 3]),
        failed,
        captured(
            2,
            "bad-body",
            Some(serde_json::json!({"text":"invalid","encoding":"unsupported"})),
        ),
        captured(
            1,
            "good",
            Some(serde_json::json!({"text":"complete","encoding":"utf8"})),
        ),
    ]);
    let outcome = scan_session(
        Server::Cn,
        &api,
        port,
        "device",
        &Baseline::default(),
        &mut |_| panic!(),
    )
    .unwrap();
    worker.join().unwrap();
    assert_eq!(outcome.response.unwrap().body, "complete");
    assert!(outcome.baseline.contains("failed"));
    assert!(outcome.baseline.contains("bad-body"));
    assert_eq!(outcome.baseline.warnings.len(), 2);
}

#[test]
fn baseline_can_be_cancelled_between_records() {
    let api = client::Client::new().unwrap();
    let (port, worker) = server(vec![serde_json::json!([1, 2]), captured(1, "old", None)]);
    let mut checks = 0;
    let result = baseline_controlled(
        Server::Cn,
        &api,
        port,
        &mut || {
            checks += 1;
            if checks >= 3 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        },
        &mut |_| {},
    );
    worker.join().unwrap();
    assert!(result.is_err());
}

#[test]
fn game_launch_failure_stays_in_the_waiting_message_across_polls_and_recovers_after_restart() {
    let store = ResponseStore::default();
    store.begin_authenticated(Server::Cn, "device".into(), Baseline::default(), None, true);
    for _ in 0..2 {
        let state = store.poll_using(|_, _| Ok(None), |_| panic!());
        assert_eq!(
            state.message,
            "等待数据中，自动启动游戏失败，请重新进入游戏……"
        );
        assert!(state.warnings.is_empty());
        assert_eq!(state.phase, ResponsePhase::Waiting);
    }
    store.reset();
    store.begin("device".into(), HashSet::new());
    assert_eq!(
        store.poll_using(|_, _| Ok(None), |_| panic!()).message,
        "等待数据中，请重新进入游戏……"
    );
}
