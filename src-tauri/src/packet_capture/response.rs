use super::{client, find_root, root_shell, shell_quote, AdbCommand};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD},
    Engine,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
};

const MAX_BODY: usize = 16 * 1024 * 1024;
const MAX_RECORD: usize = 24 * 1024 * 1024;
const MAX_IDS: usize = 128;

#[derive(Clone, Default, Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ResponsePhase {
    #[default]
    Idle,
    Waiting,
    Found,
    Error,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResponseState {
    phase: ResponsePhase,
    serial: Option<String>,
    message: String,
    response: Option<LoginResponse>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginResponse {
    record_id: u64,
    url: String,
    status_code: u16,
    byte_length: usize,
    format: String,
    body: String,
}

#[derive(Default)]
struct Inner {
    generation: u64,
    baseline: HashSet<String>,
    state: ResponseState,
}

#[derive(Clone, Default)]
pub(crate) struct ResponseStore(Arc<Mutex<Inner>>);

impl ResponseStore {
    pub(crate) fn reset(&self) {
        let mut inner = self.0.lock().unwrap();
        inner.generation = inner.generation.wrapping_add(1);
        inner.baseline.clear();
        inner.state = ResponseState::default();
    }

    pub(crate) fn begin(&self, serial: String, baseline: HashSet<String>) {
        let mut inner = self.0.lock().unwrap();
        inner.generation = inner.generation.wrapping_add(1);
        inner.baseline = baseline;
        inner.state = ResponseState {
            phase: ResponsePhase::Waiting,
            serial: Some(serial),
            message: "等待国服 toplogin 响应，请重新进入游戏…".into(),
            response: None,
        };
    }

    pub(crate) fn poll(&self, adb_path: &Path) -> ResponseState {
        self.poll_using(
            |serial, baseline| {
                let client = client::Client::new()?;
                client::on_device(adb_path, serial, |port, run| {
                    scan(&client, port, serial, baseline, run)
                })
            },
            |serial| super::stop::stop_capture(adb_path, serial),
        )
    }

    fn poll_using(
        &self,
        scan: impl FnOnce(&str, &HashSet<String>) -> Result<Option<LoginResponse>, String>,
        stop: impl FnOnce(&str) -> Result<(), String>,
    ) -> ResponseState {
        let (generation, serial, baseline) = {
            let inner = self.0.lock().unwrap();
            if inner.state.phase != ResponsePhase::Waiting {
                return inner.state.clone();
            }
            (
                inner.generation,
                inner.state.serial.clone().unwrap(),
                inner.baseline.clone(),
            )
        };
        let result = scan(&serial, &baseline);
        let mut inner = self.0.lock().unwrap();
        // A configuration change or a new start supersedes any in-flight poll.
        if inner.generation == generation && inner.state.phase == ResponsePhase::Waiting {
            match result {
                Ok(Some(response)) => {
                    inner.state.response = Some(response);
                    // Keep the complete body before stopping: Reqable may release
                    // its temporary files. Serialize stop with reset/begin so a
                    // stale poll cannot switch off a newer capture session.
                    match stop(&serial) {
                        Ok(()) => {
                            inner.state.phase = ResponsePhase::Found;
                            inner.state.message = "已获取国服 toplogin 响应，抓包已停止".into();
                        }
                        Err(error) => {
                            inner.state.phase = ResponsePhase::Error;
                            inner.state.message = format!("已获取国服 toplogin 响应；{error}");
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    inner.state.phase = ResponsePhase::Error;
                    inner.state.message = format!("{error}；请重新开始抓包后登录游戏");
                }
            }
        }
        inner.state.clone()
    }
}

#[derive(Deserialize)]
struct Record {
    id: u64,
    uid: String,
    url: String,
    response: Option<Response>,
}
#[derive(Deserialize)]
struct Response {
    code: Option<u16>,
    body: Option<Body>,
}
#[derive(Deserialize)]
struct Body {
    text: String,
    encoding: String,
}

fn target_url(raw: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(raw).ok()?;
    let host = url.host_str()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || !host.ends_with(".bilibiligame.net")
        || ![
            "-ios-fate.bilibiligame.net",
            "-bili-fate.bilibiligame.net",
            "-uo-fate.bilibiligame.net",
        ]
        .iter()
        .any(|suffix| host.ends_with(suffix))
        || url.port_or_known_default() != Some(443)
    {
        return None;
    }
    let parts: Vec<_> = url
        .path()
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() != 4
        || parts[0] != "rongame_beta"
        || parts[1] != "rgfate"
        || parts[3] != "ac.php"
    {
        return None;
    }
    let keys: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| key == "_key")
        .map(|(_, value)| value.into_owned())
        .collect();
    if keys != ["toplogin"] {
        return None;
    }
    // Keep only the routing selector in the displayed URL, never account IDs/tokens.
    url.set_query(Some("_key=toplogin"));
    url.set_fragment(None);
    Some(url.to_string())
}

fn ids(client: &client::Client, port: u16) -> Result<Vec<u64>, String> {
    let bytes = client.request(
        port,
        "filter",
        Some(&serde_json::json!({"filters":[{"type":"keyword","pattern":"_key=toplogin"}]})),
        128 * 1024,
    )?;
    let ids: Vec<u64> = serde_json::from_slice(&bytes).map_err(|_| "Reqable 记录列表无效")?;
    if ids.len() > MAX_IDS {
        return Err("匹配记录过多，请清理 Reqable 旧记录后重试".into());
    }
    Ok(ids)
}

fn record(client: &client::Client, port: u16, id: u64) -> Result<Record, String> {
    let bytes = client.request(port, "get", Some(&serde_json::json!({"id":id})), MAX_RECORD)?;
    let record: Record = serde_json::from_slice(&bytes).map_err(|_| "Reqable 记录格式无效")?;
    if record.id != id || record.uid.is_empty() {
        return Err("Reqable 记录标识无效".into());
    }
    Ok(record)
}

pub(super) fn baseline(client: &client::Client, port: u16) -> Result<HashSet<String>, String> {
    let mut baseline = HashSet::new();
    for id in ids(client, port)? {
        let record = record(client, port, id)?;
        if target_url(&record.url).is_some() {
            baseline.insert(record.uid);
        }
    }
    Ok(baseline)
}

fn scan(
    client: &client::Client,
    port: u16,
    serial: &str,
    baseline: &HashSet<String>,
    run: &mut AdbCommand<'_>,
) -> Result<Option<LoginResponse>, String> {
    // Latest completed response wins; pending records are retried on the next poll.
    for id in ids(client, port)?.into_iter().rev() {
        let record = record(client, port, id)?;
        if baseline.contains(&record.uid) {
            continue;
        }
        let Some(url) = target_url(&record.url) else {
            continue;
        };
        let Some(response) = record.response else {
            continue;
        };
        let Some(code) = response.code else {
            continue;
        };
        let Some(body) = response.body else {
            continue;
        };
        if !(200..300).contains(&code) {
            return Err(format!("toplogin 请求失败（HTTP {code}）"));
        }
        let bytes = body_bytes(body, &mut |path| read_cache(serial, path, run))?;
        if bytes.is_empty() {
            continue;
        }
        let byte_length = bytes.len();
        let (format, body) = display_body(bytes)?;
        return Ok(Some(LoginResponse {
            record_id: id,
            url,
            status_code: code,
            byte_length,
            format,
            body,
        }));
    }
    if !super::capture::decode_status(&client.request(port, "status", None, 4096)?)? {
        return Err("Reqable 抓包已停止".into());
    }
    Ok(None)
}

fn body_bytes(
    body: Body,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let bytes = match body.encoding.as_str() {
        "utf8" => body.text.into_bytes(),
        "base64" => STANDARD
            .decode(body.text)
            .map_err(|_| "响应的 Base64 编码无效")?,
        "file" => read(&cache_path(&body.text)?)?,
        _ => return Err("Reqable 响应使用了不支持的编码".into()),
    };
    if bytes.len() > MAX_BODY {
        return Err("响应超过 16 MiB，未截断展示，请在 Reqable 中查看".into());
    }
    Ok(bytes)
}

fn cache_path(raw: &str) -> Result<String, String> {
    let path = if raw.starts_with("file:") {
        reqwest::Url::parse(raw)
            .ok()
            .filter(|url| url.host_str().is_none())
            .map(|url| url.path().to_string())
    } else {
        Some(raw.to_string())
    }
    .ok_or("Reqable 缓存路径无效")?;
    // Only Reqable's response cache/temporary directories, never certificate keys.
    if ![
        "/data/user/0/com.reqable.android/cache/",
        "/data/data/com.reqable.android/cache/",
        "/data/user/0/com.reqable.android/files/tmp/",
        "/data/data/com.reqable.android/files/tmp/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
        || path.contains(['\0', '\n', '\r', '\\', '%'])
        || path.split('/').any(|part| matches!(part, "." | ".."))
    {
        return Err("Reqable 响应缓存路径不在允许的目录中".into());
    }
    Ok(path)
}

fn read_cache(serial: &str, path: &str, run: &mut AdbCommand<'_>) -> Result<Vec<u8>, String> {
    let mut shell = |command: &str| run(&["-s", serial, "shell", command]);
    let root = find_root(&mut shell)?;
    let path = shell_quote(path);
    let resolved = shell(&root_shell(root, &format!("readlink -f {path}")))
        .map_err(|_| "无法解析响应缓存文件")?;
    let resolved = String::from_utf8(resolved).map_err(|_| "响应缓存路径无效")?;
    let resolved = cache_path(resolved.trim())?;
    let resolved = shell_quote(&resolved);
    let size = shell(&root_shell(
        root,
        &format!("test -f {resolved} && stat -c %s {resolved}"),
    ))
    .map_err(|_| "响应缓存文件已失效")?;
    let size: usize = String::from_utf8_lossy(&size)
        .trim()
        .parse()
        .map_err(|_| "无法读取响应缓存大小")?;
    if size > MAX_BODY {
        return Err("响应超过 16 MiB，请在 Reqable 中查看".into());
    }
    let command = root_shell(root, &format!("head -c {} {resolved}", MAX_BODY + 1));
    let bytes = run(&["-s", serial, "exec-out", &command]).map_err(|_| "无法读取响应缓存文件")?;
    if bytes.len() != size {
        return Err("响应缓存读取不完整，请重新登录后重试".into());
    }
    Ok(bytes)
}

fn display_body(mut bytes: Vec<u8>) -> Result<(String, String), String> {
    // Reqable may already decompress HTTP content; only decode a real gzip stream.
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut expanded = Vec::new();
        flate2::read::GzDecoder::new(bytes.as_slice())
            .take((MAX_BODY + 1) as u64)
            .read_to_end(&mut expanded)
            .map_err(|_| "响应 gzip 解压失败")?;
        if expanded.len() > MAX_BODY {
            return Err("解压后的响应超过 16 MiB".into());
        }
        bytes = expanded;
    }
    match String::from_utf8(bytes) {
        Ok(text) => {
            // CN toplogin responses are commonly URL-escaped Base64 JSON.
            // Decode only if the result is valid JSON, otherwise preserve the text.
            let decoded = percent_decode(&text).and_then(|value| {
                URL_SAFE
                    .decode(&value)
                    .or_else(|_| URL_SAFE_NO_PAD.decode(&value))
                    .or_else(|_| STANDARD.decode(&value))
                    .ok()
            });
            let json = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .or_else(|| decoded.and_then(|bytes| serde_json::from_slice(&bytes).ok()));
            if let Some(json) = json {
                let pretty =
                    serde_json::to_string_pretty(&json).map_err(|_| "无法显示响应 JSON")?;
                Ok(("JSON".into(), pretty))
            } else {
                Ok(("文本".into(), text))
            }
        }
        Err(error) => Ok((
            "Base64（二进制响应）".into(),
            STANDARD.encode(error.into_bytes()),
        )),
    }
}

fn percent_decode(text: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut input = text.trim().as_bytes().iter().copied();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = (input.next()? as char).to_digit(16)?;
            let low = (input.next()? as char).to_digit(16)?;
            bytes.push((high * 16 + low) as u8);
        } else {
            bytes.push(byte);
        }
    }
    Some(bytes)
}

#[cfg(test)]
mod tests;
