//! Local, opt-in MCP integration for the running Mash application.

mod connection;
mod formation;
mod tools;
mod transport;

use std::sync::{Arc, Mutex};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::sync::{oneshot, Mutex as AsyncMutex};

#[derive(Default)]
pub(crate) struct McpServerState(AsyncMutex<Option<RunningServer>>);

struct RunningServer {
    endpoint: String,
    battle_permission: Arc<BattleRunPermission>,
    token: String,
    shutdown: oneshot::Sender<()>,
    task: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpServerStatus {
    endpoint: Option<String>,
    allow_battle_runs: bool,
}

impl RunningServer {
    fn status(&self) -> Result<McpServerStatus, String> {
        Ok(McpServerStatus {
            endpoint: Some(self.endpoint.clone()),
            allow_battle_runs: self.battle_permission.is_allowed()?,
        })
    }
}

struct BattleRunPermission(Mutex<bool>);

impl BattleRunPermission {
    fn is_allowed(&self) -> Result<bool, String> {
        Ok(*self.0.lock().map_err(|error| error.to_string())?)
    }

    fn set_allowed(&self, allowed: bool) -> Result<(), String> {
        *self.0.lock().map_err(|error| error.to_string())? = allowed;
        Ok(())
    }

    fn start<T>(&self, start: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let allowed = self.0.lock().map_err(|error| error.to_string())?;
        if !*allowed {
            return Err("请在 Mash 的 MCP 设置中打开「允许运行项目」".into());
        }
        // Hold the permission until the runner accepts the start. Revocation then
        // takes effect before any further start, including already queued requests.
        start()
    }
}

#[tauri::command]
pub(crate) async fn get_mcp_server_status(
    state: tauri::State<'_, McpServerState>,
) -> Result<McpServerStatus, String> {
    let running = state.0.lock().await;
    match running.as_ref() {
        Some(server) => server.status(),
        None => Ok(McpServerStatus::default()),
    }
}

#[tauri::command]
pub(crate) async fn start_mcp_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, McpServerState>,
    allow_battle_runs: bool,
) -> Result<McpServerStatus, String> {
    let mut running = state.0.lock().await;
    if let Some(server) = running.as_ref() {
        return server.status();
    }
    let battle_permission = Arc::new(BattleRunPermission(Mutex::new(allow_battle_runs)));

    let config_path = crate::paths::app_data_dir(&app).join("mcp-connection.json");
    let (socket, config) = connection::Connection::listen(&config_path)?;
    let address = socket.local_addr().map_err(|e| e.to_string())?;
    let listener = tokio::net::TcpListener::from_std(socket).map_err(|e| e.to_string())?;
    let (shutdown, shutdown_request) = oneshot::channel();
    let task = tauri::async_runtime::spawn(transport::serve(
        listener,
        address,
        config.token.clone(),
        tools::McpTools::new(app, Arc::clone(&battle_permission)),
        shutdown_request,
    ));
    let endpoint = format!("http://{address}/mcp");
    *running = Some(RunningServer {
        endpoint: endpoint.clone(),
        battle_permission,
        token: config.token,
        shutdown,
        task,
    });
    Ok(McpServerStatus {
        endpoint: Some(endpoint),
        allow_battle_runs,
    })
}

#[tauri::command]
pub(crate) async fn set_mcp_battle_permission(
    state: tauri::State<'_, McpServerState>,
    allow_battle_runs: bool,
) -> Result<McpServerStatus, String> {
    let running = state.0.lock().await;
    let server = running.as_ref().ok_or("MCP 服务尚未开启")?;
    server.battle_permission.set_allowed(allow_battle_runs)?;
    server.status()
}

#[tauri::command]
pub(crate) async fn stop_mcp_server(
    state: tauri::State<'_, McpServerState>,
) -> Result<McpServerStatus, String> {
    let mut running = state.0.lock().await;
    if let Some(server) = running.take() {
        let _ = server.battle_permission.set_allowed(false);
        let _ = server.shutdown.send(());
        // The next start can bind the saved port only after the listener shuts down.
        let _ = server.task.await;
    }
    Ok(McpServerStatus::default())
}

#[tauri::command]
pub(crate) async fn copy_mcp_client_config(
    app: tauri::AppHandle,
    state: tauri::State<'_, McpServerState>,
    part: Option<McpClientConfigPart>,
) -> Result<(), String> {
    let config = {
        let running = state.0.lock().await;
        let server = running.as_ref().ok_or("MCP 服务尚未开启")?;
        part.unwrap_or_default()
            .clipboard_text(&server.endpoint, &server.token)
    };
    app.clipboard()
        .write_text(config)
        .map_err(|_| "复制 MCP 配置失败".into())
}

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum McpClientConfigPart {
    Address,
    Token,
    #[default]
    Codex,
}

impl McpClientConfigPart {
    fn clipboard_text(self, endpoint: &str, token: &str) -> String {
        match self {
            Self::Address => endpoint.to_owned(),
            Self::Token => token.to_owned(),
            Self::Codex => format!(
                "[mcp_servers.mash]\nurl = \"{endpoint}\"\n\n[mcp_servers.mash.http_headers]\nAuthorization = \"Bearer {token}\"\n"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_tool_handlers_use_the_current_battle_permission() {
        let permission = Arc::new(BattleRunPermission(Mutex::new(false)));
        let handler_permission = Arc::clone(&permission);
        let denied = "请在 Mash 的 MCP 设置中打开「允许运行项目」";
        let start = || Ok("started");
        assert_eq!(handler_permission.start(start), Err(denied.into()));
        permission.set_allowed(true).unwrap();
        assert_eq!(handler_permission.start(start), Ok("started"));
        permission.set_allowed(false).unwrap();
        assert_eq!(handler_permission.start(start), Err(denied.into()));
    }

    #[test]
    fn copies_individual_connection_fields_and_the_complete_codex_config() {
        let endpoint = "http://127.0.0.1:44001/mcp";
        let token = "test-token";
        let copy = |part: &str| {
            serde_json::from_str::<McpClientConfigPart>(part)
                .unwrap()
                .clipboard_text(endpoint, token)
        };
        assert_eq!(copy(r#""address""#), endpoint);
        assert_eq!(copy(r#""token""#), token);
        let config = copy(r#""codex""#);
        assert_eq!(
            config,
            McpClientConfigPart::default().clipboard_text(endpoint, token)
        );
        assert!(config.contains("url = \"http://127.0.0.1:44001/mcp\""));
        assert!(config.contains("Authorization = \"Bearer test-token\""));
    }
}
