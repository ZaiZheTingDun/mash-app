//! Reqable's device-local API. No proxy, redirects, payload logging or disk cache.
use super::{run_adb, AdbCommand};
use std::{io::Read, path::Path, time::Duration};

pub(super) struct Client(reqwest::blocking::Client);

impl Client {
    pub(super) fn new() -> Result<Self, String> {
        reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(30))
            .user_agent("reqable-mcp/1.0.1")
            .build()
            .map(Self)
            .map_err(|_| "无法创建 Reqable 控制连接".into())
    }

    pub(super) fn request(
        &self,
        port: u16,
        route: &str,
        body: Option<&serde_json::Value>,
        limit: usize,
    ) -> Result<Vec<u8>, String> {
        let url = format!("http://127.0.0.1:{port}/capture/live/{route}");
        let request = if route == "status" {
            self.0.get(url)
        } else {
            let request = self.0.post(url);
            match body {
                Some(body) => request
                    .header("Content-Type", "application/json")
                    .body(body.to_string()),
                None => request,
            }
        };
        let response = request.send().map_err(|error| {
            if error.is_timeout() {
                if error.is_connect() {
                    "连接设备上的 Reqable 控制接口超时"
                } else {
                    "Reqable 控制接口响应超时，请保持 Reqable 运行后重试"
                }
            } else {
                "无法连接设备上的 Reqable 控制接口"
            }
        })?;
        let status = response.status();
        let mut bytes = Vec::new();
        response
            .take((limit + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "无法读取 Reqable 响应")?;
        if bytes.len() > limit {
            return Err("Reqable 响应超过读取上限".into());
        }
        if !status.is_success() {
            return Err(
                if status == reqwest::StatusCode::FORBIDDEN
                    && String::from_utf8_lossy(&bytes).contains("Please enable API Service")
                {
                    "Reqable API 服务未启用，请在 Reqable 设置中开启 API Service（API 服务）后重试"
                        .into()
                } else if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::FORBIDDEN
                {
                    "Reqable 控制接口认证失败，请检查 API 服务的访问令牌设置".into()
                } else {
                    format!("Reqable 控制接口返回 HTTP {}", status.as_u16())
                },
            );
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests;

pub(super) fn forwarded<T>(
    serial: &str,
    run: &mut AdbCommand<'_>,
    action: impl FnOnce(u16, &mut AdbCommand<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let output = run(&["-s", serial, "forward", "tcp:0", "tcp:9016"])?;
    let port: u16 = String::from_utf8_lossy(&output)
        .trim()
        .parse()
        .ok()
        .filter(|port| *port != 0)
        .ok_or("无法建立 Reqable 本机控制连接")?;
    let result = action(port, run);
    let _ = run(&["-s", serial, "forward", "--remove", &format!("tcp:{port}")]);
    result
}

pub(super) fn on_device<T>(
    adb_path: &Path,
    serial: &str,
    action: impl FnOnce(u16, &mut AdbCommand<'_>) -> Result<T, String>,
) -> Result<T, String> {
    forwarded(serial, &mut |args| run_adb(adb_path, args), action)
}
