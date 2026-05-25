use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use tauri::Manager;

pub struct Adb {
    adb_path: PathBuf,
    device: Option<String>,
    use_bluestack: bool,
}

#[cfg(windows)]
fn adb_executable_name() -> &'static str {
    "adb.exe"
}

#[cfg(not(windows))]
fn adb_executable_name() -> &'static str {
    "adb"
}

fn bundled_adb_candidates(resource_dir: PathBuf) -> [PathBuf; 2] {
    [
        resource_dir.join("adb").join(adb_executable_name()),
        resource_dir
            .join("resources")
            .join("adb")
            .join(adb_executable_name()),
    ]
}

pub(crate) fn resolve_adb_path(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .resource_dir()
        .ok()
        .and_then(|dir| {
            bundled_adb_candidates(dir)
                .into_iter()
                .find(|path| path.is_file())
        })
        .unwrap_or_else(|| PathBuf::from(adb_executable_name()))
}

impl Adb {
    pub fn new(app: &tauri::AppHandle, use_bluestack: bool) -> Self {
        Self {
            adb_path: resolve_adb_path(app),
            device: None,
            use_bluestack,
        }
    }

    /// Serial of the connected device (populated after `connect`).
    pub fn serial(&self) -> Option<&str> {
        self.device.as_deref()
    }

    pub fn path(&self) -> &std::path::Path {
        &self.adb_path
    }

    fn base_args(&self) -> Vec<String> {
        match &self.device {
            Some(d) => vec!["-s".into(), d.clone()],
            None => vec![],
        }
    }

    fn parse_first_ready_device(output: &str) -> Option<String> {
        output.lines().skip(1).find_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            let mut parts = trimmed.split('\t');
            let serial = parts.next()?.trim();
            let status = parts.next()?.trim();
            if status == "device" {
                Some(serial.to_string())
            } else {
                None
            }
        })
    }

    #[allow(dead_code)]
    fn parse_size_token(text: &str) -> Option<(u32, u32)> {
        let token = text.split_whitespace().find(|part| part.contains('x'))?;
        let (w, h) = token.split_once('x')?;
        let width = w.parse::<u32>().ok()?;
        let height = h.parse::<u32>().ok()?;
        Some((width, height))
    }

    /// Detect and connect to a device. Must be called before other operations.
    pub fn connect(&mut self) -> Result<(), String> {
        if self.use_bluestack {
            Command::new(&self.adb_path)
                .args(["connect", "127.0.0.1:5555"])
                .output()
                .ok();
        }

        let output = Command::new(&self.adb_path)
            .arg("devices")
            .output()
            .map_err(|e| format!("failed to run adb: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        self.device = Self::parse_first_ready_device(&stdout);

        if self.device.is_some() {
            Ok(())
        } else {
            Err("no device found".into())
        }
    }

    /// Kept as a fallback for environments where scrcpy reports odd dimensions.
    #[allow(dead_code)]
    pub fn screen_size(&self) -> Option<(u32, u32)> {
        let mut args = self.base_args();
        args.extend(["shell".into(), "wm".into(), "size".into()]);
        let output = Command::new(&self.adb_path).args(&args).output().ok()?;
        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().find_map(|line| {
            let trimmed = line.trim();
            if let Some((_, rhs)) = trimmed.split_once(':') {
                Self::parse_size_token(rhs.trim())
            } else {
                Self::parse_size_token(trimmed)
            }
        })
    }

    /// Capture a screenshot and write the PNG to a temp file.
    /// Returns the path. Caller is responsible for deleting the file.
    ///
    /// Superseded by the scrcpy video stream in production paths; kept for
    /// tests and one-off debugging.
    #[allow(dead_code)]
    pub fn screenshot_to_file(&self) -> Result<PathBuf, String> {
        let mut args = self.base_args();
        args.extend(["exec-out".into(), "screencap".into(), "-p".into()]);

        let output = Command::new(&self.adb_path)
            .args(&args)
            .output()
            .map_err(|e| format!("adb screencap failed: {e}"))?;

        if !output.status.success() {
            return Err(format!("adb screencap exited with: {}", output.status));
        }

        if output.stdout.is_empty() {
            return Err("adb screencap returned empty output".into());
        }

        let tmp = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .map_err(|e| format!("failed to create temp file: {e}"))?;

        let path = tmp.path().to_path_buf();

        // Persist the file so it outlives this scope (caller deletes it)
        tmp.persist(&path)
            .map_err(|e| format!("failed to persist temp file: {e}"))?;

        std::fs::write(&path, &output.stdout)
            .map_err(|e| format!("failed to write screenshot: {e}"))?;

        Ok(path)
    }

    pub fn tap(&self, x: u32, y: u32) -> Result<(), String> {
        let mut args = self.base_args();
        args.extend([
            "shell".into(),
            "input".into(),
            "tap".into(),
            x.to_string(),
            y.to_string(),
        ]);

        let output = Command::new(&self.adb_path)
            .args(&args)
            .output()
            .map_err(|e| format!("adb tap failed: {e}"))?;
        if !output.status.success() {
            return Err(format!("adb tap exited with: {}", output.status));
        }

        Ok(())
    }

    pub fn swipe(&self, from: (u32, u32), to: (u32, u32), duration_ms: u32) -> Result<(), String> {
        let mut args = self.base_args();
        args.extend([
            "shell".into(),
            "input".into(),
            "swipe".into(),
            from.0.to_string(),
            from.1.to_string(),
            to.0.to_string(),
            to.1.to_string(),
            duration_ms.to_string(),
        ]);

        let output = Command::new(&self.adb_path)
            .args(&args)
            .output()
            .map_err(|e| format!("adb swipe failed: {e}"))?;
        if !output.status.success() {
            return Err(format!("adb swipe exited with: {}", output.status));
        }

        Ok(())
    }

    /// Read a single property via `adb shell getprop <name>`. Returns
    /// the trimmed value, or an error string on failure / empty output.
    /// Used at minitouch startup to detect the device's ABI so we push
    /// the matching native binary.
    pub fn getprop(&self, name: &str) -> Result<String, String> {
        let mut args = self.base_args();
        args.extend(["shell".into(), "getprop".into(), name.into()]);
        let output = Command::new(&self.adb_path)
            .args(&args)
            .output()
            .map_err(|e| format!("adb getprop {name} failed: {e}"))?;
        if !output.status.success() {
            return Err(format!("adb getprop {name} exited with: {}", output.status));
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if value.is_empty() {
            return Err(format!("adb getprop {name} returned empty value"));
        }
        Ok(value)
    }

    /// `adb push <local> <remote>`. Used to install runtime binaries
    /// (e.g. minitouch) under `/data/local/tmp/`.
    pub fn push_file(&self, local: &Path, remote: &str) -> Result<(), String> {
        let mut args = self.base_args();
        args.extend([
            "push".into(),
            local.to_string_lossy().into_owned(),
            remote.into(),
        ]);
        let output = Command::new(&self.adb_path)
            .args(&args)
            .output()
            .map_err(|e| format!("adb push failed: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "adb push {} → {remote} exited with: {} ({stderr})",
                local.display(),
                output.status,
            ));
        }
        Ok(())
    }

    /// Run a one-shot shell command (e.g. `chmod 755 /data/local/tmp/foo`).
    pub fn shell_run(&self, command: &str) -> Result<(), String> {
        let mut args = self.base_args();
        args.extend(["shell".into(), command.into()]);
        let output = Command::new(&self.adb_path)
            .args(&args)
            .output()
            .map_err(|e| format!("adb shell `{command}` failed: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "adb shell `{command}` exited with: {} ({stderr})",
                output.status
            ));
        }
        Ok(())
    }

    /// `adb forward tcp:<local> localabstract:<name>`. Returns the
    /// locally-bound port — if `local_port == 0`, adb picks a free port
    /// and prints it on stdout, which we parse out.
    pub fn forward_tcp_to_abstract(
        &self,
        local_port: u16,
        abstract_name: &str,
    ) -> Result<u16, String> {
        let mut args = self.base_args();
        args.extend([
            "forward".into(),
            format!("tcp:{local_port}"),
            format!("localabstract:{abstract_name}"),
        ]);
        let output = Command::new(&self.adb_path)
            .args(&args)
            .output()
            .map_err(|e| format!("adb forward failed: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "adb forward tcp:{local_port}→localabstract:{abstract_name} exited with: {} ({stderr})",
                output.status
            ));
        }
        if local_port != 0 {
            return Ok(local_port);
        }
        // adb prints the auto-assigned port to stdout when the caller
        // requested tcp:0. Parse it; otherwise the caller has no way of
        // knowing which port to connect to.
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .trim()
            .parse::<u16>()
            .map_err(|e| format!("failed to parse forwarded port from `{stdout}`: {e}"))
    }

    /// `adb forward --remove tcp:<port>`. Best-effort — failures are
    /// returned to the caller but losing a forwarded port at shutdown
    /// is not fatal (`adb` will GC it eventually).
    pub fn forward_remove(&self, local_port: u16) -> Result<(), String> {
        let mut args = self.base_args();
        args.extend([
            "forward".into(),
            "--remove".into(),
            format!("tcp:{local_port}"),
        ]);
        let output = Command::new(&self.adb_path)
            .args(&args)
            .output()
            .map_err(|e| format!("adb forward --remove failed: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "adb forward --remove tcp:{local_port} exited with: {}",
                output.status,
            ));
        }
        Ok(())
    }

    /// Spawn a long-lived `adb shell <command>` and return the child
    /// handle. The shell stays attached, so dropping / killing the
    /// child also SIGHUPs the device-side process (used to manage the
    /// minitouch server lifecycle).
    ///
    /// stdout / stderr are inherited so device-side logs surface in
    /// the Tauri process's own stderr — useful for diagnosing
    /// permission denials and similar startup failures.
    pub fn shell_spawn(&self, command: &str) -> Result<Child, String> {
        let mut args = self.base_args();
        args.extend(["shell".into(), command.into()]);
        Command::new(&self.adb_path)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("adb shell spawn `{command}` failed: {e}"))
    }

    /// "Press, drag, hold, release" — the human-finger gesture that
    /// `input swipe` can't model.
    ///
    /// `input swipe` interpolates linearly between `from` and `to` and
    /// lifts off at the average swipe velocity, which Android's
    /// `VelocityTracker` interprets as a *fling* once it crosses the
    /// per-device threshold (≈100–300 px/s on most modern devices).
    /// The fling continues scrolling the target view after the finger
    /// lifts, which is why high-velocity ADB swipes routinely overshoot
    /// their intended destination on long lists.
    ///
    /// This method drives the motion event stream directly via
    /// `input motionevent DOWN/MOVE/UP`, with a `settle_ms` hold at the
    /// final position before the UP event. Because no MOVE happens
    /// during the settle window, the velocity tracker computes ≈0 px/s
    /// at lift-off and the system never enters fling mode — so the
    /// caller can rely on the list stopping exactly where the finger
    /// landed.
    ///
    /// Requires Android API 28+ (`input motionevent` was added in P).
    /// All chained `input` invocations run in a single `adb shell`
    /// session so per-command spawn overhead doesn't dominate; the
    /// realized swipe duration on a device will be roughly
    /// `swipe_ms + settle_ms + N * per_input_overhead`, where
    /// `per_input_overhead` is typically 30–80 ms.
    pub fn swipe_with_settle(
        &self,
        from: (u32, u32),
        to: (u32, u32),
        swipe_ms: u32,
        settle_ms: u32,
    ) -> Result<(), String> {
        let script = build_settle_swipe_script(from, to, swipe_ms, settle_ms);
        let mut args = self.base_args();
        args.extend(["shell".into(), script]);

        let output = Command::new(&self.adb_path)
            .args(&args)
            .output()
            .map_err(|e| format!("adb swipe_with_settle failed: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "adb swipe_with_settle exited with: {} ({stderr})",
                output.status
            ));
        }

        Ok(())
    }
}

/// Number of MOVE events between DOWN and the final settle. More steps
/// = smoother visual motion but more per-`input` process overhead. Six
/// is a balance that keeps the chained shell command around ~600 chars
/// and gives noticeably smoother scroll-handler updates than a single
/// MOVE jump.
const SETTLE_SWIPE_MOVE_STEPS: u32 = 6;

/// Build the chained `adb shell` script that emits the
/// DOWN → MOVE×N → MOVE(settle) → UP sequence for `swipe_with_settle`.
///
/// Extracted as a pure function so unit tests can assert the exact
/// command shape without needing a real ADB device.
fn build_settle_swipe_script(
    from: (u32, u32),
    to: (u32, u32),
    swipe_ms: u32,
    settle_ms: u32,
) -> String {
    let steps = SETTLE_SWIPE_MOVE_STEPS;
    let step_sleep_s = (swipe_ms as f64 / steps as f64) / 1000.0;
    let settle_s = settle_ms as f64 / 1000.0;

    let mut script = format!("input motionevent DOWN {} {}", from.0, from.1);
    for i in 1..=steps {
        // Linear interpolation from `from` to `to`. Round to nearest
        // integer pixel; saturate at zero for safety.
        let t = i as f64 / steps as f64;
        let x = ((from.0 as f64) + ((to.0 as f64) - (from.0 as f64)) * t).round();
        let y = ((from.1 as f64) + ((to.1 as f64) - (from.1 as f64)) * t).round();
        let x = x.max(0.0) as u32;
        let y = y.max(0.0) as u32;
        script.push_str(&format!(
            "; sleep {step_sleep_s:.3}; input motionevent MOVE {x} {y}"
        ));
    }
    // Hold at `to` for `settle_ms` so the velocity tracker's sliding
    // window contains a stretch of "no motion" right before UP. The
    // trailing redundant MOVE at `to` makes the zero-velocity sample
    // explicit, which is more robust across Android versions than
    // relying on the implicit "no events" interpretation.
    script.push_str(&format!(
        "; sleep {settle_s:.3}; input motionevent MOVE {} {}; input motionevent UP {} {}",
        to.0, to.1, to.0, to.1
    ));
    script
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_adb_candidates_cover_dev_and_bundle_resource_layouts() {
        let base = PathBuf::from("/app/resources");
        assert_eq!(
            bundled_adb_candidates(base.clone())[0],
            base.join("adb/adb")
        );
        assert_eq!(
            bundled_adb_candidates(base.clone())[1],
            base.join("resources/adb/adb")
        );
    }

    #[test]
    fn settle_swipe_script_starts_with_down_at_from() {
        let script = build_settle_swipe_script((540, 1500), (540, 432), 600, 400);
        assert!(
            script.starts_with("input motionevent DOWN 540 1500"),
            "expected DOWN event at the start, got: {script}"
        );
    }

    #[test]
    fn settle_swipe_script_ends_with_move_then_up_at_destination() {
        // The MOVE-then-UP pattern at `to` is what guarantees the
        // velocity tracker sees a zero-velocity sample right before
        // lift-off; without it, the OS may extrapolate from an older
        // MOVE event and still fling.
        let script = build_settle_swipe_script((540, 1500), (540, 432), 600, 400);
        let expected_tail = "input motionevent MOVE 540 432; input motionevent UP 540 432";
        assert!(
            script.ends_with(expected_tail),
            "expected '{expected_tail}' at end, got: {script}"
        );
    }

    #[test]
    fn settle_swipe_script_includes_settle_sleep_before_lift() {
        // The settle sleep must appear after the last interpolated
        // MOVE and before the final MOVE+UP pair. 400 ms = 0.400 s.
        let script = build_settle_swipe_script((540, 1500), (540, 432), 600, 400);
        assert!(
            script.contains("sleep 0.400; input motionevent MOVE 540 432; input motionevent UP"),
            "expected '0.400' settle sleep before final lift, got: {script}"
        );
    }

    #[test]
    fn settle_swipe_script_interpolates_intermediate_moves() {
        // 6 MOVE steps for a vertical swipe from y=1500 to y=432 should
        // land at evenly spaced y values: 1322, 1144, 966, 788, 610, 432.
        let script = build_settle_swipe_script((540, 1500), (540, 432), 600, 400);
        for expected_y in [1322u32, 1144, 966, 788, 610, 432] {
            assert!(
                script.contains(&format!("MOVE 540 {expected_y}")),
                "expected interpolated MOVE at y={expected_y}, got: {script}"
            );
        }
    }

    #[test]
    fn settle_swipe_script_uses_per_step_sleep_proportional_to_swipe_ms() {
        // 600 ms across 6 steps = 100 ms = 0.100 s per step.
        let script = build_settle_swipe_script((540, 1500), (540, 432), 600, 400);
        let per_step_count = script.matches("sleep 0.100").count();
        // One sleep before each of the 6 interpolated MOVEs (the
        // settle sleep is "sleep 0.400" so it doesn't match).
        assert_eq!(
            per_step_count, 6,
            "expected 6 'sleep 0.100' segments (one per interpolated MOVE), got: {script}"
        );
    }

    #[test]
    fn settle_swipe_script_handles_diagonal_swipe() {
        // Diagonal motion must also interpolate both axes. 6 steps
        // from (100, 200) to (700, 800) → x deltas of 100 px/step.
        let script = build_settle_swipe_script((100, 200), (700, 800), 600, 300);
        for (expected_x, expected_y) in [(200u32, 300u32), (400, 500), (700, 800)] {
            assert!(
                script.contains(&format!("MOVE {expected_x} {expected_y}")),
                "expected MOVE at ({expected_x},{expected_y}), got: {script}"
            );
        }
    }
}
