use super::{client, find_root, root_shell, shell_quote, target, AdbCommand};
use crate::Server;
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
    time::{Duration, Instant},
};

const MAX_BODY: usize = 16 * 1024 * 1024;
const MAX_RECORD: usize = 24 * 1024 * 1024;
// Retain the existing bounded filter payload, rather than an arbitrary record count.
const MAX_ID_BYTES: usize = 128 * 1024;

#[derive(Clone, Default, Debug)]
pub(super) struct Baseline {
    pub(super) uids: HashSet<String>,
    pub(super) ignored_ids: HashSet<u64>,
    pub(super) warnings: Vec<String>,
}

impl Baseline {
    pub(super) fn contains(&self, uid: &str) -> bool {
        self.uids.contains(uid)
    }
    fn warn(&mut self, warning: String) {
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }
}

impl From<HashSet<String>> for Baseline {
    fn from(uids: HashSet<String>) -> Self {
        Self {
            uids,
            ..Default::default()
        }
    }
}

struct ScanOutcome {
    response: Option<LoginResponse>,
    baseline: Baseline,
}

#[derive(Debug)]
enum ScanError {
    Retryable(String),
    Fatal(String),
    InvalidRecord(String),
}

impl ScanError {
    fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal(_))
    }
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retryable(message) | Self::Fatal(message) | Self::InvalidRecord(message) => {
                f.write_str(message)
            }
        }
    }
}

// ADB forwarding and client creation failures remain retryable.
impl From<String> for ScanError {
    fn from(message: String) -> Self {
        Self::Retryable(message)
    }
}

impl From<client::Error> for ScanError {
    fn from(error: client::Error) -> Self {
        match error {
            client::Error::AuthenticationFailed
            | client::Error::ApiDisabled
            | client::Error::ResponseTooLarge => Self::Fatal(error.to_string()),
            _ => Self::Retryable(error.to_string()),
        }
    }
}

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
    pub(super) phase: ResponsePhase,
    pub(super) serial: Option<String>,
    pub(super) message: String,
    pub(super) response: Option<LoginResponse>,
    pub(super) warnings: Vec<String>,
    pub(super) retry_after_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResponseStatus {
    phase: ResponsePhase,
    serial: Option<String>,
    message: String,
    warnings: Vec<String>,
    retry_after_ms: u64,
}

impl ResponseState {
    pub(crate) fn status(&self) -> ResponseStatus {
        ResponseStatus {
            phase: self.phase.clone(),
            serial: self.serial.clone(),
            message: self.message.clone(),
            warnings: self.warnings.clone(),
            retry_after_ms: self.retry_after_ms,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LoginResponse {
    pub(super) capture_id: String,
    pub(super) captured_at: u64,
    record_id: u64,
    url: String,
    status_code: u16,
    byte_length: usize,
    format: String,
    pub(super) body: String,
    pub(super) items: Vec<super::inventory::CapturedItem>,
}

impl LoginResponse {
    /// Heap allocations retained by a history record, in addition to the struct itself.
    pub(super) fn allocated_bytes(&self) -> usize {
        self.capture_id.capacity()
            + self.url.capacity()
            + self.format.capacity()
            + self.body.capacity()
            + self.items.capacity() * std::mem::size_of::<super::inventory::CapturedItem>()
            + self
                .items
                .iter()
                .map(|item| item.name.capacity())
                .sum::<usize>()
    }
}

#[derive(Default)]
struct Inner {
    generation: u64,
    server: Server,
    baseline: Baseline,
    failures: u32,
    game_launch_failed: bool,
    next_poll: Option<Instant>,
    state: ResponseState,
    interrupted_stop_target: Option<String>,
    api_key: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct ResponseStore(Arc<Mutex<Inner>>);

impl ResponseStore {
    pub(crate) fn ensure_client_selection_allowed(&self) -> Result<(), String> {
        let inner = self.0.lock().unwrap();
        if inner.state.phase == ResponsePhase::Waiting
            || (inner.state.phase == ResponsePhase::Error && inner.state.serial.is_some())
        {
            return Err("请先停止抓包，再选择客户端或配置环境".into());
        }
        Ok(())
    }

    pub(crate) fn mark_stopped(&self, message: &str) {
        let mut inner = self.0.lock().unwrap();
        inner.generation = inner.generation.wrapping_add(1);
        inner.state.phase = ResponsePhase::Idle;
        inner.state.message = message.into();
        inner.interrupted_stop_target = None;
    }

    pub(crate) fn mark_stop_failed(&self, serial: &str, error: &str) {
        let mut inner = self.0.lock().unwrap();
        inner.generation = inner.generation.wrapping_add(1);
        inner.interrupted_stop_target = Some(serial.into());
        inner.state.serial = Some(serial.into());
        inner.state.phase = ResponsePhase::Error;
        inner.state.message = format!("停止失败：{error}");
    }

    pub(crate) fn interrupted_stop_target(&self) -> Option<String> {
        self.0.lock().unwrap().interrupted_stop_target.clone()
    }

    pub(crate) fn stop(&self, adb_path: &Path) -> Result<(), String> {
        self.stop_authenticated_using(|serial, key| {
            super::stop::stop_capture(adb_path, serial, key)
        })
    }

    #[cfg(test)]
    fn stop_using(&self, stop: impl FnOnce(&str) -> Result<(), String>) -> Result<(), String> {
        self.stop_authenticated_using(|serial, _| stop(serial))
    }

    fn stop_authenticated_using(
        &self,
        stop: impl FnOnce(&str, Option<&str>) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut inner = self.0.lock().unwrap();
        if let Some(serial) = &inner.state.serial {
            stop(serial, inner.api_key.as_deref())?;
        }
        inner.generation = inner.generation.wrapping_add(1);
        inner.state.phase = ResponsePhase::Idle;
        inner.state.message = "抓包已停止".into();
        inner.interrupted_stop_target = None;
        Ok(())
    }

    pub(crate) fn set_warnings(&self, warnings: Vec<String>) {
        self.0.lock().unwrap().state.warnings = warnings;
    }

    pub(crate) fn reset(&self) {
        let mut inner = self.0.lock().unwrap();
        inner.generation = inner.generation.wrapping_add(1);
        inner.baseline = Baseline::default();
        inner.game_launch_failed = false;
        inner.failures = 0;
        inner.next_poll = None;
        inner.api_key = None;
        inner.state = ResponseState::default();
        inner.interrupted_stop_target = None;
    }

    #[cfg(test)]
    pub(crate) fn begin(&self, serial: String, baseline: HashSet<String>) {
        self.begin_authenticated(Server::Cn, serial, baseline.into(), None, false);
    }

    pub(super) fn begin_authenticated(
        &self,
        server: Server,
        serial: String,
        baseline: Baseline,
        key: Option<String>,
        game_launch_failed: bool,
    ) {
        let mut inner = self.0.lock().unwrap();
        inner.generation = inner.generation.wrapping_add(1);
        inner.server = server;
        inner.baseline = baseline;
        inner.failures = 0;
        inner.next_poll = None;
        inner.api_key = key;
        inner.game_launch_failed = game_launch_failed;
        inner.interrupted_stop_target = None;
        inner.state = ResponseState {
            phase: ResponsePhase::Waiting,
            serial: Some(serial),
            message: waiting_message(game_launch_failed).into(),
            response: None,
            warnings: inner.baseline.warnings.clone(),
            retry_after_ms: 1000,
        };
    }

    pub(crate) fn poll(&self, adb_path: &Path, debug: &mut dyn FnMut(&str)) -> ResponseState {
        let debug = std::cell::RefCell::new(debug);
        let before = {
            let inner = self.0.lock().unwrap();
            (
                inner.state.phase.clone(),
                inner.state.message.clone(),
                inner.state.warnings.len(),
            )
        };
        let state = self.poll_authenticated_using(
            |server, serial, baseline, key| {
                let client = client::Client::with_api_key(key)?;
                client::on_device(adb_path, serial, |port, run| {
                    scan_session(
                        server,
                        &client,
                        port,
                        serial,
                        baseline,
                        run,
                        &mut |message| debug.borrow_mut()(message),
                    )
                })
            },
            |serial, key| {
                debug.borrow_mut()("已保存完整响应到会话内存，开始自动停止 Reqable");
                let began = Instant::now();
                let result = super::stop::stop_capture(adb_path, serial, key);
                debug.borrow_mut()(&format!(
                    "自动停止 Reqable 完成，成功={}，耗时 {} ms",
                    result.is_ok(),
                    began.elapsed().as_millis()
                ));
                result
            },
        );
        if before
            != (
                state.phase.clone(),
                state.message.clone(),
                state.warnings.len(),
            )
        {
            debug.borrow_mut()(&format!(
                "抓包状态更新：{:?}，提醒数={}，重试间隔={} ms",
                state.phase,
                state.warnings.len(),
                state.retry_after_ms
            ));
        }
        state
    }

    #[cfg(test)]
    fn poll_using(
        &self,
        scan: impl FnOnce(&str, &HashSet<String>) -> Result<Option<LoginResponse>, ScanError>,
        stop: impl FnOnce(&str) -> Result<(), String>,
    ) -> ResponseState {
        self.poll_authenticated_using(
            |_, serial, baseline, _| {
                scan(serial, &baseline.uids).map(|response| ScanOutcome {
                    response,
                    baseline: baseline.clone(),
                })
            },
            |serial, _| stop(serial),
        )
    }

    fn poll_authenticated_using(
        &self,
        scan: impl FnOnce(Server, &str, &Baseline, Option<&str>) -> Result<ScanOutcome, ScanError>,
        stop: impl FnOnce(&str, Option<&str>) -> Result<(), String>,
    ) -> ResponseState {
        let (generation, server, serial, baseline, key) = {
            let inner = self.0.lock().unwrap();
            if inner.state.phase != ResponsePhase::Waiting
                || inner.next_poll.is_some_and(|time| Instant::now() < time)
            {
                return inner.state.clone();
            }
            (
                inner.generation,
                inner.server,
                inner.state.serial.clone().unwrap(),
                inner.baseline.clone(),
                inner.api_key.clone(),
            )
        };
        let result = scan(server, &serial, &baseline, key.as_deref());
        let mut inner = self.0.lock().unwrap();
        // A configuration change or a new start supersedes any in-flight poll.
        if inner.generation == generation && inner.state.phase == ResponsePhase::Waiting {
            match result {
                Ok(outcome) => {
                    inner.baseline = outcome.baseline;
                    inner.state.warnings = inner.baseline.warnings.clone();
                    inner.failures = 0;
                    inner.next_poll = None;
                    inner.state.retry_after_ms = 1000;
                    let Some(response) = outcome.response else {
                        inner.state.message = waiting_message(inner.game_launch_failed).into();
                        return inner.state.clone();
                    };
                    inner.state.response = Some(response);
                    // Keep the complete body before stopping: Reqable may release
                    // its temporary files. Serialize stop with reset/begin so a
                    // stale poll cannot switch off a newer capture session.
                    match stop(&serial, key.as_deref()) {
                        Ok(()) => {
                            inner.state.phase = ResponsePhase::Found;
                            inner.state.message = format!(
                                "已获取{} 响应，抓包已停止",
                                target::response_label(server)
                            );
                        }
                        Err(error) => {
                            inner.state.phase = ResponsePhase::Error;
                            inner.state.message =
                                format!("已获取{} 响应；{error}", target::response_label(server));
                        }
                    }
                }
                Err(error) if error.is_fatal() => {
                    inner.state.phase = ResponsePhase::Error;
                    inner.state.message = format!("{error}；请停止后重新开始抓包");
                }
                Err(error) => {
                    inner.failures = inner.failures.saturating_add(1);
                    let delay = Duration::from_secs(1 << inner.failures.min(3));
                    inner.next_poll = Some(Instant::now() + delay);
                    inner.state.retry_after_ms = delay.as_millis() as u64;
                    inner.state.message = format!("{error}；连接恢复后将自动继续等待");
                }
            }
        }
        inner.state.clone()
    }
}

fn waiting_message(game_launch_failed: bool) -> &'static str {
    if game_launch_failed {
        "等待数据中，自动启动游戏失败，请重新进入游戏……"
    } else {
        "等待数据中，请重新进入游戏……"
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

fn target_url(server: Server, raw: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(raw).ok()?;
    let host = url.host_str()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return None;
    }
    match server {
        Server::Jp => {
            if host != "game.fate-go.jp" || url.path() != "/login/top" {
                return None;
            }
            // JP routes by path. Its query may contain private account data.
            url.set_query(None);
        }
        Server::Cn => {
            if ![
                "-ios-fate.bilibiligame.net",
                "-bili-fate.bilibiligame.net",
                "-uo-fate.bilibiligame.net",
            ]
            .iter()
            .any(|suffix| host.ends_with(suffix))
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
            url.set_query(Some("_key=toplogin"));
        }
    }
    url.set_fragment(None);
    Some(url.to_string())
}

fn ids(server: Server, client: &client::Client, port: u16) -> Result<Vec<u64>, ScanError> {
    let bytes = client.request_typed(
        port,
        "filter",
        Some(
            &serde_json::json!({"filters":[{"type":"keyword","pattern":target::selector(server)}]}),
        ),
        MAX_ID_BYTES,
    )?;
    let ids: Vec<u64> = serde_json::from_slice(&bytes)
        .map_err(|_| ScanError::Fatal("Reqable 记录列表无效".into()))?;
    Ok(ids)
}

fn record(client: &client::Client, port: u16, id: u64) -> Result<Record, ScanError> {
    let bytes = client
        .request_typed(port, "get", Some(&serde_json::json!({"id":id})), MAX_RECORD)
        .map_err(|error| match error {
            client::Error::Http(_) | client::Error::ResponseTooLarge => {
                ScanError::InvalidRecord(error.to_string())
            }
            _ => error.into(),
        })?;
    let record: Record = serde_json::from_slice(&bytes)
        .map_err(|_| ScanError::InvalidRecord("Reqable 记录格式无效".into()))?;
    if record.id != id || record.uid.is_empty() {
        return Err(ScanError::InvalidRecord("Reqable 记录标识无效".into()));
    }
    Ok(record)
}

#[cfg(test)]
pub(super) fn baseline(
    server: Server,
    client: &client::Client,
    port: u16,
) -> Result<Baseline, String> {
    baseline_controlled(
        server,
        client,
        port,
        &mut || Ok(()),
        &mut |_| {},
        &mut |_| {},
    )
}

pub(super) fn baseline_controlled(
    server: Server,
    client: &client::Client,
    port: u16,
    check: &mut dyn FnMut() -> Result<(), String>,
    progress: &mut dyn FnMut(&str),
    debug: &mut dyn FnMut(&str),
) -> Result<Baseline, String> {
    check()?;
    debug("开始查询已有登录记录列表");
    let began = Instant::now();
    let ids = ids(server, client, port).map_err(|error| error.to_string())?;
    debug(&format!(
        "已有记录列表查询完成，共 {} 条，耗时 {} ms",
        ids.len(),
        began.elapsed().as_millis()
    ));
    let mut baseline = Baseline::default();
    for (index, id) in ids.iter().enumerate() {
        check()?;
        if index % 16 == 0 {
            progress(&format!("正在识别已有记录：{}/{}", index, ids.len()));
        }
        debug(&format!(
            "开始读取旧记录 {}/{}，ID={id}",
            index + 1,
            ids.len()
        ));
        let began = Instant::now();
        match record(client, port, *id) {
            Ok(record) => {
                let matched = target_url(server, &record.url).is_some();
                debug(&format!(
                    "旧记录读取完成，ID={id}，匹配登录接口={matched}，耗时 {} ms",
                    began.elapsed().as_millis()
                ));
                if matched {
                    baseline.uids.insert(record.uid);
                }
            }
            Err(error @ (ScanError::Fatal(_) | ScanError::Retryable(_))) => {
                debug(&format!(
                    "旧记录读取中断，ID={id}，耗时 {} ms",
                    began.elapsed().as_millis()
                ));
                return Err(error.to_string());
            }
            Err(ScanError::InvalidRecord(_)) => {
                debug(&format!(
                    "旧记录读取失败，本次忽略 ID={id}，耗时 {} ms",
                    began.elapsed().as_millis()
                ));
                baseline.ignored_ids.insert(*id);
            }
        }
        check()?;
    }
    if !baseline.ignored_ids.is_empty() {
        baseline.warn(format!(
            "{} 条旧记录无法识别，本次将忽略这些记录 ID，避免读取旧响应",
            baseline.ignored_ids.len()
        ));
    }
    debug(&format!(
        "已有记录识别完成：{}/{}，基线 UID 数={}，忽略 ID 数={}",
        ids.len(),
        ids.len(),
        baseline.uids.len(),
        baseline.ignored_ids.len()
    ));
    Ok(baseline)
}

// Keep the original test helper focused on response selection.
#[cfg(test)]
fn scan(
    server: Server,
    client: &client::Client,
    port: u16,
    serial: &str,
    baseline: &Baseline,
    run: &mut AdbCommand<'_>,
) -> Result<Option<LoginResponse>, ScanError> {
    scan_session(server, client, port, serial, baseline, run, &mut |_| {})
        .map(|outcome| outcome.response)
}

fn scan_session(
    server: Server,
    client: &client::Client,
    port: u16,
    serial: &str,
    baseline: &Baseline,
    run: &mut AdbCommand<'_>,
    debug: &mut dyn FnMut(&str),
) -> Result<ScanOutcome, ScanError> {
    let mut baseline = baseline.clone();
    // Latest completed response wins; incomplete/transient records remain retryable.
    for id in ids(server, client, port)?.into_iter().rev() {
        if baseline.ignored_ids.contains(&id) {
            continue;
        }
        let record = match record(client, port, id) {
            Ok(record) => record,
            Err(error @ ScanError::InvalidRecord(_)) => {
                baseline.warn(format!("记录 {id} 暂时无法读取：{error}；将继续等待"));
                continue;
            }
            Err(error) => return Err(error),
        };
        if baseline.contains(&record.uid) {
            continue;
        }
        let Some(url) = target_url(server, &record.url) else {
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
            debug(&format!("登录请求返回失败状态，ID={id}，HTTP={code}"));
            baseline.warn(format!(
                "{} 请求失败（HTTP {code}），正在等待下一次登录",
                target::response_label(server)
            ));
            baseline.uids.insert(record.uid);
            continue;
        }
        debug(&format!("开始读取登录响应，ID={id}，HTTP={code}"));
        let began = Instant::now();
        let result =
            body_bytes(body, &mut |path| read_cache(serial, path, run)).and_then(|bytes| {
                let byte_length = bytes.len();
                raw_body(bytes).map(|(format, body)| (byte_length, format, body))
            });
        let (byte_length, raw_format, raw_body) = match result {
            Ok(result) => result,
            Err(error) => {
                debug(&format!(
                    "登录响应读取失败，ID={id}，耗时 {} ms",
                    began.elapsed().as_millis()
                ));
                baseline.warn(format!("记录 {id} 暂不可用：{error}；将继续等待"));
                // Cache files and ADB can recover; malformed/unsupported bodies cannot.
                if matches!(error, ScanError::InvalidRecord(_)) {
                    baseline.uids.insert(record.uid);
                }
                continue;
            }
        };
        if byte_length == 0 {
            continue;
        }
        debug(&format!(
            "登录响应读取完成，ID={id}，字节数={byte_length}，耗时 {} ms",
            began.elapsed().as_millis()
        ));
        let items = super::inventory::preview(&raw_body);
        let captured_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| ScanError::Retryable("系统时间无效".into()))?
            .as_millis() as u64;
        return Ok(ScanOutcome {
            response: Some(LoginResponse {
                capture_id: uuid::Uuid::new_v4().to_string(),
                captured_at,
                record_id: id,
                url,
                status_code: code,
                byte_length,
                format: raw_format,
                body: raw_body,
                items,
            }),
            baseline,
        });
    }
    if !super::capture::decode_status(&client.request_typed(port, "status", None, 4096)?)
        .map_err(ScanError::Fatal)?
    {
        return Err(ScanError::Fatal("Reqable 抓包已停止".into()));
    }
    Ok(ScanOutcome {
        response: None,
        baseline,
    })
}

fn body_bytes(
    body: Body,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, ScanError>,
) -> Result<Vec<u8>, ScanError> {
    let bytes = match body.encoding.as_str() {
        "utf8" => body.text.into_bytes(),
        "base64" => STANDARD
            .decode(body.text)
            .map_err(|_| ScanError::InvalidRecord("响应的 Base64 编码无效".into()))?,
        "file" => read(&cache_path(&body.text).map_err(ScanError::InvalidRecord)?)?,
        _ => {
            return Err(ScanError::InvalidRecord(
                "Reqable 响应使用了不支持的编码".into(),
            ))
        }
    };
    if bytes.len() > MAX_BODY {
        return Err(ScanError::InvalidRecord(
            "响应超过 16 MiB，未截断展示，请在 Reqable 中查看".into(),
        ));
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

fn read_cache(serial: &str, path: &str, run: &mut AdbCommand<'_>) -> Result<Vec<u8>, ScanError> {
    let mut shell = |command: &str| run(&["-s", serial, "shell", command]);
    let root = find_root(&mut shell)?;
    let path = shell_quote(path);
    let resolved = shell(&root_shell(root, &format!("readlink -f {path}")))
        .map_err(|_| ScanError::Retryable("无法解析响应缓存文件".into()))?;
    let resolved = String::from_utf8(resolved)
        .map_err(|_| ScanError::InvalidRecord("响应缓存路径无效".into()))?;
    let resolved = cache_path(resolved.trim()).map_err(ScanError::InvalidRecord)?;
    let resolved = shell_quote(&resolved);
    let size = shell(&root_shell(
        root,
        &format!("test -f {resolved} && stat -c %s {resolved}"),
    ))
    .map_err(|_| ScanError::Retryable("响应缓存文件已失效".into()))?;
    let size: usize = String::from_utf8_lossy(&size)
        .trim()
        .parse()
        .map_err(|_| ScanError::Retryable("无法读取响应缓存大小".into()))?;
    if size > MAX_BODY {
        return Err(ScanError::InvalidRecord(
            "响应超过 16 MiB，请在 Reqable 中查看".into(),
        ));
    }
    let command = root_shell(root, &format!("head -c {} {resolved}", MAX_BODY + 1));
    let bytes = run(&["-s", serial, "exec-out", &command])
        .map_err(|_| ScanError::Retryable("无法读取响应缓存文件".into()))?;
    if bytes.len() != size {
        return Err(ScanError::Retryable(
            "响应缓存读取不完整，请重新登录后重试".into(),
        ));
    }
    Ok(bytes)
}

fn raw_body(mut bytes: Vec<u8>) -> Result<(String, String), ScanError> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut expanded = Vec::new();
        flate2::read::GzDecoder::new(bytes.as_slice())
            .take((MAX_BODY + 1) as u64)
            .read_to_end(&mut expanded)
            .map_err(|_| ScanError::InvalidRecord("响应 gzip 解压失败".into()))?;
        if expanded.len() > MAX_BODY {
            return Err(ScanError::InvalidRecord("解压后的响应超过 16 MiB".into()));
        }
        bytes = expanded;
    }
    match String::from_utf8(bytes) {
        Ok(text) => Ok(("原始响应".into(), text)),
        Err(error) => Ok((
            "Base64（二进制响应）".into(),
            STANDARD.encode(error.into_bytes()),
        )),
    }
}

// Decoding is exclusively for the inventory projection, never for display/copy.
pub(super) fn decoded_json(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str(text).ok().or_else(|| {
        let value = percent_decode(text)?;
        let bytes = URL_SAFE
            .decode(&value)
            .or_else(|_| URL_SAFE_NO_PAD.decode(&value))
            .or_else(|_| STANDARD.decode(&value))
            .ok()?;
        if bytes.len() > MAX_BODY {
            return None;
        }
        serde_json::from_slice(&bytes).ok()
    })
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
