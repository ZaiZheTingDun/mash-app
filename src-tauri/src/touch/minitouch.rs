//! `minitouch` backend.
//!
//! [DeviceFarmer/minitouch] is a tiny native server that lives on the
//! Android device, listens on an abstract Unix socket, and streams
//! touch events directly to `/dev/input/event*`. Compared to
//! `adb shell input motionevent` (which spawns a fresh JVM per event,
//! ~30–80 ms each), minitouch processes commands in well under a
//! millisecond — so a swipe with 60+ MOVE events at 16 ms intervals
//! (visually indistinguishable from real finger motion) is feasible,
//! whereas the same gesture over `input motionevent` would take 2–5
//! seconds and look choppy.
//!
//! Bring-up: on `start` we push the bundled binary, `chmod 755` it,
//! spawn `adb shell /data/local/tmp/minitouch`, set up an `adb forward`
//! to its abstract socket, and connect via TCP. The TCP connection is
//! kept open for the lifetime of the backend, so per-swipe latency
//! is just the local TCP write + the device-side event injection.
//!
//! Per-ABI binaries live under
//! `src-tauri/resources/minitouch/<abi>/minitouch`. Missing-ABI cases
//! return an error from `start`, which the factory in `touch::build`
//! turns into an auto-fallback to `AdbInputBackend`.
//!
//! [DeviceFarmer/minitouch]: https://github.com/DeviceFarmer/minitouch

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};

use crate::adb::Adb;
use crate::touch::TouchBackend;

/// Where on the device we push the minitouch binary. Under
/// `/data/local/tmp` because that path is writable by `shell` on every
/// Android version without root, and binaries there can be `exec`'d.
const DEVICE_BINARY_PATH: &str = "/data/local/tmp/minitouch";

/// Abstract socket name minitouch listens on. Hard-coded by the
/// minitouch binary itself; if the upstream project ever changes this,
/// update here in lockstep.
const MINITOUCH_ABSTRACT_SOCKET: &str = "minitouch";

/// Max time we'll wait for the device-side minitouch to start listening
/// on its abstract socket after we spawn it. 2 s is generous — startup
/// is usually <100ms — but we want a clean error message if e.g. the
/// binary is for the wrong ABI and crashed immediately.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Pressure value sent with every touch event. minitouch normalizes
/// against the value advertised in the banner; 50 is a safe middle
/// (Android scroll handlers don't care about pressure beyond
/// "not zero").
const TOUCH_PRESSURE: u32 = 50;

/// Briefest "press, release" we can issue. Some Android scroll
/// handlers debounce on the order of 50–80 ms, so an absolutely
/// instantaneous DOWN-UP can be missed. 30 ms keeps the gesture
/// well clear of that floor while still feeling instant to the
/// operator.
const TAP_DURATION_MS: u32 = 30;

/// Default per-step duration for the active phase of a `swipe` /
/// `swipe_with_settle`. ~60 fps cadence; small enough that motion
/// looks continuous, large enough to not flood the device-side event
/// queue.
const MOVE_STEP_MS: u32 = 16;

pub struct MinitouchBackend {
    /// Cloned for `forward_remove` on Drop. Cheap (3 fields, none of
    /// them allocate per call).
    adb: Adb,
    /// Local TCP port that `adb forward` mapped to the device-side
    /// abstract socket.
    local_port: u16,
    /// The `adb shell /data/local/tmp/minitouch` child process. Owned
    /// here purely so we can `kill()` it on drop — killing the adb
    /// shell SIGHUPs the device-side binary too.
    child: Child,
    /// Open TCP stream to the forwarded port. Writes go straight into
    /// minitouch's command stream.
    stream: TcpStream,
    /// Device screen dimensions (in physical pixels) — used to scale
    /// the trait's pixel coords into minitouch's banner coordinate
    /// system, which is in `[0, max_x] × [0, max_y]` and may use a
    /// wholly different scale (often `32767 × 32767` for the touch
    /// digitizer regardless of display resolution).
    screen_w: u32,
    screen_h: u32,
    /// Maximum X coordinate minitouch accepts (from the banner).
    max_x: u32,
    /// Maximum Y coordinate minitouch accepts (from the banner).
    max_y: u32,
    /// Maximum pressure value minitouch accepts (from the banner).
    /// Currently informational only — we always send `TOUCH_PRESSURE`.
    #[allow(dead_code)]
    max_pressure: u32,
}

impl MinitouchBackend {
    /// Push, spawn, forward, connect — the full bring-up sequence.
    ///
    /// `binary_dir` should be a directory containing per-ABI
    /// subfolders (e.g. `arm64-v8a/minitouch`). Tauri resolves this
    /// from the bundle / dev manifest in `Runner::new`; we don't
    /// reach for `AppHandle` here so the module stays unit-testable.
    pub fn start(
        adb: Adb,
        binary_dir: &Path,
        screen: (u32, u32),
    ) -> Result<Self, String> {
        let abi = adb
            .getprop("ro.product.cpu.abi")
            .map_err(|e| format!("failed to detect device ABI: {e}"))?;
        let local_binary = resolve_binary_path(binary_dir, &abi)?;

        // Best-effort: kill any minitouch leftover from a previous
        // run. Failures here are fine — usually it means there was
        // nothing to kill in the first place.
        let _ = adb.shell_run(&format!("pkill -f {DEVICE_BINARY_PATH}"));

        adb.push_file(&local_binary, DEVICE_BINARY_PATH)
            .map_err(|e| format!("failed to push minitouch binary: {e}"))?;
        adb.shell_run(&format!("chmod 755 {DEVICE_BINARY_PATH}"))
            .map_err(|e| format!("failed to chmod minitouch binary: {e}"))?;

        let child = adb
            .shell_spawn(DEVICE_BINARY_PATH)
            .map_err(|e| format!("failed to spawn minitouch: {e}"))?;

        // tcp:0 → adb picks a free port; avoids collisions if the
        // user already has another minitouch instance forwarded.
        let local_port = adb
            .forward_tcp_to_abstract(0, MINITOUCH_ABSTRACT_SOCKET)
            .map_err(|e| format!("failed to forward minitouch port: {e}"))?;

        let stream = connect_with_retry(local_port, CONNECT_TIMEOUT)
            .map_err(|e| format!("failed to connect to minitouch on tcp:{local_port}: {e}"))?;
        let (max_x, max_y, max_pressure) = read_banner(&stream)
            .map_err(|e| format!("failed to read minitouch banner: {e}"))?;

        Ok(Self {
            adb,
            local_port,
            child,
            stream,
            screen_w: screen.0,
            screen_h: screen.1,
            max_x,
            max_y,
            max_pressure,
        })
    }

    /// Write a pre-built minitouch command script to the open stream
    /// and flush. Centralized so `tap` / `swipe` / `swipe_with_settle`
    /// share the error-mapping path.
    fn send(&mut self, script: &str) -> Result<(), String> {
        self.stream
            .write_all(script.as_bytes())
            .map_err(|e| format!("minitouch write failed: {e}"))?;
        self.stream
            .flush()
            .map_err(|e| format!("minitouch flush failed: {e}"))
    }

    /// Convert a pixel point in the device's display resolution to
    /// minitouch's banner coordinate system.
    fn to_mt(&self, px: u32, py: u32) -> (u32, u32) {
        pixel_to_minitouch(
            px,
            py,
            (self.screen_w, self.screen_h),
            (self.max_x, self.max_y),
        )
    }
}

impl TouchBackend for MinitouchBackend {
    fn tap(&mut self, x: u32, y: u32) -> Result<(), String> {
        let (mtx, mty) = self.to_mt(x, y);
        let script = build_tap_script(mtx, mty, TOUCH_PRESSURE, TAP_DURATION_MS);
        self.send(&script)
    }

    fn swipe(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        duration_ms: u32,
    ) -> Result<(), String> {
        let from_mt = self.to_mt(from.0, from.1);
        let to_mt = self.to_mt(to.0, to.1);
        let script = build_swipe_script(
            from_mt,
            to_mt,
            duration_ms,
            /* settle_ms */ 0,
            TOUCH_PRESSURE,
        );
        self.send(&script)
    }

    fn swipe_with_settle(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        swipe_ms: u32,
        settle_ms: u32,
    ) -> Result<(), String> {
        let from_mt = self.to_mt(from.0, from.1);
        let to_mt = self.to_mt(to.0, to.1);
        let script = build_swipe_script(from_mt, to_mt, swipe_ms, settle_ms, TOUCH_PRESSURE);
        self.send(&script)
    }

    fn name(&self) -> &'static str {
        "minitouch"
    }
}

impl Drop for MinitouchBackend {
    fn drop(&mut self) {
        // Best-effort cleanup; nothing here is allowed to panic since
        // we may be unwinding. The order matters: closing the TCP
        // stream first lets minitouch shut down gracefully when its
        // socket EOFs, killing the adb-shell child is the hard
        // backstop, and removing the port forward last keeps
        // `adb forward --list` tidy between runs.
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Err(err) = self.adb.forward_remove(self.local_port) {
            eprintln!(
                "[minitouch] failed to remove tcp:{} forward: {err}",
                self.local_port
            );
        }
    }
}

// --- pure helpers --------------------------------------------------

/// Resolve `<binary_dir>/<abi>/minitouch`, returning a clear error
/// if the per-ABI binary is missing. Kept separate from `start()`
/// so unit tests can exercise the path logic without an ADB device.
fn resolve_binary_path(binary_dir: &Path, abi: &str) -> Result<PathBuf, String> {
    let path = binary_dir.join(abi).join("minitouch");
    if !path.is_file() {
        return Err(format!(
            "no minitouch binary bundled for ABI `{abi}` (expected at {})",
            path.display()
        ));
    }
    Ok(path)
}

/// Poll-connect to `127.0.0.1:port`, retrying every 25ms until either
/// `adb forward`'d socket starts accepting connections or `timeout`
/// elapses. The forwarded port is "live" on adb's side the moment
/// `adb forward` returns, but the device-side minitouch may still be
/// initializing its event listener, so a freshly-issued connect can
/// race the binary's startup.
fn connect_with_retry(port: u16, timeout: Duration) -> Result<TcpStream, String> {
    let deadline = Instant::now() + timeout;
    let addr = format!("127.0.0.1:{port}");
    let mut last_err: Option<String> = None;
    while Instant::now() < deadline {
        match TcpStream::connect(&addr) {
            Ok(stream) => {
                // Banner reads are short; the swipe writes are also
                // bounded. A read timeout guards against a hung server.
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                return Ok(stream);
            }
            Err(e) => last_err = Some(e.to_string()),
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(format!(
        "timed out after {:?} (last error: {})",
        timeout,
        last_err.unwrap_or_else(|| "no attempts made".into())
    ))
}

/// Read minitouch's 3-line startup banner:
///
/// ```text
/// v <protocol_version>
/// ^ <max_contacts> <max_x> <max_y> <max_pressure>
/// $ <pid>
/// ```
///
/// We only really need the `^` line — the version and PID are
/// informational. Returns `(max_x, max_y, max_pressure)`.
fn read_banner(stream: &TcpStream) -> Result<(u32, u32, u32), String> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut line = String::new();
    for _ in 0..3 {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("connection closed before banner finished".into());
        }
        if let Some(rest) = line.trim().strip_prefix("^ ") {
            return parse_banner_caret_line(rest);
        }
    }
    Err(format!("did not receive `^` banner line; got: {line:?}"))
}

fn parse_banner_caret_line(rest: &str) -> Result<(u32, u32, u32), String> {
    let mut parts = rest.split_whitespace();
    let _max_contacts = parts
        .next()
        .ok_or_else(|| "banner `^` line missing max_contacts".to_string())?
        .parse::<u32>()
        .map_err(|e| format!("banner max_contacts: {e}"))?;
    let max_x = parts
        .next()
        .ok_or_else(|| "banner `^` line missing max_x".to_string())?
        .parse::<u32>()
        .map_err(|e| format!("banner max_x: {e}"))?;
    let max_y = parts
        .next()
        .ok_or_else(|| "banner `^` line missing max_y".to_string())?
        .parse::<u32>()
        .map_err(|e| format!("banner max_y: {e}"))?;
    let max_pressure = parts
        .next()
        .ok_or_else(|| "banner `^` line missing max_pressure".to_string())?
        .parse::<u32>()
        .map_err(|e| format!("banner max_pressure: {e}"))?;
    Ok((max_x, max_y, max_pressure))
}

/// Convert a display-pixel coordinate into a minitouch banner
/// coordinate. The display and digitizer ranges are independent on
/// most Android devices (e.g. 1080 × 1920 display + 32767 × 32767
/// digitizer), so we always scale rather than passing through.
fn pixel_to_minitouch(
    px: u32,
    py: u32,
    screen: (u32, u32),
    max: (u32, u32),
) -> (u32, u32) {
    let nx = if screen.0 == 0 { 0.0 } else { px as f64 / screen.0 as f64 };
    let ny = if screen.1 == 0 { 0.0 } else { py as f64 / screen.1 as f64 };
    let mtx = (nx.clamp(0.0, 1.0) * max.0 as f64).round() as u32;
    let mty = (ny.clamp(0.0, 1.0) * max.1 as f64).round() as u32;
    (mtx, mty)
}

/// Number of intermediate MOVE events for the active swipe phase. At
/// ~60 fps cadence (`swipe_ms / MOVE_STEP_MS`), a 555 ms swipe gets
/// ~35 events — well into "looks like a finger" territory. We cap
/// the count so a long swipe doesn't generate megabytes of commands,
/// but the cap is far above any realistic swipe length.
fn pick_step_count(swipe_ms: u32) -> u32 {
    let raw = (swipe_ms / MOVE_STEP_MS).max(1);
    raw.min(120)
}

/// Build a "tap" command sequence: DOWN, brief wait, UP. minitouch
/// supports an `input motionevent`-style tap directly, but the
/// pattern below works on every minitouch version and gives us a
/// knob (`hold_ms`) to tune if a particular UI insists on a longer
/// press.
fn build_tap_script(x: u32, y: u32, pressure: u32, hold_ms: u32) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "d 0 {x} {y} {pressure}");
    let _ = writeln!(s, "c");
    let _ = writeln!(s, "w {hold_ms}");
    let _ = writeln!(s, "u 0");
    let _ = writeln!(s, "c");
    s
}

/// Build the full text protocol payload for a swipe. `settle_ms = 0`
/// turns this into a fling-allowed swipe with no settle phase;
/// non-zero values produce the "press, drag, hold, release" pattern
/// that suppresses fling.
fn build_swipe_script(
    from: (u32, u32),
    to: (u32, u32),
    swipe_ms: u32,
    settle_ms: u32,
    pressure: u32,
) -> String {
    let steps = pick_step_count(swipe_ms);
    let step_ms = (swipe_ms / steps).max(1);

    let mut script = String::new();
    let _ = writeln!(script, "d 0 {} {} {pressure}", from.0, from.1);
    let _ = writeln!(script, "c");

    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let x = (from.0 as f64 + (to.0 as f64 - from.0 as f64) * t).round() as u32;
        let y = (from.1 as f64 + (to.1 as f64 - from.1 as f64) * t).round() as u32;
        let _ = writeln!(script, "w {step_ms}");
        let _ = writeln!(script, "m 0 {x} {y} {pressure}");
        let _ = writeln!(script, "c");
    }

    if settle_ms > 0 {
        // Settle: hold the contact at `to` long enough that
        // Android's velocity tracker computes ~0 px/s on lift-off
        // and skips fling. The trailing MOVE-at-`to` after the wait
        // gives the tracker an explicit zero-velocity sample right
        // before UP, which is more robust across Android versions
        // than relying on "no events" being interpreted as zero
        // velocity.
        let _ = writeln!(script, "w {settle_ms}");
        let _ = writeln!(script, "m 0 {} {} {pressure}", to.0, to.1);
        let _ = writeln!(script, "c");
    }

    let _ = writeln!(script, "u 0");
    let _ = writeln!(script, "c");
    script
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_binary_path_returns_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let abi_dir = dir.path().join("arm64-v8a");
        std::fs::create_dir_all(&abi_dir).unwrap();
        let binary = abi_dir.join("minitouch");
        std::fs::write(&binary, b"placeholder").unwrap();
        let resolved = resolve_binary_path(dir.path(), "arm64-v8a").unwrap();
        assert_eq!(resolved, binary);
    }

    #[test]
    fn resolve_binary_path_errors_on_missing_abi() {
        let dir = tempfile::tempdir().unwrap();
        let err = resolve_binary_path(dir.path(), "x86_64").unwrap_err();
        assert!(err.contains("x86_64"));
        assert!(err.contains("minitouch"));
    }

    #[test]
    fn parse_banner_caret_line_extracts_max_x_y_pressure() {
        let parsed = parse_banner_caret_line("10 32767 32767 100").unwrap();
        assert_eq!(parsed, (32767, 32767, 100));
    }

    #[test]
    fn parse_banner_caret_line_rejects_malformed_input() {
        assert!(parse_banner_caret_line("10 32767").is_err());
        assert!(parse_banner_caret_line("not a banner").is_err());
    }

    #[test]
    fn pixel_to_minitouch_scales_proportionally() {
        // 1080×1920 display, 32767×32767 digitizer:
        //   nx = 540/1080 = 0.5    → 16384
        //   ny = 1498/1920 ≈ 0.7802 → 25565
        let (x, y) = pixel_to_minitouch(540, 1498, (1080, 1920), (32767, 32767));
        assert_eq!((x, y), (16384, 25565));
    }

    #[test]
    fn pixel_to_minitouch_clamps_to_max_range() {
        // Pixel coords outside the screen rect (jitter overflow, etc.)
        // must not blow past the digitizer max — clamp instead of
        // overflowing u32.
        let (x, y) = pixel_to_minitouch(9999, 9999, (1080, 1920), (32767, 32767));
        assert_eq!((x, y), (32767, 32767));
    }

    #[test]
    fn pixel_to_minitouch_handles_zero_screen_dim() {
        // Defensive: a bogus 0-sized screen must not divide by zero.
        let (x, y) = pixel_to_minitouch(500, 500, (0, 0), (32767, 32767));
        assert_eq!((x, y), (0, 0));
    }

    #[test]
    fn tap_script_emits_down_wait_up_commit_cycle() {
        let s = build_tap_script(540, 1500, 50, 30);
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines, ["d 0 540 1500 50", "c", "w 30", "u 0", "c"]);
    }

    #[test]
    fn swipe_script_with_settle_starts_with_down_at_from() {
        let script = build_swipe_script((100, 200), (900, 800), 400, 300, 50);
        let first_line = script.lines().next().unwrap();
        assert_eq!(first_line, "d 0 100 200 50");
        assert_eq!(script.lines().nth(1).unwrap(), "c");
    }

    #[test]
    fn swipe_script_with_settle_ends_with_settle_then_up() {
        let script = build_swipe_script((100, 200), (900, 800), 400, 300, 50);
        let lines: Vec<&str> = script.lines().collect();
        let n = lines.len();
        // …; w <settle>; m 0 to_x to_y P; c; u 0; c
        assert_eq!(lines[n - 5], "w 300");
        assert_eq!(lines[n - 4], "m 0 900 800 50");
        assert_eq!(lines[n - 3], "c");
        assert_eq!(lines[n - 2], "u 0");
        assert_eq!(lines[n - 1], "c");
    }

    #[test]
    fn swipe_script_without_settle_skips_settle_phase() {
        let script = build_swipe_script((100, 200), (900, 800), 400, 0, 50);
        // No `w 0` at the tail; the gesture ends with the final
        // interpolated MOVE → u 0 → c.
        let lines: Vec<&str> = script.lines().collect();
        let n = lines.len();
        assert_eq!(lines[n - 2], "u 0");
        // Two before the UP should be the commit of the final
        // interpolation MOVE, not a settle move.
        assert_eq!(lines[n - 3], "c");
        // The text "w 0" must NOT appear — we omit the settle entirely
        // when settle_ms is zero.
        assert!(!script.contains("\nw 0\n"));
    }

    #[test]
    fn swipe_script_step_count_scales_with_duration() {
        assert_eq!(pick_step_count(960), 60);
        assert_eq!(pick_step_count(400), 25);
        assert_eq!(pick_step_count(50), 3);
    }

    #[test]
    fn swipe_script_step_count_capped_and_floored() {
        assert_eq!(pick_step_count(60_000), 120);
        assert_eq!(pick_step_count(0), 1);
    }

    #[test]
    fn swipe_script_emits_one_move_per_step_plus_settle_move() {
        let script = build_swipe_script((100, 200), (900, 800), 400, 300, 50);
        let move_count = script.lines().filter(|l| l.starts_with("m 0 ")).count();
        assert_eq!(move_count, 25 + 1);
    }

    #[test]
    fn read_banner_parses_realistic_payload() {
        let (mut server, client) = pipe_streams();
        let writer = thread::spawn(move || {
            server
                .write_all(b"v 1\n^ 10 32767 32767 100\n$ 1234\n")
                .unwrap();
        });
        let (max_x, max_y, max_p) = read_banner(&client).unwrap();
        writer.join().unwrap();
        assert_eq!((max_x, max_y, max_p), (32767, 32767, 100));
    }

    fn pipe_streams() -> (TcpStream, TcpStream) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (server, _addr) = listener.accept().unwrap();
        (server, client)
    }
}
