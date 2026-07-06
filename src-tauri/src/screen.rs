use base64::Engine;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

mod types;
pub use types::*;

pub const SIDECAR_STARTUP_REDOWNLOAD_MESSAGE: &str =
    "CV 运行时启动失败，请前往资源管理重新下载 CV 运行时。";

pub fn sidecar_startup_user_message(error: &str) -> Option<&'static str> {
    if error.contains("sidecar did not become ready")
        || error.contains("OpenCV bindings requires")
        || error.contains("invalid JSON from sidecar")
    {
        Some(SIDECAR_STARTUP_REDOWNLOAD_MESSAGE)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Sidecar client — communicates with the Python mash-cv process
// ---------------------------------------------------------------------------

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
            .ok_or_else(|| "无法解析 mash-cv code runtime，请先安装 CV 代码包".to_string())?;
        if !code_dir.join("mash_cv").is_dir() {
            return Err(format!(
                "未找到 mash-cv code 包 ({}). 请先下载并安装当前版本需要的 CV 代码包。",
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
            .env(
                "MASH_CV_MODELS_DIR",
                models_dir.to_string_lossy().to_string(),
            )
            .env(
                "MASH_CV_SERVANTS_JSON_PATH",
                servants_json.to_string_lossy().to_string(),
            )
            .env("PYTHONPATH", code_dir.to_string_lossy().to_string());
        let (mut rx, child) = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn sidecar: {e}"))?;

        // Bridge the async tokio receiver into a sync std::mpsc channel so the
        // blocking runner thread can call recv() without an async runtime.
        let (line_tx, line_rx) = mpsc::channel::<String>();

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
                        eprintln!("[mash-cv stderr] {msg}");
                    }
                    CommandEvent::Error(e) => {
                        eprintln!("[mash-cv error] {e}");
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
        };

        // Wait for the sidecar to finish its cold-boot before sending any real
        // commands. The PyInstaller onefile bundle (OpenCV + PyAV) can take
        // 20-40s on macOS the first time it's extracted. A dedicated `ping`
        // response tells us Python's REPL is running and keeps subsequent
        // request/response pairs in lockstep.
        let ping = serde_json::json!({ "cmd": "ping" });
        match client.send_recv_with_timeout(&ping, Duration::from_secs(90)) {
            Ok(resp) => eprintln!("[mash-cv] ping -> {resp}"),
            Err(e) => {
                let detail = format!("sidecar did not become ready: {e}");
                eprintln!("[mash-cv startup] {detail}");
                return Err(detail);
            }
        }

        for (idx, dir) in template_dirs.iter().enumerate() {
            if dir.dir.exists() {
                let req = serde_json::json!({
                    "cmd": "load_templates",
                    "dir": dir.dir.to_string_lossy(),
                    "append": idx > 0,
                    "keyPrefix": dir.key_prefix.as_deref().unwrap_or(""),
                });
                match client.send_recv(&req) {
                    Ok(resp) => eprintln!("[mash-cv] load_templates -> {resp}"),
                    Err(e) => eprintln!("[mash-cv] load_templates failed: {e}"),
                }
            } else {
                eprintln!("[mash-cv] templates dir missing: {}", dir.dir.display());
            }
        }

        for (idx, path) in config_paths.iter().enumerate() {
            if path.exists() {
                let req = serde_json::json!({
                    "cmd": "load_config",
                    "path": path.to_string_lossy(),
                    "merge": idx > 0,
                });
                match client.send_recv(&req) {
                    Ok(resp) => eprintln!("[mash-cv] load_config -> {resp}"),
                    Err(e) => eprintln!("[mash-cv] load_config failed: {e}"),
                }
            } else {
                eprintln!("[mash-cv] config path missing: {}", path.display());
            }
        }

        // Tell the sidecar which server's OCR model to use. Sent after
        // templates + config so a fresh `_ocr_engine` rebuild sees the
        // right model on the first `find_supports` call. Failures are
        // logged but non-fatal: an older sidecar binary that doesn't
        // know `set_server` simply stays on its compile-time default.
        let req = serde_json::json!({
            "cmd": "set_server",
            "server": server.to_string(),
        });
        match client.send_recv(&req) {
            Ok(resp) => eprintln!("[mash-cv] set_server -> {resp}"),
            Err(e) => eprintln!("[mash-cv] set_server failed (sidecar may be stale): {e}"),
        }

        Ok(client)
    }

    /// Send a JSON command and wait for the JSON response line.
    fn send_recv(&mut self, request: &serde_json::Value) -> Result<serde_json::Value, String> {
        self.send_recv_with_timeout(request, Duration::from_secs(10))
    }

    /// Like `send_recv` but with a caller-specified timeout. Used by commands
    /// that can legitimately take longer than 10s (sidecar cold boot, scrcpy
    /// handshake).
    ///
    /// Tags the outgoing request with a monotonic ``id`` and discards any
    /// response whose ``id`` doesn't match -- those are stale leftovers from
    /// a previously timed-out call still in flight from the sidecar.
    fn send_recv_with_timeout(
        &mut self,
        request: &serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);

        let mut tagged = request.clone();
        if let Some(obj) = tagged.as_object_mut() {
            obj.insert("id".into(), serde_json::Value::from(id));
        } else {
            return Err(format!(
                "send_recv: request must be a JSON object, got {request}"
            ));
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
                Some(rid) if rid == id => return Ok(parsed),
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
                    ));
                }
            }
        }
    }

    /// Helper to inject `imagePath` into the request when the caller provided one.
    /// Passing `None` makes the sidecar read the latest frame from the scrcpy stream.
    fn add_image_path(req: &mut serde_json::Value, image_path: Option<&Path>) {
        if let Some(p) = image_path {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "imagePath".into(),
                    serde_json::Value::String(p.to_string_lossy().into_owned()),
                );
            }
        }
    }

    /// Detect which screen is shown. Pass `None` to use the live scrcpy frame.
    pub fn detect(&mut self, image_path: Option<&Path>) -> Result<Screen, String> {
        let (screen, _) = self.detect_full(image_path)?;
        Ok(screen)
    }

    /// Like `detect` but also returns the classifier score.
    pub fn detect_full(&mut self, image_path: Option<&Path>) -> Result<(Screen, f64), String> {
        let (screen_str, score) = self.detect_label_full(image_path)?;
        let screen = screen_str.parse::<Screen>().unwrap_or(Screen::Unknown);
        Ok((screen, score))
    }

    /// Like `detect_full`, but preserves the raw configured screen name.
    pub fn detect_label_full(
        &mut self,
        image_path: Option<&Path>,
    ) -> Result<(String, f64), String> {
        let mut req = serde_json::json!({ "cmd": "detect" });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        let screen_str = resp["screen"].as_str().unwrap_or("Unknown");
        let score = resp["score"].as_f64().unwrap_or(0.0);
        Ok((screen_str.to_string(), score))
    }

    /// Search for a template element within a region. Pass `None` to use the
    /// live scrcpy frame.
    pub fn find_element(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        region: NormRect,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let mut req = serde_json::json!({
            "cmd": "find_element",
            "templateKey": template_key,
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "threshold": threshold,
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        // Sidecar surfaces hard errors (e.g. ``template not loaded: <key>``)
        // through the ``error`` field. Treat those as Err so callers don't
        // silently degrade to "not found" — historically this masked a
        // missing-template bug for hours during the support-select work.
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_element({template_key}): {err}"));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Search for an AP recovery item icon, but only accept it when the
    /// surrounding row is enabled. Depleted rows keep the item icon visible
    /// under a dark overlay, so plain template matching is not enough.
    pub fn find_enabled_ap_recovery_item(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        region: NormRect,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let mut req = serde_json::json!({
            "cmd": "find_element",
            "templateKey": template_key,
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "threshold": threshold,
            "requireApRecoveryEnabled": true,
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_element({template_key}): {err}"));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Like `find_element`, but returns the full match (score + bounding box)
    /// for debug/visualization purposes.
    pub fn find_element_full(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        region: NormRect,
        threshold: f64,
    ) -> Result<ElementMatch, String> {
        let mut req = serde_json::json!({
            "cmd": "find_element",
            "templateKey": template_key,
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "threshold": threshold,
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_element({template_key}): {err}"));
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|r| {
            Some(NormRect {
                x: r.get("x")?.as_f64()?,
                y: r.get("y")?.as_f64()?,
                w: r.get("w")?.as_f64()?,
                h: r.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }

    /// Look up an element by `(screen, element)` name in the loaded cv.json
    /// and run template matching for it.
    pub fn find_element_by_name(
        &mut self,
        image_path: Option<&Path>,
        screen: &str,
        element: &str,
    ) -> Result<ElementMatch, String> {
        let mut req = serde_json::json!({
            "cmd": "find_element_by_name",
            "screen": screen,
            "element": element,
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            if !resp.get("found").and_then(|v| v.as_bool()).unwrap_or(false) {
                return Err(err.to_string());
            }
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|r| {
            Some(NormRect {
                x: r.get("x")?.as_f64()?,
                y: r.get("y")?.as_f64()?,
                w: r.get("w")?.as_f64()?,
                h: r.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }

    /// Return the mean grayscale luma for a normalized region.
    pub fn probe_skill_use_dialog(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        dialog_region: NormRect,
        dialog_threshold: f64,
        confirm_region: NormRect,
    ) -> Result<SkillUseDialogProbe, String> {
        let mut req = serde_json::json!({
            "cmd": "probe_skill_use_dialog",
            "templateKey": template_key,
            "dialogRegion": {
                "x": dialog_region.x,
                "y": dialog_region.y,
                "w": dialog_region.w,
                "h": dialog_region.h,
            },
            "dialogThreshold": dialog_threshold,
            "confirmRegion": {
                "x": confirm_region.x,
                "y": confirm_region.y,
                "w": confirm_region.w,
                "h": confirm_region.h,
            },
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let mean_luma = resp["meanLuma"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|r| {
            Some(NormRect {
                x: r.get("x")?.as_f64()?,
                y: r.get("y")?.as_f64()?,
                w: r.get("w")?.as_f64()?,
                h: r.get("h")?.as_f64()?,
            })
        });
        let error = resp
            .get("error")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        Ok(SkillUseDialogProbe {
            found,
            score,
            mean_luma,
            region,
            error,
        })
    }

    /// Identify the suit and (optionally) servant occupying each fixed
    /// command-card slot on the attack screen.
    ///
    /// ``card_regions`` overrides the sidecar's built-in five-slot layout
    /// — pass ``None`` to use the defaults. ``assets_dir`` must contain
    /// ``{servant_id}/card_servant_*.png`` for identification to succeed;
    /// when missing or empty, only suit + slot position are returned.
    pub fn find_command_cards(
        &mut self,
        image_path: Option<&Path>,
        card_regions: Option<&[NormRect]>,
        servant_ids: &[u32],
        assets_dir: Option<&Path>,
    ) -> Result<Vec<CommandCardMatch>, String> {
        let mut req = serde_json::json!({
            "cmd": "find_command_cards",
            "servantIds": servant_ids,
        });
        if let Some(regions) = card_regions {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "cardRegions".into(),
                    serde_json::Value::Array(
                        regions
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "x": r.x, "y": r.y, "w": r.w, "h": r.h,
                                })
                            })
                            .collect(),
                    ),
                );
            }
        }
        if let Some(dir) = assets_dir {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "assetsDir".into(),
                    serde_json::Value::String(dir.to_string_lossy().into_owned()),
                );
            }
        }
        Self::add_image_path(&mut req, image_path);

        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            // Sidecar reports a missing frame / bad path here. Surface it.
            return Err(err.to_string());
        }
        let cards = resp
            .get("cards")
            .ok_or_else(|| "find_command_cards: response missing 'cards'".to_string())?;
        serde_json::from_value::<Vec<CommandCardMatch>>(cards.clone())
            .map_err(|e| format!("invalid command-card response: {e}"))
    }

    /// Report NP readiness from the bottom gauge percentages. ``np_regions``
    /// still provides the upper NP-card tap regions — pass ``None`` to use
    /// the defaults.
    pub fn find_noble_phantasms(
        &mut self,
        image_path: Option<&Path>,
        np_regions: Option<&[NormRect]>,
    ) -> Result<Vec<NoblePhantasmMatch>, String> {
        let mut req = serde_json::json!({
            "cmd": "find_noble_phantasms",
        });
        if let Some(regions) = np_regions {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "npRegions".into(),
                    serde_json::Value::Array(
                        regions
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "x": r.x, "y": r.y, "w": r.w, "h": r.h,
                                })
                            })
                            .collect(),
                    ),
                );
            }
        }
        Self::add_image_path(&mut req, image_path);

        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        let slots = resp
            .get("slots")
            .ok_or_else(|| "find_noble_phantasms: response missing 'slots'".to_string())?;
        serde_json::from_value::<Vec<NoblePhantasmMatch>>(slots.clone())
            .map_err(|e| format!("invalid noble-phantasm response: {e}"))
    }

    /// OCR the support-select screen's list region and return rows whose
    /// servant-name + NP-name fragments fuzzy-match one of ``expected_names`` and
    /// any of ``expected_np_names`` and sit close enough vertically to be
    /// part of the same row. Defaults (list region, thresholds, pair_dy)
    /// are owned by the sidecar; this binding stays minimal so retuning
    /// happens on the Python side.
    pub fn find_supports(
        &mut self,
        image_path: Option<&Path>,
        expected_name: &str,
        expected_names: &[String],
        expected_np_names: &[String],
        include_support_details: bool,
    ) -> Result<FindSupportsResult, String> {
        let mut req = serde_json::json!({
            "cmd": "find_supports",
            "expectedName": expected_name,
            "expectedNames": expected_names,
            "expectedNpNames": expected_np_names,
            "includeSupportDetails": include_support_details,
        });
        Self::add_image_path(&mut req, image_path);

        // OCR cold-start (loading the ONNX model on first call) can run
        // 5-10s on a fresh sidecar; bump the per-call timeout accordingly.
        // Subsequent calls return in <1s.
        let resp = self.send_recv_with_timeout(&req, Duration::from_secs(30))?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<FindSupportsResult>(resp)
            .map_err(|e| format!("invalid find_supports response: {e}"))
    }

    /// Run OCR inside the given normalized region and return raw
    /// fragments plus a joined text blob.
    pub fn ocr_region(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<OcrRegionResult, String> {
        let mut req = serde_json::json!({
            "cmd": "ocr_region",
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv_with_timeout(&req, Duration::from_secs(30))?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<OcrRegionResult>(resp)
            .map_err(|e| format!("invalid ocr_region response: {e}"))
    }

    /// Read the servant-enhancement level pair (``current/max``) using the
    /// sidecar's digit-template mapper.
    pub fn read_level_digits(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<LevelDigitsResult, String> {
        let mut req = serde_json::json!({
            "cmd": "read_level_digits",
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<LevelDigitsResult>(resp)
            .map_err(|e| format!("invalid read_level_digits response: {e}"))
    }

    /// Read the bond level-up result overlay and return the post-upgrade
    /// bond level when the sidecar can resolve it.
    pub fn read_bond_level_up(
        &mut self,
        image_path: Option<&Path>,
        debug: bool,
    ) -> Result<BondLevelUpReadResult, String> {
        let mut req = serde_json::json!({
            "cmd": "read_bond_level_up",
            "debug": debug,
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv_with_timeout(&req, Duration::from_secs(30))?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<BondLevelUpReadResult>(resp)
            .map_err(|e| format!("invalid read_bond_level_up response: {e}"))
    }

    /// Score a support row's CE icon against the bundled template.
    ///
    /// The runner calls this once per OCR-matched support row when a CE
    /// is pinned on the team-builder support slot. ``region`` is the
    /// search window in absolute normalized coordinates (computed by
    /// applying ``SUPPORT_CE_OFFSET_IN_ROW`` to the row's bbox).
    /// ``template_path`` points at ``assets/ces/{id}/card_ce.png``.
    ///
    /// Returns ``(score, passed)``; both are ``(0.0, false)`` if the
    /// template can't be read or the crop is empty so the caller can
    /// treat read failures the same as score failures.
    pub fn verify_support_ce(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
        template_path: &Path,
        threshold: f64,
        options: SupportCeVerificationOptions,
    ) -> Result<SupportCeVerificationResult, String> {
        let mut req = serde_json::json!({
            "cmd": "verify_support_ce",
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "templatePath": template_path.to_string_lossy(),
            "threshold": threshold,
            "mlbRequired": options.mlb_required,
            "grandBondCeMode": options.grand_bond_ce_mode,
            "fullGateThreshold": options.full_gate_threshold,
            "mlbIconThreshold": options.mlb_icon_threshold,
            "bondIconThreshold": options.bond_icon_threshold,
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<SupportCeVerificationResult>(resp)
            .map_err(|e| format!("invalid verify_support_ce response: {e}"))
    }

    /// Search for an arbitrary grayscale template file within a region.
    #[allow(dead_code)]
    pub fn find_region(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let mut req = serde_json::json!({
            "cmd": "find_region",
            "templatePath": template_path.to_string_lossy(),
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "threshold": threshold,
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Search for an arbitrary template file after cropping the template.
    #[allow(dead_code)]
    pub fn find_region_with_template_crop(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let mut req = serde_json::json!({
            "cmd": "find_region",
            "templatePath": template_path.to_string_lossy(),
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "templateCrop": {
                "x": template_crop.x,
                "y": template_crop.y,
                "w": template_crop.w,
                "h": template_crop.h,
            },
            "threshold": threshold,
        });
        if let Some((w, h)) = template_size {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("templateSize".into(), serde_json::json!({ "w": w, "h": h }));
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Search for an arbitrary template file after cropping the template,
    /// returning the raw match payload even when it misses the threshold.
    #[allow(dead_code)]
    pub fn find_region_with_template_crop_full(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
    ) -> Result<ElementMatch, String> {
        let mut req = serde_json::json!({
            "cmd": "find_region",
            "templatePath": template_path.to_string_lossy(),
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "templateCrop": {
                "x": template_crop.x,
                "y": template_crop.y,
                "w": template_crop.w,
                "h": template_crop.h,
            },
            "threshold": threshold,
        });
        if let Some((w, h)) = template_size {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("templateSize".into(), serde_json::json!({ "w": w, "h": h }));
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()));
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|r| {
            Some(NormRect {
                x: r.get("x")?.as_f64()?,
                y: r.get("y")?.as_f64()?,
                w: r.get("w")?.as_f64()?,
                h: r.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }

    pub fn find_enhancement_servant_grid(
        &mut self,
        image_path: Option<&Path>,
        face_template_paths: &[PathBuf],
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
        retry_seconds: f64,
    ) -> Result<FindEnhancementServantGridResult, String> {
        let mut req = serde_json::json!({
            "cmd": "find_enhancement_servant_grid",
            "anchorTemplateKey": "text_servant_avatar_bottom_line",
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "templateCrop": {
                "x": template_crop.x,
                "y": template_crop.y,
                "w": template_crop.w,
                "h": template_crop.h,
            },
            "faceThreshold": threshold,
            "retrySeconds": retry_seconds,
            "retryIntervalSeconds": 0.15,
            "faceTemplatePaths": face_template_paths
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        });
        if let Some((w, h)) = template_size {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("templateSize".into(), serde_json::json!({ "w": w, "h": h }));
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<FindEnhancementServantGridResult>(resp)
            .map_err(|e| format!("invalid find_enhancement_servant_grid response: {e}"))
    }

    /// Send a `read_battle_scene` request to the sidecar and return the
    /// raw JSON response. Shared by both the lean (`read_battle_scene`)
    /// and diagnostic (`read_battle_scene_debug`) variants so the
    /// request shape stays in one place.
    fn send_read_battle_scene(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
        debug: bool,
    ) -> Result<serde_json::Value, String> {
        let mut req = serde_json::json!({
            "cmd": "read_battle_scene",
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
        });
        if debug {
            req["debug"] = serde_json::Value::Bool(true);
        }
        Self::add_image_path(&mut req, image_path);
        self.send_recv(&req)
    }

    /// Read the current battle-scene indicator (`m` of `n`) drawn next to
    /// the BATTLE label in the top-right HUD. Returns `None` when the
    /// sidecar cannot resolve both numbers (anchor missing, NP overlay
    /// covering the strip, etc.).
    pub fn read_battle_scene(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<Option<(u32, u32)>, String> {
        let resp = self.send_read_battle_scene(image_path, region, false)?;
        let scene = resp["scene"].as_u64().map(|n| n as u32);
        let total = resp["total"].as_u64().map(|n| n as u32);
        Ok(scene.zip(total))
    }

    /// Diagnostic variant of [`Self::read_battle_scene`] that asks the
    /// sidecar for the full intermediate state (anchor score & box,
    /// strip, every above-threshold digit candidate, the kept set after
    /// NMS, the chosen split + best gap, and a `failReason` enum). Used
    /// only by the `debug_read_battle_scene` Tauri command — the runner
    /// stays on the lean variant.
    pub fn read_battle_scene_debug(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<serde_json::Value, String> {
        self.send_read_battle_scene(image_path, region, true)
    }

    /// Start the scrcpy server on the device and begin streaming.
    /// Returns the negotiated codec (width, height) -- already downscaled by
    /// the device if `max_size` was non-zero, so this is what every decoded
    /// frame will measure, not the raw display size.
    pub fn start_stream(
        &mut self,
        adb_path: &Path,
        jar_path: &Path,
        serial: Option<&str>,
        max_size: u32,
        bit_rate: u32,
    ) -> Result<(u32, u32), String> {
        let mut req = serde_json::json!({
            "cmd": "start_stream",
            "adbPath": adb_path.to_string_lossy(),
            "jarPath": jar_path.to_string_lossy(),
            "maxSize": max_size,
            "bitRate": bit_rate,
        });
        if let Some(s) = serial {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("serial".into(), serde_json::Value::String(s.to_string()));
            }
        }
        let resp = self.send_recv_with_timeout(&req, Duration::from_secs(45))?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("start_stream failed: {err}"));
        }
        let w = resp["width"].as_u64().unwrap_or(0) as u32;
        let h = resp["height"].as_u64().unwrap_or(0) as u32;
        if w == 0 || h == 0 {
            return Err("start_stream returned invalid dimensions".into());
        }
        self.stream_size = Some((w, h));
        Ok((w, h))
    }

    /// Stop the scrcpy server / decoder thread. Safe to call if not started.
    pub fn stop_stream(&mut self) -> Result<(), String> {
        let req = serde_json::json!({ "cmd": "stop_stream" });
        let resp = self.send_recv(&req)?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("stop_stream failed: {err}"));
        }
        self.stream_size = None;
        Ok(())
    }

    /// Return the (width, height) of the live stream, if one was started.
    pub fn stream_size(&self) -> Option<(u32, u32)> {
        self.stream_size
    }

    /// Return the latest decoded frame as a JPEG byte buffer. ``wait_seconds``
    /// is the maximum time the sidecar will block waiting for a fresh frame
    /// (0 = return immediately if none is cached).
    pub fn get_frame_jpeg(&mut self, wait_seconds: f64) -> Result<Vec<u8>, String> {
        let (b64, _, _) = self.get_frame_jpeg_base64(wait_seconds)?;
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("invalid base64 in jpegB64: {e}"))
    }

    /// Return the latest decoded frame as base64 JPEG plus frame dimensions.
    pub fn get_frame_jpeg_base64(
        &mut self,
        wait_seconds: f64,
    ) -> Result<(String, u32, u32), String> {
        let req = serde_json::json!({
            "cmd": "get_frame",
            "waitSeconds": wait_seconds,
        });
        let resp = self.send_recv(&req)?;
        if !resp["ok"].as_bool().unwrap_or(false) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("get_frame failed: {err}"));
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
            let req = serde_json::json!({"cmd": "quit"});
            let mut line = req.to_string();
            line.push('\n');
            let _ = child.write(line.as_bytes());
            std::thread::sleep(Duration::from_millis(150));
            let _ = child.kill();
        }
    }
}

impl Drop for SidecarClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
