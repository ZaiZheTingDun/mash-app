use base64::Engine;
use std::path::Path;
use std::str::FromStr;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

// ---------------------------------------------------------------------------
// Core geometry types (normalized 0.0..1.0)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn to_physical(&self, width: u32, height: u32) -> (u32, u32) {
        (
            (self.x * width as f64) as u32,
            (self.y * height as f64) as u32,
        )
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct NormRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementMatch {
    pub found: bool,
    pub x: f64,
    pub y: f64,
    pub score: f64,
    pub region: Option<NormRect>,
}

/// One detected command-card slot on the attack screen.
///
/// The five slot bboxes are fixed positions (configured in the Python
/// sidecar's ``DEFAULT_COMMAND_CARD_SLOTS``) and always present in the
/// response. ``suit`` / ``icon_*`` are filled in when at least one suit
/// icon template scores inside the slot. ``servant_id`` / ``ascension``
/// / ``face_score`` are populated only when the caller passes a non-empty
/// candidate list **and** an assets directory containing
/// ``{id}/card_servant_*.png`` files.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandCardMatch {
    pub slot: u32,
    /// Tap point (slot center) in normalized coordinates.
    pub x: f64,
    pub y: f64,
    /// The slot bbox itself.
    pub card_region: NormRect,
    /// Upper-portion of the slot used as the face-template search area.
    pub face_region: NormRect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Suit code: ``"a"`` (Arts), ``"b"`` (Buster), or ``"q"`` (Quick).
    pub suit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_region: Option<NormRect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub servant_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascension: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_score: Option<f64>,
}

// ---------------------------------------------------------------------------
// Screen enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Screen {
    TeamConfirm,
    TeamChange,
    SupportSelect,
    ServantSelect,
    Battle,
    Attack,
    Unknown,
}

impl std::fmt::Display for Screen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TeamConfirm => write!(f, "TeamConfirm"),
            Self::TeamChange => write!(f, "TeamChange"),
            Self::SupportSelect => write!(f, "SupportSelect"),
            Self::ServantSelect => write!(f, "ServantSelect"),
            Self::Battle => write!(f, "Battle"),
            Self::Attack => write!(f, "Attack"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl FromStr for Screen {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let screen = match s {
            "TeamConfirm" => Self::TeamConfirm,
            "TeamChange" => Self::TeamChange,
            "SupportSelect" => Self::SupportSelect,
            "ServantSelect" => Self::ServantSelect,
            "Battle" => Self::Battle,
            "Attack" => Self::Attack,
            _ => Self::Unknown,
        };
        Ok(screen)
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

impl SidecarClient {
    /// Spawn the mash-cv sidecar and optionally load templates + config.
    pub fn spawn(
        app: &tauri::AppHandle,
        templates_dir: Option<&Path>,
        config_path: Option<&Path>,
    ) -> Result<Self, String> {
        let exe = crate::resolve_sidecar_exe(app)
            .ok_or_else(|| "failed to resolve sidecar resource_dir".to_string())?;
        if !exe.exists() {
            return Err(format!(
                "未找到 mash-cv sidecar 可执行文件 ({}). 请在项目根目录执行 `cd sidecar/mash_cv && bash build_sidecar.sh` 构建 sidecar，再重新运行应用。",
                exe.display()
            ));
        }

        let cmd = app.shell().command(&exe);
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
            Err(e) => return Err(format!("sidecar did not become ready: {e}")),
        }

        if let Some(dir) = templates_dir {
            if dir.exists() {
                let req = serde_json::json!({
                    "cmd": "load_templates",
                    "dir": dir.to_string_lossy(),
                });
                match client.send_recv(&req) {
                    Ok(resp) => eprintln!("[mash-cv] load_templates -> {resp}"),
                    Err(e) => eprintln!("[mash-cv] load_templates failed: {e}"),
                }
            } else {
                eprintln!("[mash-cv] templates dir missing: {}", dir.display());
            }
        }

        if let Some(path) = config_path {
            if path.exists() {
                let req = serde_json::json!({
                    "cmd": "load_config",
                    "path": path.to_string_lossy(),
                });
                match client.send_recv(&req) {
                    Ok(resp) => eprintln!("[mash-cv] load_config -> {resp}"),
                    Err(e) => eprintln!("[mash-cv] load_config failed: {e}"),
                }
            } else {
                eprintln!("[mash-cv] config path missing: {}", path.display());
            }
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
                .ok_or_else(|| {
                    format!("sidecar response timeout (waiting for id={id})")
                })?;
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
        let mut req = serde_json::json!({ "cmd": "detect" });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        let screen_str = resp["screen"].as_str().unwrap_or("Unknown");
        let screen = screen_str.parse::<Screen>().unwrap_or(Screen::Unknown);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        Ok((screen, score))
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

    /// Read the current turn number from the battle screen.
    /// Returns None if the sidecar cannot detect the turn number.
    pub fn read_turn(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<Option<u32>, String> {
        let mut req = serde_json::json!({
            "cmd": "read_turn",
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
        });
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        Ok(resp["turn"].as_u64().map(|n| n as u32))
    }

    /// Start the scrcpy server on the device and begin streaming.
    /// Returns the negotiated codec (width, height) -- already downscaled by
    /// the device if `max_size` was non-zero, so this is what every decoded
    /// frame will measure, not the raw display size.
    pub fn start_stream(
        &mut self,
        jar_path: &Path,
        serial: Option<&str>,
        max_size: u32,
        bit_rate: u32,
    ) -> Result<(u32, u32), String> {
        let mut req = serde_json::json!({
            "cmd": "start_stream",
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
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("invalid base64 in jpegB64: {e}"))
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
