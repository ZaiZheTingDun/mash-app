use std::path::PathBuf;
use std::process::Command;
use tauri::Manager;

#[derive(Clone)]
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

/// Target wall-clock interval between adjacent MOVE events during the
/// active swipe phase, in milliseconds. The on-screen list view re-
/// renders on each MOVE event it receives, so this interval is what
/// caps the perceived frame rate of the scroll animation — a 67-ms
/// interval (the previous fixed 6-step shape over a 400-ms swipe)
/// drops the active phase to ~15 Hz, which reads as visibly choppy on
/// 60–120 Hz displays. 20 ms ≈ 50 Hz lifts the animation past the
/// perceptual smoothness threshold on the displays this app targets,
/// while staying loose enough that the per-step `input motionevent`
/// spawn overhead (≈5 ms on real devices, up to ~150 ms on slow
/// emulators) doesn't routinely dominate the realized swipe time.
const SETTLE_SWIPE_TARGET_STEP_MS: u32 = 20;

/// Minimum number of MOVE events, regardless of swipe duration. A
/// few-event swipe is what `input swipe` already does and it lets
/// Android extrapolate a fling, which is exactly the behaviour
/// `swipe_with_settle` exists to avoid.
const SETTLE_SWIPE_MIN_STEPS: u32 = 4;

/// Upper bound on MOVE event count so a pathologically long swipe (or
/// a slow emulator where each `input motionevent` invocation already
/// adds 100+ ms of overhead) can't blow the realized swipe time past
/// the planned `swipe_ms` by an order of magnitude. 30 events over a
/// 600-ms swipe is ≈50 Hz; anything denser is below the noise floor
/// of `input motionevent` jitter anyway.
const SETTLE_SWIPE_MAX_STEPS: u32 = 30;

/// Pick how many MOVE events to emit for a swipe of `swipe_ms` ms so
/// the on-device event cadence approaches
/// `SETTLE_SWIPE_TARGET_STEP_MS` between adjacent events. Exposed for
/// unit tests so they can derive the expected script shape from the
/// same formula instead of duplicating it.
fn settle_swipe_move_steps(swipe_ms: u32) -> u32 {
    let target = swipe_ms.saturating_div(SETTLE_SWIPE_TARGET_STEP_MS).max(1);
    target.clamp(SETTLE_SWIPE_MIN_STEPS, SETTLE_SWIPE_MAX_STEPS)
}

/// Build the chained `adb shell` script that emits the
/// DOWN → MOVE×N → MOVE(settle) → UP sequence for `swipe_with_settle`.
///
/// Extracted as a pure function so unit tests can assert the exact
/// command shape without needing a real ADB device. Step count scales
/// with `swipe_ms` via `settle_swipe_move_steps` so a short swipe gets
/// the minimum smoothing budget and a long swipe spreads MOVE events
/// across the full duration at ~50 Hz instead of clumping them.
fn build_settle_swipe_script(
    from: (u32, u32),
    to: (u32, u32),
    swipe_ms: u32,
    settle_ms: u32,
) -> String {
    let steps = settle_swipe_move_steps(swipe_ms);
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
        // 600-ms swipe → 30 evenly-spaced MOVE steps from y=1500 to
        // y=500 → step size of (1500 - 500) / 30 ≈ 33.33 px.
        // Sample a handful of the interpolated y values along the
        // way; the exact rounded pixel positions are derived from
        // the same step count the production code picks, so any
        // future tuning of `settle_swipe_move_steps` only needs to
        // keep the linear-interp contract intact.
        let swipe_ms = 600u32;
        let steps = settle_swipe_move_steps(swipe_ms);
        let script = build_settle_swipe_script((540, 1500), (540, 500), swipe_ms, 400);
        // First, midpoint, and last MOVE positions — these pin the
        // start/middle/end of the interpolation without depending on
        // the exact step count.
        for &i in &[1, steps / 2, steps] {
            let t = i as f64 / steps as f64;
            let expected_y = (1500.0 + (500.0 - 1500.0) * t).round() as u32;
            assert!(
                script.contains(&format!("MOVE 540 {expected_y}")),
                "expected interpolated MOVE at y={expected_y} (step {i}/{steps}), got: {script}"
            );
        }
    }

    #[test]
    fn settle_swipe_script_uses_per_step_sleep_proportional_to_swipe_ms() {
        // `swipe_ms / steps` ≈ target step interval. 600 ms across
        // 30 steps = 20 ms = 0.020 s per step (matches the target
        // cadence — see `SETTLE_SWIPE_TARGET_STEP_MS`).
        let swipe_ms = 600u32;
        let steps = settle_swipe_move_steps(swipe_ms);
        let per_step_ms = swipe_ms as f64 / steps as f64;
        let script = build_settle_swipe_script((540, 1500), (540, 500), swipe_ms, 400);
        let needle = format!("sleep {per_step_ms_s:.3}", per_step_ms_s = per_step_ms / 1000.0);
        let count = script.matches(&needle).count();
        assert_eq!(
            count, steps as usize,
            "expected {steps} '{needle}' segments (one per interpolated MOVE), got: {script}"
        );
    }

    #[test]
    fn settle_swipe_script_handles_diagonal_swipe() {
        // Diagonal motion must interpolate both axes — sample the
        // first, midpoint, and final positions for a 600 px × 600 px
        // diagonal from (100,200) to (700,800).
        let swipe_ms = 600u32;
        let steps = settle_swipe_move_steps(swipe_ms);
        let script = build_settle_swipe_script((100, 200), (700, 800), swipe_ms, 300);
        for &i in &[1, steps / 2, steps] {
            let t = i as f64 / steps as f64;
            let expected_x = (100.0 + 600.0 * t).round() as u32;
            let expected_y = (200.0 + 600.0 * t).round() as u32;
            assert!(
                script.contains(&format!("MOVE {expected_x} {expected_y}")),
                "expected MOVE at ({expected_x},{expected_y}) for step {i}/{steps}, got: {script}"
            );
        }
    }

    #[test]
    fn settle_swipe_move_steps_targets_50hz_within_bounds() {
        // Short swipes get the minimum step floor so even a 100-ms
        // swipe is broken into a handful of MOVE events instead of
        // landing in a single jump (which is what `input swipe` does
        // and is exactly what triggers Android's fling extrapolation).
        assert_eq!(
            settle_swipe_move_steps(0),
            SETTLE_SWIPE_MIN_STEPS,
            "zero-length swipe must still emit the minimum steps"
        );
        assert_eq!(
            settle_swipe_move_steps(50),
            SETTLE_SWIPE_MIN_STEPS,
            "swipe under {} ms must clamp to the {}-step floor",
            SETTLE_SWIPE_MIN_STEPS * SETTLE_SWIPE_TARGET_STEP_MS,
            SETTLE_SWIPE_MIN_STEPS
        );
        // Typical scroll swipes (400–600 ms) target ~50 Hz, so step
        // count scales linearly with `swipe_ms` in this range.
        assert_eq!(settle_swipe_move_steps(400), 20);
        assert_eq!(settle_swipe_move_steps(600), 30);
        // Long swipes are capped so we don't pile on hundreds of
        // events when a planned 2-second swipe meets a slow emulator
        // whose per-`input` overhead already paces events at ~10 Hz.
        assert_eq!(settle_swipe_move_steps(2000), SETTLE_SWIPE_MAX_STEPS);
        assert_eq!(settle_swipe_move_steps(u32::MAX), SETTLE_SWIPE_MAX_STEPS);
    }
}
