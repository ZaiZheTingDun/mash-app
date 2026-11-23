//! Stateless Streamable HTTP transport. All requests are authenticated locally.

use http_body_util::{combinators::BoxBody, BodyExt, Empty};
use hyper::body::Body;
use hyper::body::Bytes;
use hyper::header::{AUTHORIZATION, HOST, ORIGIN, WWW_AUTHENTICATE};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use rmcp::transport::streamable_http_server::{
    session::never::NeverSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::ServerHandler;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinSet;

const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) async fn serve(
    listener: TcpListener,
    address: SocketAddr,
    token: String,
    tools: impl ServerHandler + Clone,
    mut shutdown: oneshot::Receiver<()>,
) {
    let access = Arc::new(LocalAccess::new(address, token));
    let service = local_service(address, tools);
    let mut connections = JoinSet::new();
    loop {
        while connections.try_join_next().is_some() {}
        let accepted = tokio::select! {
            biased;
            _ = &mut shutdown => break,
            accepted = listener.accept() => accepted,
        };
        let (socket, _) = match accepted {
            Ok(connection) => connection,
            Err(error) => {
                eprintln!("[mcp] local accept failed: {error}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let access = access.clone();
        let service = service.clone();
        connections.spawn(async move {
            let service = hyper::service::service_fn(move |request| {
                let access = access.clone();
                let service = service.clone();
                async move { Ok::<_, Infallible>(handle_request(request, &access, &service).await) }
            });
            let _ = hyper::server::conn::http1::Builder::new()
                .timer(TokioTimer::new())
                .header_read_timeout(READ_TIMEOUT)
                .serve_connection(TokioIo::new(socket), service)
                .await;
        });
    }
    drop(listener);
    connections.shutdown().await;
}

fn local_service<S: ServerHandler + Clone>(
    address: SocketAddr,
    tools: S,
) -> StreamableHttpService<S, NeverSessionManager> {
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts([address.to_string()])
        .with_allowed_origins([format!("http://{address}")])
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_max_request_body_bytes(MAX_BODY_BYTES);
    StreamableHttpService::new(
        move || Ok(tools.clone()),
        Arc::new(NeverSessionManager::default()),
        config,
    )
}

async fn handle_request<B: Body + Send + 'static>(
    request: Request<B>,
    access: &LocalAccess,
    service: &StreamableHttpService<impl ServerHandler, NeverSessionManager>,
) -> Response<BoxBody<Bytes, Infallible>>
where
    B::Error: std::fmt::Display,
{
    if request.uri().path() != "/mcp" {
        return empty_response(StatusCode::NOT_FOUND);
    }
    if let Err(status) = access.authorize(&request) {
        return empty_response(status);
    }
    service.handle(request).await
}

struct LocalAccess {
    authority: String,
    origin: String,
    authorization: String,
}

impl LocalAccess {
    fn new(address: SocketAddr, token: String) -> Self {
        Self {
            authority: address.to_string(),
            origin: format!("http://{address}"),
            authorization: format!("Bearer {token}"),
        }
    }

    fn authorize(&self, request: &Request<impl hyper::body::Body>) -> Result<(), StatusCode> {
        let headers = request.headers();
        let valid_host = headers.get_all(HOST).iter().count() == 1
            && headers
                .get(HOST)
                .is_some_and(|host| host == self.authority.as_str())
            && request
                .uri()
                .authority()
                .is_none_or(|authority| authority.as_str() == self.authority);
        let valid_origin = headers.get_all(ORIGIN).iter().count() <= 1
            && headers
                .get(ORIGIN)
                .is_none_or(|origin| origin == self.origin.as_str());
        if !valid_host || !valid_origin {
            return Err(StatusCode::FORBIDDEN);
        }
        let authorized = headers.get_all(AUTHORIZATION).iter().count() == 1
            && headers
                .get(AUTHORIZATION)
                .is_some_and(|value| value.as_bytes().ct_eq(self.authorization.as_bytes()).into());
        if !authorized {
            return Err(StatusCode::UNAUTHORIZED);
        }
        Ok(())
    }
}

fn empty_response(status: StatusCode) -> Response<BoxBody<Bytes, Infallible>> {
    let mut builder = Response::builder().status(status);
    if status == StatusCode::UNAUTHORIZED {
        builder = builder.header(WWW_AUTHENTICATE, "Bearer");
    }
    builder.body(Empty::new().boxed()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::Full;
    use rmcp::model::{
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    };
    use rmcp::{service::RequestContext, ErrorData, RoleServer};
    use serde_json::{json, Value};

    #[derive(Clone)]
    struct TestTools;

    impl ServerHandler for TestTools {
        fn get_info(&self) -> ServerConfig {
            ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
        }

        async fn list_tools(
            &self,
            _: Option<PaginatedRequestParams>,
            _: RequestContext<RoleServer>,
        ) -> Result<ListToolsResult, ErrorData> {
            Ok(ListToolsResult::with_all_items(vec![Tool::new(
                "test_tool",
                "A test tool",
                json!({"type":"object"}).as_object().unwrap().clone(),
            )]))
        }
    }

    #[test]
    fn authenticated_client_can_initialize_and_discover_tools() {
        tauri::async_runtime::block_on(async {
            let address = "127.0.0.1:4567".parse().unwrap();
            let service = local_service(address, TestTools);
            let access = LocalAccess::new(address, "secret".into());
            let request = |body: Value| {
                Request::post("/mcp")
                    .header(HOST, address.to_string())
                    .header(AUTHORIZATION, "Bearer secret")
                    .header("accept", "application/json, text/event-stream")
                    .header("content-type", "application/json")
                    .header("mcp-protocol-version", "2025-11-25")
                    .body(Full::new(Bytes::from(body.to_string())))
                    .unwrap()
            };
            let mut invalid = request(json!({"jsonrpc":"2.0", "id":1, "method":"tools/list"}));
            invalid.headers_mut().remove(AUTHORIZATION);
            assert_eq!(
                handle_request(invalid, &access, &service).await.status(),
                StatusCode::UNAUTHORIZED
            );

            let initialize = request(
                json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{
                    "protocolVersion":"2025-11-25", "capabilities":{}, "clientInfo":{"name":"test", "version":"1"}
                }}),
            );
            let response = handle_request(initialize, &access, &service).await;
            assert_eq!(response.status(), StatusCode::OK);
            let initialized: Value =
                serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");

            let list = request(json!({"jsonrpc":"2.0", "id":2, "method":"tools/list"}));
            let response = handle_request(list, &access, &service).await;
            assert_eq!(response.status(), StatusCode::OK);
            let listed: Value =
                serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(listed["result"]["tools"][0]["name"], "test_tool");
        });
    }

    #[test]
    fn requires_the_local_host_a_valid_origin_and_one_token() {
        let access = LocalAccess::new("127.0.0.1:4567".parse().unwrap(), "secret".into());
        let request = |host: &str| {
            Request::post("/mcp")
                .header(HOST, host)
                .header(AUTHORIZATION, "Bearer secret")
                .body(Empty::<Bytes>::new())
                .unwrap()
        };
        assert_eq!(access.authorize(&request("127.0.0.1:4567")), Ok(()));
        assert_eq!(
            access.authorize(&request("example.com")),
            Err(StatusCode::FORBIDDEN)
        );
        let mut foreign = request("127.0.0.1:4567");
        foreign
            .headers_mut()
            .insert(ORIGIN, "https://example.com".parse().unwrap());
        assert_eq!(access.authorize(&foreign), Err(StatusCode::FORBIDDEN));
        let mut missing_token = request("127.0.0.1:4567");
        missing_token.headers_mut().remove(AUTHORIZATION);
        assert_eq!(
            access.authorize(&missing_token),
            Err(StatusCode::UNAUTHORIZED)
        );
        let mut duplicate = request("127.0.0.1:4567");
        duplicate
            .headers_mut()
            .append(AUTHORIZATION, "Bearer secret".parse().unwrap());
        assert_eq!(access.authorize(&duplicate), Err(StatusCode::UNAUTHORIZED));
    }
}
