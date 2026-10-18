//! Sidecar process lifecycle, request transport, streaming, and cleanup.

use super::error::{template_response_error, CvError, CvResult};
use super::protocol::{request, SidecarCommand};
use crate::operation_log::{self, DebugSource};
use base64::Engine;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

pub struct SidecarClient {
    child: Option<tauri_plugin_shell::process::CommandChild>,
    /// Receives stdout lines forwarded from the async task.
    line_rx: mpsc::Receiver<String>,
    /// Monotonic request id. Every outgoing request carries `{"id": n}` and
    /// the sidecar echoes the same id back; responses whose id does not match
    /// the in-flight request are dropped (they are stale leftovers from a
    /// previously timed-out call). 0 is reserved for "no id sent yet".
    next_id: u64,
    /// Last (width, height) reported by `start_stream`. None until a stream
    /// has been started successfully.
    stream_size: Option<(u32, u32)>,
    stream_device: Option<(String, u32)>,
    healthy: bool,
    pub(super) detection_screens: Option<Vec<String>>,
    pub(super) diagnostic_supported: bool,
    pub(super) diagnostic_enabled: bool,
    pub(crate) recognition_context: serde_json::Value,
    pub(crate) recognition_device: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct TemplateLoadSpec {
    pub dir: PathBuf,
    pub key_prefix: Option<String>,
}

impl SidecarClient {
    /// Spawn the mash-cv sidecar and optionally load templates + config.
    ///
    /// ``server`` is forwarded to the sidecar via a `set_server` REPL call
    /// after templates / config are loaded so the sidecar can pin the
    /// matching OCR model on first use. Defaulting on the sidecar side
    /// means an older sidecar binary that doesn't recognize `set_server`
    /// degrades to JP-only behaviour rather than failing the spawn.
    pub fn spawn(
        app: &tauri::AppHandle,
        template_dirs: &[TemplateLoadSpec],
        config_paths: &[PathBuf],
        server: crate::Server,
    ) -> Result<Self, String> {
        let exe = crate::resolve_sidecar_exe(app)
            .ok_or_else(|| "无法解析 mash-cv runtime，请先安装 CV 运行时".to_string())?;
        if !exe.exists() {
            return Err(format!(
                "未找到 mash-cv runtime 可执行文件 ({}). 请先下载并安装当前版本需要的 CV 运行时。",
                exe.display()
            ));
        }
        let code_dir = crate::resolve_sidecar_code_dir(app)
            .ok_or_else(|| "未找到应用内置的 CV 代码，请重新安装应用".to_string())?;
        if !code_dir.join("mash_cv").is_dir() {
            return Err(format!(
                "未找到 mash-cv code 包 ({}). 请重新安装应用。",
                code_dir.display()
            ));
        }
        let models_dir = crate::resolve_sidecar_models_dir(app)
            .ok_or_else(|| "无法解析 mash-cv OCR 模型目录，请先安装 CV runtime 包".to_string())?;
        if !models_dir.is_dir() {
            return Err(format!(
                "未找到 mash-cv OCR 模型目录 ({}). 请先下载并安装当前版本需要的 CV runtime 包。",
                models_dir.display()
            ));
        }
        let servants_json = crate::resolve_servants_json_path(app)
            .ok_or_else(|| "无法解析从者元数据文件".to_string())?;
        if !servants_json.is_file() {
            return Err(format!(
                "未找到从者元数据文件 ({})",
                servants_json.display()
            ));
        }

        let cmd = app
            .shell()
            .command(&exe)
            .env("MASH_CV_CODE_DIR", code_dir.to_string_lossy().to_string())
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env(
                "MASH_CV_MODELS_DIR",
                models_dir.to_string_lossy().to_string(),
            )
            .env(
                "MASH_CV_SERVANTS_JSON_PATH",
                servants_json.to_string_lossy().to_string(),
            )
            .env("PYTHONPATH", code_dir.to_string_lossy().to_string())
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUTF8", "1");
        let (mut rx, child) = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn sidecar: {e}"))?;

        // Bridge the async tokio receiver into a sync std::mpsc channel so the
        // blocking runner thread can call recv() without an async runtime.
        let (line_tx, line_rx) = mpsc::channel::<String>();
        let log_app = app.clone();

        tauri::async_runtime::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    CommandEvent::Stdout(bytes) => {
                        let line = String::from_utf8_lossy(&bytes).trim().to_string();
                        if !line.is_empty() {
                            if line_tx.send(line).is_err() {
                                break;
                            }
                        }
                    }
                    CommandEvent::Stderr(bytes) => {
                        let msg = String::from_utf8_lossy(&bytes);
                        for line in msg.lines().map(str::trim).filter(|line| !line.is_empty()) {
                            let message = format!("[mash-cv stderr] {line}");
                            eprintln!("{message}");
                            operation_log::emit_debug(&log_app, DebugSource::Sidecar, message);
                        }
                    }
                    CommandEvent::Error(e) => {
                        let message = format!("[mash-cv error] {e}");
                        eprintln!("{message}");
                        operation_log::emit_debug(&log_app, DebugSource::Sidecar, message);
                    }
                    CommandEvent::Terminated(_) => break,
                    _ => {}
                }
            }
        });

        let mut client = Self {
            child: Some(child),
            line_rx,
            next_id: 1,
            stream_size: None,
            stream_device: None,
            healthy: true,
            detection_screens: None,
            diagnostic_supported: false,
            diagnostic_enabled: false,
            recognition_context: serde_json::json!({}),
            recognition_device: serde_json::json!({}),
        };

        // Wait for the sidecar to finish its cold-boot before sending any real
        // commands. The PyInstaller onefile bundle (OpenCV + PyAV) can take
        // 20-40s on macOS the first time it's extracted. A dedicated `ping`
        // response tells us Python's REPL is running and keeps subsequent
        // request/response pairs in lockstep.
        let ping = request(SidecarCommand::Ping, serde_json::json!({}))?;
        match client.send_recv_with_timeout(&ping, Duration::from_secs(90)) {
            Ok(resp) => {
                client.diagnostic_supported =
                    resp["recognitionDiagnosticsVersion"].as_u64() == Some(1);
                eprintln!("[mash-cv] ping -> {resp}");
            }
            Err(e) => {
                let detail = format!("sidecar did not become ready: {e}");
                eprintln!("[mash-cv startup] {detail}");
                return Err(detail);
            }
        }

        for (idx, dir) in template_dirs.iter().enumerate() {
            if dir.dir.is_dir() {
                let req = request(
                    SidecarCommand::LoadTemplates,
                    serde_json::json!({
                        "dir": dir.dir.to_string_lossy(),
                        "append": idx > 0,
                        "keyPrefix": dir.key_prefix.as_deref().unwrap_or(""),
                    }),
                )?;
                let context = format!("load_templates({})", dir.dir.display());
                let resp = client
                    .send_recv(&req)
                    .map_err(|e| format!("{context}: {e}"))?;
                Self::require_success(&resp, &context)?;
                eprintln!("[mash-cv] load_templates -> {resp}");
            } else {
                return Err(format!(
                    "templates directory missing: {}",
                    dir.dir.display()
                ));
            }
        }

        for (idx, path) in config_paths.iter().enumerate() {
            if path.exists() {
                let req = request(
                    SidecarCommand::LoadConfig,
                    serde_json::json!({
                        "path": path.to_string_lossy(),
                        "merge": idx > 0,
                    }),
                )?;
                let context = format!("load_config({})", path.display());
                let resp = client
                    .send_recv(&req)
                    .map_err(|e| format!("{context}: {e}"))?;
                Self::require_success(&resp, &context)?;
                eprintln!("[mash-cv] load_config -> {resp}");
            } else {
                return Err(format!("config path missing: {}", path.display()));
            }
        }

        // Tell the sidecar which server's OCR model to use. Sent after
        // templates + config so a fresh `_ocr_engine` rebuild sees the
        // right model on the first `find_supports` call. Failures are
        // logged but non-fatal: an older sidecar binary that doesn't
        // know `set_server` simply stays on its compile-time default.
        let req = request(
            SidecarCommand::SetServer,
            serde_json::json!({
                "server": server.to_string(),
            }),
        )?;
        match client.send_recv(&req) {
            Ok(resp) => eprintln!("[mash-cv] set_server -> {resp}"),
            Err(e) => eprintln!("[mash-cv] set_server failed (sidecar may be stale): {e}"),
        }

        client.recognition_context =
            crate::recognition_diagnostics::launch_context(&exe, &code_dir, config_paths, server);
        Ok(client)
    }

    pub(super) fn require_success(
        response: &serde_json::Value,
        context: &str,
    ) -> Result<(), String> {
        if let Some(error) = response.get("error").and_then(|value| value.as_str()) {
            return Err(format!("{context}: {error}"));
        }
        if response["ok"].as_bool() != Some(true) {
            return Err(format!("{context}: sidecar did not report success"));
        }
        Ok(())
    }

    pub(crate) fn is_healthy(&self) -> bool {
        self.healthy
    }

    /// Cache only a healthy client, after releasing per-run resources. A
    /// failed client is dropped here so no runner needs its own failure guard.
    pub(crate) fn return_to_cache(mut self, cache: Option<&Mutex<Option<Self>>>) {
        if !self.is_healthy() {
            return;
        }
        if let Err(error) = self.prepare_for_cache() {
            eprintln!("[mash-cv] prepare sidecar for cache failed: {error}");
            return;
        }
        if let Some(cache) = cache {
            let mut cached = cache.lock().unwrap();
            if cached.is_none() {
                *cached = Some(self);
            }
        }
    }

    pub fn use_battle_detection(&mut self) {
        self.detection_screens = Some(
            super::types::BATTLE_DETECTION_SCREENS
                .iter()
                .map(|name| name.to_string())
                .collect(),
        );
    }

    fn check_response(&mut self, response: &serde_json::Value) -> CvResult<()> {
        if let Some(error) = template_response_error(response) {
            self.healthy = false;
            return Err(error);
        }
        Ok(())
    }

    /// Send a JSON command and wait for the JSON response line.
    pub(super) fn send_recv(
        &mut self,
        request: &serde_json::Value,
    ) -> Result<serde_json::Value, CvError> {
        self.send_recv_with_timeout(request, Duration::from_secs(10))
    }

    /// Like `send_recv` but with a caller-specified timeout. Used by commands
    /// that can legitimately take longer than 10s (sidecar cold boot, scrcpy
    /// handshake).
    ///
    /// Tags the outgoing request with a monotonic ``id`` and discards any
    /// response whose ``id`` doesn't match -- those are stale leftovers from
    /// a previously timed-out call still in flight from the sidecar.
    pub(super) fn send_recv_with_timeout(
        &mut self,
        request: &serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, CvError> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);

        let mut tagged = request.clone();
        if let Some(obj) = tagged.as_object_mut() {
            obj.insert("id".into(), serde_json::Value::from(id));
        } else {
            return Err(format!("send_recv: request must be a JSON object, got {request}").into());
        }

        let mut line = tagged.to_string();
        line.push('\n');
        self.child
            .as_mut()
            .ok_or_else(|| "sidecar process already stopped".to_string())?
            .write(line.as_bytes())
            .map_err(|e| format!("failed to write to sidecar: {e}"))?;

        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| format!("sidecar response timeout (waiting for id={id})"))?;
            let response = self
                .line_rx
                .recv_timeout(remaining)
                .map_err(|e| format!("sidecar response timeout: {e}"))?;
            let parsed: serde_json::Value = serde_json::from_str(&response)
                .map_err(|e| format!("invalid JSON from sidecar: {e}: {response}"))?;
            match parsed.get("id").and_then(|v| v.as_u64()) {
                Some(rid) if rid == id => {
                    self.check_response(&parsed)?;
                    return Ok(parsed);
                }
                Some(rid) => {
                    eprintln!(
                        "[mash-cv] dropping stale response id={rid} (expected {id}): {response}"
                    );
                    continue;
                }
                None => {
                    // Strict mode: a sidecar built against this Rust client
                    // must echo the id. A response without one almost certainly
                    // means the sidecar binary is from an older build -- tell
                    // the operator instead of silently corrupting CV results.
                    return Err(format!(
                        "sidecar response missing 'id' field (rebuild mash-cv sidecar): {response}"
                    )
                    .into());
                }
            }
        }
    }

    /// Helper to inject `imagePath` into the request when the caller provided one.
    /// Passing `None` makes the sidecar read the latest frame from the scrcpy stream.
    pub(super) fn add_image_path(req: &mut serde_json::Value, image_path: Option<&Path>) {
        if let Some(p) = image_path {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "imagePath".into(),
                    serde_json::Value::String(p.to_string_lossy().into_owned()),
                );
            }
        }
    }

    /// Start the scrcpy server on the device and begin streaming.
    /// Returns the negotiated codec (width, height) -- already downscaled by
    /// the device if `max_size` was non-zero, so this is what every decoded
    /// frame will measure, not the raw display size.
    pub fn start_stream(
        &mut self,
        adb: &crate::adb::Adb,
        jar_path: &Path,
        max_size: u32,
        bit_rate: u32,
        max_fps: u32,
    ) -> Result<(u32, u32), CvError> {
        self.stream_size = None;
        self.stream_device = None;
        let mut req = request(
            SidecarCommand::StartStream,
            serde_json::json!({
                "adbPath": adb.path().to_string_lossy(),
                "jarPath": jar_path.to_string_lossy(),
                "maxSize": max_size,
                "bitRate": bit_rate,
                "maxFps": max_fps,
                "displayId": adb.display_id(),
            }),
        )?;
        if let Some(s) = adb.serial() {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("serial".into(), serde_json::Value::String(s.to_string()));
            }
        }
        let resp = self.send_recv_with_timeout(&req, Duration::from_secs(45))?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("start_stream failed: {err}").into());
        }
        if let Err(error) = validate_stream_display(&resp, adb.display_id()) {
            let _ = self.stop_stream();
            return Err(error.into());
        }
        let w = resp["width"].as_u64().unwrap_or(0) as u32;
        let h = resp["height"].as_u64().unwrap_or(0) as u32;
        if w == 0 || h == 0 {
            return Err("start_stream returned invalid dimensions".into());
        }
        self.stream_size = Some((w, h));
        self.stream_device = adb
            .serial()
            .map(|serial| (serial.to_string(), adb.display_id()));
        self.recognition_context["stream"] = serde_json::json!({"width": w, "height": h,
            "maxSize": max_size, "bitRate": bit_rate, "maxFps": max_fps, "displayId": adb.display_id()});
        Ok((w, h))
    }

    pub(crate) fn stream_matches_device(
        &self,
        device_serial: Option<&str>,
        display_id: u32,
    ) -> bool {
        self.stream_device
            .as_ref()
            .is_some_and(|(serial, display)| {
                Some(serial.as_str()) == device_serial && *display == display_id
            })
    }

    /// Select which battle-HUD digit reader the sidecar should use.
    pub fn set_digit_recognition_mode(&mut self, mode: &str) -> Result<(), CvError> {
        let req = request(
            SidecarCommand::SetDigitRecognitionMode,
            serde_json::json!({ "mode": mode }),
        )?;
        let resp = self.send_recv(&req)?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("set_digit_recognition_mode failed: {err}").into());
        }
        Ok(())
    }

    /// Select the complete-number CNN-CTC reader for battle HUD values.
    pub fn set_sequence_recognition_mode(&mut self, mode: &str) -> Result<(), CvError> {
        let req = request(
            SidecarCommand::SetSequenceRecognitionMode,
            serde_json::json!({ "mode": mode }),
        )?;
        let resp = self.send_recv(&req)?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("set_sequence_recognition_mode failed: {err}").into());
        }
        Ok(())
    }

    /// Stop the scrcpy server / decoder thread. Safe to call if not started.
    pub fn stop_stream(&mut self) -> Result<(), CvError> {
        let req = request(SidecarCommand::StopStream, serde_json::json!({}))?;
        let resp = self.send_recv(&req)?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("stop_stream failed: {err}").into());
        }
        self.stream_size = None;
        self.stream_device = None;
        Ok(())
    }

    /// Release the disposable RapidOCR/ONNX worker without stopping the
    /// lightweight mash-cv process. Safe to call before OCR was initialized.
    pub fn release_ocr(&mut self) -> Result<(), CvError> {
        let req = request(SidecarCommand::ReleaseOcr, serde_json::json!({}))?;
        let resp = self.send_recv(&req)?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("release_ocr failed: {err}").into());
        }
        Ok(())
    }

    /// Stop device streaming and release the memory-heavy OCR worker before
    /// this client is placed back into the shared sidecar cache.
    pub fn prepare_for_cache(&mut self) -> Result<(), CvError> {
        self.detection_screens = None;
        self.diagnostic_enabled = false;
        let stream_error = self.stop_stream().err();
        let ocr_error = self.release_ocr().err();
        match (stream_error, ocr_error) {
            (None, None) => Ok(()),
            (Some(stream), None) => Err(stream),
            (None, Some(ocr)) => Err(ocr),
            (Some(stream), Some(ocr)) => Err(format!("{stream}; {ocr}").into()),
        }
    }

    /// Return the (width, height) of the live stream, if one was started.
    pub fn stream_size(&self) -> Option<(u32, u32)> {
        self.stream_size
    }

    /// Return the latest decoded frame as a JPEG byte buffer. ``wait_seconds``
    /// is the maximum time the sidecar will block waiting for a fresh frame
    /// (0 = return immediately if none is cached).
    pub fn get_frame_jpeg(&mut self, wait_seconds: f64) -> Result<Vec<u8>, CvError> {
        let (b64, _, _) = self.get_frame_jpeg_base64(wait_seconds)?;
        Ok(base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("invalid base64 in jpegB64: {e}"))?)
    }

    /// Return the latest decoded frame as a lossless PNG byte buffer.
    pub fn get_frame_png(&mut self, wait_seconds: f64) -> Result<Vec<u8>, CvError> {
        let req = request(
            SidecarCommand::GetFrame,
            serde_json::json!({
                "format": "png",
                "waitSeconds": wait_seconds,
            }),
        )?;
        let resp = self.send_recv(&req)?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("get_frame failed: {err}").into());
        }
        let b64 = resp["pngB64"]
            .as_str()
            .ok_or_else(|| "get_frame missing pngB64".to_string())?;
        Ok(base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("invalid base64 in pngB64: {e}"))?)
    }

    /// Return the latest decoded frame as base64 JPEG plus frame dimensions.
    pub fn get_frame_jpeg_base64(
        &mut self,
        wait_seconds: f64,
    ) -> Result<(String, u32, u32), CvError> {
        let req = request(
            SidecarCommand::GetFrame,
            serde_json::json!({
                "waitSeconds": wait_seconds,
            }),
        )?;
        let resp = self.send_recv(&req)?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("get_frame failed: {err}").into());
        }
        let b64 = resp["jpegB64"]
            .as_str()
            .ok_or_else(|| "get_frame missing jpegB64".to_string())?;
        let width = resp["width"].as_u64().unwrap_or(0) as u32;
        let height = resp["height"].as_u64().unwrap_or(0) as u32;
        Ok((b64.to_string(), width, height))
    }

    /// Tell the sidecar to exit.
    pub fn shutdown(&mut self) {
        // Best-effort: ask the device-side scrcpy server to exit cleanly
        // before we kill the host process. Without this, abruptly killing the
        // sidecar mid-stream relies on scrcpy's `cleanup=true` to GC the
        // server -- which works most of the time but occasionally leaves a
        // zombie `app_process` on the device.
        if self.child.is_some() && self.stream_size.is_some() {
            if let Err(e) = self.stop_stream() {
                eprintln!("[mash-cv] stop_stream during shutdown failed: {e}");
            }
        }
        if let Some(mut child) = self.child.take() {
            let req = request(SidecarCommand::Quit, serde_json::json!({}))
                .expect("empty sidecar quit request must serialize");
            let mut line = req.to_string();
            line.push('\n');
            let _ = child.write(line.as_bytes());
            std::thread::sleep(Duration::from_millis(150));
            let _ = child.kill();
        }
    }
}

fn validate_stream_display(response: &serde_json::Value, display_id: u32) -> Result<(), String> {
    // Old CV code only captures display 0. Never let it silently pair a
    // desktop frame with touches bound to a secondary game display.
    let captured = response
        .get("displayId")
        .and_then(serde_json::Value::as_u64);
    if captured == Some(u64::from(display_id)) || (display_id == 0 && captured.is_none()) {
        Ok(())
    } else {
        Err(format!(
            "视频流未确认目标显示屏 {display_id}，请更新 CV 代码后重试"
        ))
    }
}

impl Drop for SidecarClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation_error::{AutomationError, AutomationResult, AutomationResultExt};

    fn client_without_process() -> SidecarClient {
        let (_, line_rx) = mpsc::channel();
        SidecarClient {
            child: None,
            line_rx,
            next_id: 1,
            stream_size: None,
            stream_device: None,
            healthy: true,
            detection_screens: None,
            diagnostic_supported: false,
            diagnostic_enabled: false,
            recognition_context: serde_json::json!({}),
            recognition_device: serde_json::json!({}),
        }
    }

    #[test]
    fn secondary_display_requires_acknowledgement_from_cv_code() {
        assert!(validate_stream_display(&serde_json::json!({"displayId": 2}), 2).is_ok());
        assert!(validate_stream_display(&serde_json::json!({"displayId": 0}), 2).is_err());
        assert!(validate_stream_display(&serde_json::json!({}), 2).is_err());
        assert!(validate_stream_display(&serde_json::json!({}), 0).is_ok());
    }

    #[test]
    fn cached_stream_is_only_reused_for_same_device_and_display() {
        let mut client = client_without_process();
        client.stream_device = Some(("emulator-5554".into(), 2));
        assert!(client.stream_matches_device(Some("emulator-5554"), 2));
        assert!(!client.stream_matches_device(Some("emulator-5554"), 0));
        assert!(!client.stream_matches_device(Some("emulator-5556"), 2));
        assert!(!client.stream_matches_device(None, 2));
    }

    #[test]
    fn template_failure_exits_before_fallback_or_action_and_discards_client() {
        let mut client = client_without_process();
        let response = serde_json::json!({
            "errorCode": "template_load_failed",
            "templateKey": "battle/attack",
            "templatePath": "/tmp/attack.png",
            "error": "template load failed: battle/attack (/tmp/attack.png)"
        });
        let mut acted = false;
        let result: AutomationResult<()> = (|| {
            // A normal recognition error may reach the existing fallback;
            // a template failure must escape it and the enclosing step.
            let _ = client.check_response(&response).recoverable()?.ok();
            acted = true;
            Ok(())
        })();
        assert!(
            matches!(result, Err(AutomationError::Cv(CvError::TemplateLoad { ref key, ref path, .. }))
            if key.as_deref() == Some("battle/attack") && path.as_deref() == Some("/tmp/attack.png"))
        );
        assert!(!acted);
        assert!(!client.is_healthy());
        let cache = Mutex::new(None);
        client.return_to_cache(Some(&cache));
        assert!(cache.lock().unwrap().is_none());
    }

    #[test]
    fn ordinary_recognition_response_keeps_client_healthy() {
        let mut client = client_without_process();
        for response in [
            serde_json::json!({"found": false, "score": 0.1}),
            serde_json::json!({"error": "frame unavailable"}),
        ] {
            assert!(client.check_response(&response).is_ok());
            assert!(client.is_healthy());
        }
    }
}
