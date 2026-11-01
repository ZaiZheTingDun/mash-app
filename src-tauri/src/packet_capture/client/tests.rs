use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
};

fn request(status: &str, body: &str) -> Result<Vec<u8>, String> {
    delayed_request(Client::new().unwrap(), status, body, Duration::ZERO)
}

fn delayed_request(
    client: Client,
    status: &str,
    body: &str,
    delay: Duration,
) -> Result<Vec<u8>, String> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            assert_eq!(stream.read(&mut byte).unwrap(), 1);
            request.push(byte[0]);
        }
        assert!(request.starts_with(b"GET /capture/live/status HTTP/1.1\r\n"));
        std::thread::sleep(delay);
        // Timeout cases intentionally close the connection before the reply.
        let _ = stream.write_all(response.as_bytes());
    });
    let result = client.request(port, "status", None, 4096);
    server.join().unwrap();
    result
}

#[test]
fn disabled_api_service_has_an_actionable_error() {
    let error = request(
        "403 Forbidden",
        "Access denied. Please enable API Service in Reqable settings.",
    )
    .unwrap_err();
    assert!(error.contains("API 服务未启用"));
    assert!(error.contains("Reqable 设置"));
}

#[test]
fn authentication_and_other_http_errors_remain_distinct_without_echoing_response_bodies() {
    for status in ["401 Unauthorized", "403 Forbidden"] {
        let error = request(status, "private-response-token").unwrap_err();
        assert!(error.contains("认证失败"));
        assert!(!error.contains("private-response-token"));
    }
    assert!(
        request("500 Internal Server Error", "private-response-body")
            .unwrap_err()
            .contains("HTTP 500")
    );
}

#[test]
fn successful_status_response_is_preserved() {
    let body = r#"{"status":"inactive"}"#;
    assert_eq!(request("200 OK", body).unwrap(), body.as_bytes());
}

#[test]
fn a_connected_reqable_can_take_longer_than_two_seconds_to_respond() {
    let body = r#"{"status":"active"}"#;
    assert_eq!(
        delayed_request(
            Client::new().unwrap(),
            "200 OK",
            body,
            Duration::from_millis(2200)
        )
        .unwrap(),
        body.as_bytes()
    );
}

#[test]
fn response_timeouts_are_distinct_from_connection_failures() {
    let client = Client(
        reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap(),
    );
    let error = delayed_request(client, "200 OK", "{}", Duration::from_millis(250)).unwrap_err();
    assert!(error.contains("响应超时"));
    assert!(!error.contains("无法连接"));
}
