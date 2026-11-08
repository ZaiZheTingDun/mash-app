//! Device-scoped API keys. Never returned over IPC or included in diagnostics.
use super::{capture::decode_status, client, clipboard::ClipboardReader, run_adb};
use crate::device_operation::OperationCancellation;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

mod manual;
pub(crate) use manual::{AuthorizationPrompt, AuthorizationStore};

pub(crate) struct ApiKeyStore(PathBuf);

impl ApiKeyStore {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self(path)
    }

    fn read(&self) -> Result<BTreeMap<String, String>, String> {
        match std::fs::read(&self.0) {
            Ok(bytes) if bytes.len() <= 1024 * 1024 => serde_json::from_slice(&bytes)
                .map_err(|_| "Reqable API key 配置无法读取，请检查本地配置文件".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            _ => Err("Reqable API key 配置无法读取".into()),
        }
    }

    fn get(&self, serial: &str) -> Result<Option<String>, String> {
        Ok(self.read()?.remove(serial))
    }

    fn save(&self, serial: &str, key: &str) -> Result<(), String> {
        let mut keys = self.read()?;
        keys.insert(serial.into(), key.into());
        // Atomic storage uses a private temporary file; existing data survives a failed save.
        crate::storage::write_json_atomic(&self.0, &keys, "Reqable API key 配置")
            .map_err(|_| "保存 Reqable API key 失败，请检查应用数据目录权限".into())
    }
}

pub(super) fn needs_api_key(version: &str) -> bool {
    super::version::parse(version).is_some_and(|parts| parts >= [3, 3, 9])
}

fn candidate(text: &str) -> Option<&str> {
    let key = text.trim();
    // Bearer-token syntax; accept UUIDs and future token formats without guessing
    // a Reqable-specific prefix. The API itself is the authority on validity.
    (key.len() >= 8
        && key.len() <= 4096
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~+/=".contains(&b)))
    .then_some(key)
}

pub(super) fn authorize(
    store: &ApiKeyStore,
    adb_path: &Path,
    jar_path: &Path,
    serial: &str,
    port: u16,
    cancellation: &OperationCancellation,
    authorization: &AuthorizationStore,
    progress: &mut dyn FnMut(&str),
) -> Result<String, String> {
    cancellation.check()?;
    let saved = store.get(serial)?;
    if let Some(key) = &saved {
        match validate_saved_key(
            &mut |timeout| {
                let client = client::Client::with_api_key(Some(key))?;
                let bytes = client.request_with_timeout(port, "status", None, 4096, timeout)?;
                decode_status(&bytes).map(|_| ())
            },
            &mut || cancellation.wait(Duration::from_millis(250)),
            cancellation,
        ) {
            Ok(()) => {
                cancellation.check()?;
                return Ok(key.clone());
            }
            Err(error) if error == client::AUTHENTICATION_FAILED => {}
            Err(error) => return Err(error),
        }
    }
    cancellation.check()?;
    let session = authorization.begin(serial, cancellation);
    let mut clipboard = match ClipboardReader::start(adb_path, jar_path, serial, cancellation) {
        Ok(reader) => Some(reader),
        Err(_) => {
            cancellation.check()?;
            session.feedback("自动读取剪贴板不可用，请手动输入 Reqable API key");
            None
        }
    };
    progress("API 服务开启后，请创建 API key 并点击复制，或手动输入");
    let started = Instant::now();
    let mut previous_auto = None;
    loop {
        cancellation.check()?;
        if started.elapsed() >= Duration::from_secs(120) {
            return Err("等待 API key 超时，请重新开始并复制或手动输入 API key".into());
        }
        let mut text = session.take_key();
        if text.is_none() {
            if let Some(reader) = clipboard.as_mut() {
                match reader.next_text() {
                    Ok(Some(value)) if candidate(&value).is_some() => {
                        match check_reqable_focus(&mut |command| {
                            run_adb(adb_path, &["-s", serial, "shell", command])
                        }) {
                            Ok(true) if previous_auto.as_ref() != Some(&value) => {
                                previous_auto = Some(value.clone());
                                text = Some(value);
                            }
                            Ok(true) => {}
                            Ok(false) => {
                                session.feedback("请切回 Reqable 再次复制，或手动输入 API key")
                            }
                            Err(_) => session.feedback("无法确认前台窗口，请手动输入 API key"),
                        }
                    }
                    Ok(_) => {}
                    Err(_) => {
                        session.feedback("剪贴板连接已断开，请手动输入 Reqable API key");
                        clipboard = None;
                    }
                }
            } else {
                cancellation.wait(Duration::from_millis(100));
            }
        }
        let Some(text) = text else {
            continue;
        };
        let Some(key) = candidate(&text) else {
            continue;
        };
        session.verifying();
        let result = validate_saved_key(
            &mut |timeout| validate_with_timeout(port, key, timeout),
            &mut || cancellation.wait(Duration::from_millis(250)),
            cancellation,
        );
        match result {
            Ok(()) => {
                cancellation.check()?;
                drop(clipboard);
                cancellation.check()?;
                store.save(serial, key)?;
                progress("API key 已验证并保存，正在启动抓包…");
                return Ok(key.into());
            }
            Err(error) if error == client::AUTHENTICATION_FAILED => {
                session.feedback("API key 认证失败，请检查后重新复制或输入");
            }
            Err(error) => {
                cancellation.check()?;
                session.feedback(&format!("{error}；请恢复连接后重新提交 API key"));
            }
        }
    }
}

pub(super) fn probe_access(port: u16, cancellation: &OperationCancellation) -> Result<(), String> {
    validate_saved_key(
        &mut |timeout| {
            let bytes =
                client::Client::new()?.request_with_timeout(port, "status", None, 4096, timeout)?;
            decode_status(&bytes).map(|_| ())
        },
        &mut || cancellation.wait(Duration::from_millis(250)),
        cancellation,
    )
}

fn check_reqable_focus(
    shell: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<bool, String> {
    // "windows" lists individual windows but omits mCurrentFocus on some Android
    // emulators. It is a display property; query displays first, then the full
    // dump for ROMs which expose it only there. Missing metadata is not background.
    for command in ["dumpsys window displays", "dumpsys window"] {
        if let Ok(window) = shell(command) {
            if let Some(focused) = reqable_has_focus(&window) {
                return Ok(focused);
            }
        }
    }
    Err("无法读取模拟器的前台窗口信息，请保持 Reqable 打开后重试".into())
}

fn reqable_has_focus(window: &[u8]) -> Option<bool> {
    let dump = String::from_utf8_lossy(window);
    let mut focus = dump
        .lines()
        .filter_map(|line| line.trim().strip_prefix("mCurrentFocus="))
        .peekable();
    focus.peek()?;
    Some(focus.any(|line| {
        line.split_whitespace()
            .any(|part| part.starts_with("com.reqable.android/"))
    }))
}

fn validate_with_timeout(port: u16, key: &str, timeout: Duration) -> Result<(), String> {
    let client = client::Client::with_api_key(Some(key))?;
    let bytes = client.request_with_timeout(port, "status", None, 4096, timeout)?;
    decode_status(&bytes).map(|_| ())
}

fn validate_saved_key(
    validate: &mut dyn FnMut(Duration) -> Result<(), String>,
    pause: &mut dyn FnMut(),
    cancellation: &OperationCancellation,
) -> Result<(), String> {
    // Preserve tolerance for a slow status response without multiplying the
    // original 30-second request timeout by the number of readiness retries.
    let started = Instant::now();
    for attempt in 0..12 {
        cancellation.check()?;
        let remaining = Duration::from_secs(30).saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err("Reqable 控制接口尚未就绪，请稍后重试".into());
        }
        let result = validate(remaining);
        cancellation.check()?;
        match result {
            Err(error)
                if attempt < 11
                    && client::is_transient_error(&error)
                    && started.elapsed() < Duration::from_secs(30) =>
            {
                pause()
            }
            result => return result,
        }
    }
    unreachable!("the final validation attempt always returns")
}

#[cfg(test)]
mod tests;
