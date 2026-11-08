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
    delayed_route_request(client, status, body, delay, "status", None)
}

fn delayed_route_request(
    client: Client,
    status: &str,
    body: &str,
    delay: Duration,
    route: &str,
    timeout: Option<Duration>,
) -> Result<Vec<u8>, String> {
    delayed_route_request_typed(client, status, body, delay, route, timeout)
        .map_err(|error| error.to_string())
}

fn delayed_route_request_typed(
    client: Client,
    status: &str,
    body: &str,
    delay: Duration,
    route: &str,
    timeout: Option<Duration>,
) -> Result<Vec<u8>, Error> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let authorization = client
        .1
        .as_ref()
        .map(|value| value.to_str().unwrap().to_owned());
    let expected = format!(
        "{} /capture/live/{route} HTTP/1.1\r\n",
        if route == "status" { "GET" } else { "POST" }
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
        assert!(request.starts_with(expected.as_bytes()));
        let headers = String::from_utf8(request).unwrap().to_ascii_lowercase();
        if let Some(authorization) = authorization {
            assert!(headers.contains(&format!(
                "authorization: {}\r\n",
                authorization.to_ascii_lowercase()
            )));
        } else {
            assert!(!headers.contains("authorization:"));
        }
        std::thread::sleep(delay);
        // Timeout cases intentionally close the connection before the reply.
        let _ = stream.write_all(response.as_bytes());
    });
    let result = match timeout {
        Some(timeout) => client.request_with_timeout_typed(port, route, None, 4096, timeout),
        None => client.request_typed(port, route, None, 4096),
    };
    server.join().unwrap();
    result
}

#[test]
fn authenticated_requests_send_a_sensitive_bearer_header() {
    let client = Client::with_api_key(Some("synthetic-private-key")).unwrap();
    assert!(client.1.as_ref().unwrap().is_sensitive());
    assert!(!format!("{:?}", client.1).contains("synthetic-private-key"));
    assert_eq!(
        delayed_request(client, "200 OK", r#"{"status":"inactive"}"#, Duration::ZERO).unwrap(),
        br#"{"status":"inactive"}"#
    );
    assert!(Client::with_api_key(Some("bad\r\nInjected: true")).is_err());
}

#[test]
fn disabled_api_service_has_an_actionable_error() {
    let error = request(
        "403 Forbidden",
        "Access denied. Please enable API Service in Reqable settings.",
    )
    .unwrap_err();
    assert_eq!(
        error,
        "Reqable API 服务未启用，请在 Reqable 设置中开启 API 服务后重试"
    );
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
    for route in ["status", "off"] {
        let error = delayed_route_request_typed(
            Client::with_api_key(Some("synthetic-private-key")).unwrap(),
            "200 OK",
            "{}",
            Duration::from_millis(250),
            route,
            Some(Duration::from_millis(100)),
        )
        .unwrap_err();
        assert_eq!(error, Error::ResponseTimeout);
        assert!(is_transient_error(&error.to_string()));
    }
}
