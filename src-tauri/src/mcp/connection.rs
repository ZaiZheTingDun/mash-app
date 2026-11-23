//! Stable local address and credential for external MCP clients.

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::path::Path;

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct Connection {
    port: u16,
    pub(super) token: String,
}

impl Connection {
    pub(super) fn listen(path: &Path) -> Result<(TcpListener, Self), String> {
        open_connection(path, |port| {
            let socket = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))?;
            socket.set_nonblocking(true)?;
            let port = socket.local_addr()?.port();
            Ok((socket, port))
        })
    }

    fn load(path: &Path) -> Result<Option<Self>, String> {
        let saved = match std::fs::read(path) {
            Ok(saved) => saved,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("读取 MCP 连接配置失败：{error}")),
        };
        let config: Self = serde_json::from_slice(&saved)
            .map_err(|_| "MCP 连接配置格式错误，请检查应用数据目录中的 mcp-connection.json")?;
        if config.port == 0
            || config.token.len() != 32
            || !config.token.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("MCP 连接配置无效，请检查应用数据目录中的 mcp-connection.json".into());
        }
        Ok(Some(config))
    }
}

fn open_connection<T>(
    path: &Path,
    bind: impl Fn(u16) -> io::Result<(T, u16)>,
) -> Result<(T, Connection), String> {
    let config = match Connection::load(path)? {
        Some(config) => config,
        None => {
            let (socket, port) = bind(0).map_err(|error| format!("无法启动 MCP 服务：{error}"))?;
            let config = Connection {
                port,
                token: uuid::Uuid::new_v4().simple().to_string(),
            };
            if crate::storage::create_json_atomic(path, &config, "MCP 连接配置")? {
                return Ok((socket, config));
            }
            // Another instance published its address and token while this one was binding.
            drop(socket);
            Connection::load(path)?.ok_or("MCP 连接配置已被移除，请重试")?
        }
    };
    let (socket, _) = bind(config.port).map_err(|error| match error.kind() {
        io::ErrorKind::AddrInUse => format!(
            "MCP 端口 {} 已被占用。请关闭正在使用该端口的程序，再启动 MCP 服务。",
            config.port
        ),
        _ => format!("无法在端口 {} 启动 MCP 服务：{error}", config.port),
    })?;
    Ok((socket, config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn reopening_uses_the_same_port_and_token_without_writing_a_new_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-connection.json");
        let (_, first) = open_connection(&path, |port| {
            assert_eq!(port, 0);
            Ok(((), 44213))
        })
        .unwrap();
        let saved = std::fs::read(&path).unwrap();
        let (_, second) = open_connection(&path, |port| {
            assert_eq!(port, first.port);
            Ok(((), port))
        })
        .unwrap();
        assert_eq!(first.port, second.port);
        assert!(first.token == second.token);
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn occupied_port_keeps_the_existing_client_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-connection.json");
        open_connection(&path, |_| Ok(((), 44213))).unwrap();
        let saved = std::fs::read(&path).unwrap();
        let calls = Cell::new(0);
        let error = open_connection::<()>(&path, |port| {
            assert_eq!(port, 44213);
            calls.set(calls.get() + 1);
            Err(io::ErrorKind::AddrInUse.into())
        })
        .err()
        .unwrap();
        assert!(error.contains("44213 已被占用"));
        assert_eq!(calls.get(), 1);
        assert_eq!(std::fs::read(path).unwrap(), saved);
    }

    #[test]
    fn simultaneous_first_start_uses_the_other_instances_saved_connection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-connection.json");
        let (_, connection) = open_connection(&path, |port| {
            if port == 0 {
                open_connection(&path, |_| Ok(((), 44213))).unwrap();
                return Ok(((), 44214));
            }
            assert_eq!(port, 44213);
            Ok(((), port))
        })
        .unwrap();
        assert_eq!(connection.port, 44213);
        assert!(Connection::load(&path).unwrap().unwrap().token == connection.token);
    }

    #[test]
    fn malformed_credentials_are_preserved_and_excluded_from_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-connection.json");
        let saved = br#"{"port": "private-data"}"#;
        std::fs::write(&path, saved).unwrap();
        let error = open_connection::<()>(&path, |_| panic!("should not open a different port"))
            .err()
            .unwrap();
        assert!(error.contains("MCP 连接配置格式错误"));
        assert!(!error.contains("private-data"));
        assert_eq!(std::fs::read(path).unwrap(), saved);
    }
}
