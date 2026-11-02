//! Device-scoped API keys. Never returned over IPC or included in diagnostics.
use super::{capture::decode_status, client, clipboard::ClipboardReader, run_adb};
use crate::device_operation::OperationCancellation;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

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
    let parts: Vec<u32> = version
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect();
    parts.as_slice() >= [3, 3, 9].as_slice()
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
    let mut clipboard = ClipboardReader::start(adb_path, jar_path, serial, cancellation)?;
    let progress = RefCell::new(progress);
    progress.borrow_mut()("API 服务开启后，请创建 API key 并点击复制，mash 将自动验证并保存");
    let started = Instant::now();
    let key = wait_for_key(
        saved.as_deref(),
        &mut || {
            cancellation.check()?;
            let text = clipboard.next_text()?;
            if let Some(text) = &text {
                if candidate(text).is_none() {
                    progress.borrow_mut()(
                        "已收到剪贴板内容，但不是 API key 格式，请在 Reqable 中复制生成的 API key",
                    );
                    return Ok(None);
                }
                // Clipboard notifications do not identify their source. Only
                // accept candidates while Reqable is the focused Android window.
                if !check_reqable_focus(&mut |command| {
                    run_adb(adb_path, &["-s", serial, "shell", command])
                })? {
                    progress.borrow_mut()("已收到复制内容，但 Reqable 不在前台，请切回 Reqable 并再次点击复制 API key");
                    return Ok(None);
                }
            }
            Ok(text)
        },
        &mut |key| {
            progress.borrow_mut()("已读取 API key，正在验证…");
            let result = validate(port, key);
            if result
                .as_ref()
                .is_err_and(|error| error == client::AUTHENTICATION_FAILED)
            {
                progress.borrow_mut()(
                    "复制的 API key 认证失败，请在 Reqable 中确认 key 有效并重新创建、复制",
                );
            }
            result
        },
        &mut || {
            cancellation.check()?;
            if started.elapsed() >= Duration::from_secs(120) {
                return Err(
                    "等待 API key 超时，请重新开始并在 Reqable 中创建 key 后点击复制".into(),
                );
            }
            Ok(())
        },
    )?;
    // Close the temporary clipboard listener before persisting/starting capture.
    drop(clipboard);
    cancellation.check()?;
    store.save(serial, &key)?;
    progress.borrow_mut()("API key 已验证并保存，正在启动抓包…");
    Ok(key)
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

fn validate(port: u16, key: &str) -> Result<(), String> {
    let client = client::Client::with_api_key(Some(key))?;
    let bytes = client.request(port, "status", None, 4096)?;
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

fn wait_for_key(
    rejected: Option<&str>,
    read: &mut dyn FnMut() -> Result<Option<String>, String>,
    validate: &mut dyn FnMut(&str) -> Result<(), String>,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<String, String> {
    let mut previous = rejected.map(str::to_owned);
    loop {
        check()?;
        let Some(text) = read()? else {
            continue;
        };
        let Some(key) = candidate(&text) else {
            continue;
        };
        if previous.as_deref() == Some(key) {
            continue;
        }
        previous = Some(key.into());
        check()?;
        match validate(key) {
            Ok(()) => {
                check()?;
                return Ok(key.into());
            }
            Err(error) if error == client::AUTHENTICATION_FAILED => {}
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests;
