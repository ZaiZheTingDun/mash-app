use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

const URL: &str = "https://le1-ios-fate.bilibiligame.net/rongame_beta//rgfate/60_1001/ac.php?_userId=private&_key=toplogin&token=secret";

#[test]
fn matching_requires_cn_ac_php_and_exact_query_selector_without_disclosing_other_parameters() {
    let matched = target_url(URL).unwrap();
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
        assert!(target_url(&wrong).is_none());
    }
    assert!(target_url(&URL.replace("le1-ios-fate", "line3-s2-bili-fate")).is_some());
    assert!(target_url(&URL.replace("_key=toplogin", "_key=%74oplogin")).is_some());
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
        ResponsePhase::Error
    );
    assert_eq!(
        store.poll_using(|_, _| panic!(), |_| panic!()).phase,
        ResponsePhase::Error
    );
}

fn complete_response() -> LoginResponse {
    LoginResponse {
        capture_id: uuid::Uuid::new_v4().to_string(),
        captured_at: 1,
        items: vec![],
        record_id: 42,
        url: target_url(URL).unwrap(),
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
        response: Some(complete_response()),
    };
    let status = serde_json::to_value(state.status()).unwrap();
    assert_eq!(
        status,
        serde_json::json!({ "phase": "found", "serial": "device", "message": "已获取响应" })
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

#[test]
fn polling_ignores_baseline_uids_but_accepts_reused_numeric_ids_and_pending_responses() {
    let api = client::Client::new().unwrap();
    let (port, worker) = server(vec![serde_json::json!([1]), captured(1, "old", None)]);
    let old = baseline(&api, port).unwrap();
    let requests = worker.join().unwrap();
    assert_eq!(requests[0].1["filters"][0]["pattern"], "_key=toplogin");
    let (port, worker) = server(vec![
        serde_json::json!([1]),
        captured(1, "old", None),
        serde_json::json!({"status":"active"}),
    ]);
    assert!(scan(&api, port, "device", &old, &mut |_| panic!())
        .unwrap()
        .is_none());
    worker.join().unwrap();
    let (port, worker) = server(vec![
        serde_json::json!([1]),
        captured(1, "new", None),
        serde_json::json!({"status":"active"}),
    ]);
    assert!(scan(&api, port, "device", &old, &mut |_| panic!())
        .unwrap()
        .is_none());
    worker.join().unwrap();
    let (port, worker) = server(vec![
        serde_json::json!([1]),
        captured(
            1,
            "new",
            Some(serde_json::json!({"text":"{\"cache\":{}}","encoding":"utf8"})),
        ),
    ]);
    let response = scan(&api, port, "device", &old, &mut |_| panic!())
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
        &adb,
        Some(&serial),
        &crate::device_operation::OperationCancellation::default(),
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
