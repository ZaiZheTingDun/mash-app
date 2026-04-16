use std::io::ErrorKind;
use std::path::Path;
use std::str::FromStr;
use std::sync::mpsc;
use std::time::Duration;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

// ---------------------------------------------------------------------------
// Core geometry types (normalized 0.0..1.0)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
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
}

impl SidecarClient {
    /// Spawn the mash-cv sidecar and optionally load templates + config.
    pub fn spawn(
        app: &tauri::AppHandle,
        templates_dir: Option<&Path>,
        config_path: Option<&Path>,
    ) -> Result<Self, String> {
        let shell = app.shell();
        let cmd = shell
            .sidecar("mash-cv")
            .map_err(|e| format!("failed to create sidecar command: {e}"))?;

        let (mut rx, child) = cmd
            .spawn()
            .map_err(|e| {
                if let tauri_plugin_shell::Error::Io(io_err) = &e {
                    if io_err.kind() == ErrorKind::NotFound {
                        return "failed to spawn sidecar: 未找到 mash-cv sidecar 可执行文件。请先在项目根目录执行 `cd sidecar/mash_cv && bash build_sidecar.sh` 构建 sidecar，再重新运行应用。".to_string();
                    }
                }
                format!("failed to spawn sidecar: {e}")
            })?;

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
        };

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
        let mut line = request.to_string();
        line.push('\n');
        self.child
            .as_mut()
            .ok_or_else(|| "sidecar process already stopped".to_string())?
            .write(line.as_bytes())
            .map_err(|e| format!("failed to write to sidecar: {e}"))?;

        let response = self
            .line_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|e| format!("sidecar response timeout: {e}"))?;

        serde_json::from_str(&response)
            .map_err(|e| format!("invalid JSON from sidecar: {e}: {response}"))
    }

    /// Detect which screen is shown in the screenshot at `image_path`.
    pub fn detect(&mut self, image_path: &Path) -> Result<Screen, String> {
        let (screen, _) = self.detect_full(image_path)?;
        Ok(screen)
    }

    /// Like `detect` but also returns the classifier score.
    pub fn detect_full(&mut self, image_path: &Path) -> Result<(Screen, f64), String> {
        let req = serde_json::json!({
            "cmd": "detect",
            "imagePath": image_path.to_string_lossy(),
        });
        let resp = self.send_recv(&req)?;
        let screen_str = resp["screen"].as_str().unwrap_or("Unknown");
        let screen = screen_str.parse::<Screen>().unwrap_or(Screen::Unknown);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        Ok((screen, score))
    }

    /// Search for a template element within a region of the screenshot.
    pub fn find_element(
        &mut self,
        image_path: &Path,
        template_key: &str,
        region: NormRect,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let req = serde_json::json!({
            "cmd": "find_element",
            "imagePath": image_path.to_string_lossy(),
            "templateKey": template_key,
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "threshold": threshold,
        });
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
        image_path: &Path,
        template_key: &str,
        region: NormRect,
        threshold: f64,
    ) -> Result<ElementMatch, String> {
        let req = serde_json::json!({
            "cmd": "find_element",
            "imagePath": image_path.to_string_lossy(),
            "templateKey": template_key,
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
            "threshold": threshold,
        });
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
        image_path: &Path,
        screen: &str,
        element: &str,
    ) -> Result<ElementMatch, String> {
        let req = serde_json::json!({
            "cmd": "find_element_by_name",
            "imagePath": image_path.to_string_lossy(),
            "screen": screen,
            "element": element,
        });
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

    /// Read the current turn number from the battle screen.
    /// Returns None if the sidecar cannot detect the turn number.
    pub fn read_turn(
        &mut self,
        image_path: &Path,
        region: NormRect,
    ) -> Result<Option<u32>, String> {
        let req = serde_json::json!({
            "cmd": "read_turn",
            "imagePath": image_path.to_string_lossy(),
            "region": {
                "x": region.x,
                "y": region.y,
                "w": region.w,
                "h": region.h,
            },
        });
        let resp = self.send_recv(&req)?;
        Ok(resp["turn"].as_u64().map(|n| n as u32))
    }

    /// Tell the sidecar to exit.
    pub fn shutdown(&mut self) {
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
