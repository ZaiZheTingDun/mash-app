//! Minitouch client for low-latency, smooth touch injection.
//!
//! [DeviceFarmer/minitouch] is a small native server that lives on the
//! Android device, listens on an abstract Unix socket, and streams
//! touch events directly into `/dev/input/event*`. Compared to
//! `adb shell input motionevent` (which spawns a fresh JVM per event,
//! ~30–80 ms each), minitouch processes commands in under a millisecond
//! — so a swipe with 60+ MOVE events at 16 ms intervals (visually
//! indistinguishable from real finger motion) is feasible, whereas the
//! same gesture over `input motionevent` would take 2–5 seconds and
//! look choppy.
//!
//! This module owns the device-side minitouch process for the lifetime
//! of a `Runner`: on `start` it pushes the bundled binary,
//! `chmod 755`s it, spawns `adb shell /data/local/tmp/minitouch`, sets
//! up an `adb forward` to its abstract socket, and connects via TCP.
//! On `Drop` it kills the child (which SIGHUPs the device-side
//! process) and removes the forward.
//!
//! Currently only the swipe path is wired up — taps and drags still
//! use `Adb::tap` / `Adb::swipe`. The minitouch binary is bundled per
//! ABI under `src-tauri/resources/minitouch/<abi>/minitouch`; the only
//! ABI bundled today is `arm64-v8a` (modern emulators, real phones).
//! Missing-binary cases are returned as errors so callers can fall
//! back to the ADB-based swipe.
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

/// Where on the device we push the minitouch binary. Under
/// `/data/local/tmp` because that path is writable by `shell` on every
/// Android version without root, and binaries there can be `exec`'d.
const DEVICE_BINARY_PATH: &str = "/data/local/tmp/minitouch";

/// Abstract socket name minitouch listens on. Hard-coded by the
/// minitouch binary itself; if the upstream project ever changes this,
/// update here in lockstep.
const MINITOUCH_ABSTRACT_SOCKET: &str = "minitouch";

/// Max time we'll wait for the device-side minitouch to start listening
/// on its abstract socket after we spawn it. 2s is generous — startup
/// is usually <100ms — but we want a clean error message if e.g. the
/// binary is for the wrong ABI and crashed immediately.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Pressure value sent with every touch event. minitouch normalizes
/// against the value advertised in the banner; 50 is a safe middle
/// (Android scroll handlers don't care about pressure beyond "not zero").
const TOUCH_PRESSURE: u32 = 50;

/// Connected handle to a running device-side minitouch. Dropping this
/// kills the device-side process and removes the local port forward.
pub struct Minitouch {
    /// Local TCP port that `adb forward` mapped to the device-side
    /// abstract socket. Kept so `Drop` can remove the forward.
    local_port: u16,
    /// The `adb shell /data/local/tmp/minitouch` child process. Owned
    /// here purely so we can `kill()` it on drop — killing the adb
    /// shell SIGHUPs the device-side binary too.
    child: Child,
    /// Open TCP stream to the forwarded port. Writes go straight into
    /// minitouch's command stream.
    stream: TcpStream,
    /// Maximum X coordinate minitouch accepts (from the banner). Used
    /// to scale our normalized `[0.0, 1.0]` coords up to device pixels.
    max_x: u32,
    /// Maximum Y coordinate minitouch accepts (from the banner).
    max_y: u32,
    /// Maximum pressure value minitouch accepts (from the banner).
    /// Currently informational only — we always send `TOUCH_PRESSURE`,
    /// which is well under typical maxes (usually 100+).
    #[allow(dead_code)]
    max_pressure: u32,
}

impl Minitouch {
    /// Push, spawn, forward, connect — the full bring-up sequence.
    ///
    /// `binary_dir` should be a directory containing per-ABI
    /// subfolders (e.g. `arm64-v8a/minitouch`). The caller is expected
    /// to pass the runtime-resolved resources path; we don't reach for
    /// Tauri's `AppHandle` here so this module stays unit-testable.
    pub fn start(adb: &Adb, binary_dir: &Path) -> Result<Self, String> {
        let abi = adb
            .getprop("ro.product.cpu.abi")
            .map_err(|e| format!("failed to detect device ABI: {e}"))?;
        let local_binary = resolve_binary_path(binary_dir, &abi)?;

        // Best-effort: kill any minitouch leftover from a previous run.
        // Failures here are fine — usually it means there was nothing
        // to kill in the first place.
        let _ = adb.shell_run(&format!("pkill -f {DEVICE_BINARY_PATH}"));

        adb.push_file(&local_binary, DEVICE_BINARY_PATH)
            .map_err(|e| format!("failed to push minitouch binary: {e}"))?;
        adb.shell_run(&format!("chmod 755 {DEVICE_BINARY_PATH}"))
            .map_err(|e| format!("failed to chmod minitouch binary: {e}"))?;

        let child = adb
            .shell_spawn(DEVICE_BINARY_PATH)
            .map_err(|e| format!("failed to spawn minitouch: {e}"))?;

        // Use tcp:0 so adb assigns a free port — avoids collisions if
        // the user already has another minitouch instance forwarded.
        let local_port = adb
            .forward_tcp_to_abstract(0, MINITOUCH_ABSTRACT_SOCKET)
            .map_err(|e| format!("failed to forward minitouch port: {e}"))?;

        let stream = connect_with_retry(local_port, CONNECT_TIMEOUT)
            .map_err(|e| format!("failed to connect to minitouch on tcp:{local_port}: {e}"))?;
        let (max_x, max_y, max_pressure) = read_banner(&stream)
            .map_err(|e| format!("failed to read minitouch banner: {e}"))?;

        Ok(Self {
            local_port,
            child,
            stream,
            max_x,
            max_y,
            max_pressure,
        })
    }

    /// Drive a "press, drag, hold, release" gesture in normalized
    /// screen coordinates. `swipe_ms` is the active-motion duration;
    /// `settle_ms` is the hold-at-destination delay before lift-off
    /// (which is what suppresses Android's fling response).
    ///
    /// All timing inside the gesture is server-side (minitouch's `w`
    /// command), so the local TCP write is essentially instantaneous —
    /// motion smoothness is bounded only by minitouch's event injection
    /// rate, not by our network or process overhead.
    pub fn swipe_with_settle(
        &mut self,
        from: (f64, f64),
        to: (f64, f64),
        swipe_ms: u32,
        settle_ms: u32,
    ) -> Result<(), String> {
        let script = build_swipe_script(
            from,
            to,
            swipe_ms,
            settle_ms,
            self.max_x,
            self.max_y,
            TOUCH_PRESSURE,
        );
        self.stream
            .write_all(script.as_bytes())
            .map_err(|e| format!("minitouch write failed: {e}"))?;
        self.stream
            .flush()
            .map_err(|e| format!("minitouch flush failed: {e}"))?;
        Ok(())
    }
}

impl Drop for Minitouch {
    fn drop(&mut self) {
        // Best-effort cleanup; nothing here is allowed to panic since
        // we may be unwinding. The order matters: closing the TCP
        // stream first lets minitouch shut down gracefully when its
        // socket EOFs, but if it lingers, killing the adb shell child
        // is the hard backstop.
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Remove the port forward last — adb still cleans these up at
        // server shutdown if we forget, but leaving them around bloats
        // `adb forward --list` between runs.
        // We don't have an `Adb` handle here; the runner removes the
        // forward via `adb.forward_remove` after dropping us. (Storing
        // `Adb` here would force `Runner` to wrap it in Arc just for
        // this one path, which isn't worth it.)
        let _ = self.local_port; // Suppress unused-field warning when port-removal lives in Runner.
    }
}

impl Minitouch {
    /// Local TCP port the runner used `adb forward` to set up. The
    /// runner reads this in its own `Drop` to call `forward_remove`.
    pub fn local_port(&self) -> u16 {
        self.local_port
    }
}

/// Resolve `<binary_dir>/<abi>/minitouch`, returning a clear error if
/// the per-ABI binary is missing. Kept separate from `start()` so unit
/// tests can exercise the path logic without an ADB device.
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

/// Number of intermediate MOVE events for the active swipe phase. At
/// ~60 fps cadence (`swipe_ms / 16`), a 555 ms swipe gets ~35 events —
/// well into "looks like a finger" territory. We cap this so a long
/// swipe doesn't generate megabytes of commands, but the cap is far
/// above any realistic swipe length.
fn pick_step_count(swipe_ms: u32) -> u32 {
    let raw = (swipe_ms / 16).max(1);
    raw.min(120)
}

/// Build the full text protocol payload for a settle swipe. Pure
/// function so unit tests can pin the command shape without needing a
/// minitouch device.
///
/// All MOVE / DOWN / UP coords are scaled from the input `[0.0, 1.0]`
/// normalized range up to `[0, max_x]` × `[0, max_y]`, matching
/// minitouch's reported banner.
fn build_swipe_script(
    from: (f64, f64),
    to: (f64, f64),
    swipe_ms: u32,
    settle_ms: u32,
    max_x: u32,
    max_y: u32,
    pressure: u32,
) -> String {
    let steps = pick_step_count(swipe_ms);
    let step_ms = (swipe_ms / steps).max(1);

    let map_x = |nx: f64| (nx.clamp(0.0, 1.0) * max_x as f64).round() as u32;
    let map_y = |ny: f64| (ny.clamp(0.0, 1.0) * max_y as f64).round() as u32;

    let (x0, y0) = (map_x(from.0), map_y(from.1));
    let mut script = String::new();
    let _ = writeln!(script, "d 0 {x0} {y0} {pressure}");
    let _ = writeln!(script, "c");

    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let x = from.0 + (to.0 - from.0) * t;
        let y = from.1 + (to.1 - from.1) * t;
        let _ = writeln!(script, "w {step_ms}");
        let _ = writeln!(script, "m 0 {} {} {pressure}", map_x(x), map_y(y));
        let _ = writeln!(script, "c");
    }

    // Settle: hold the contact at `to` long enough that Android's
    // velocity tracker computes ~0 px/s on lift-off and skips fling.
    // The trailing MOVE-at-`to` after the wait gives the tracker an
    // explicit zero-velocity sample right before UP, which is more
    // robust across Android versions than relying on "no events" to
    // be interpreted as zero velocity.
    let (xn, yn) = (map_x(to.0), map_y(to.1));
    let _ = writeln!(script, "w {settle_ms}");
    let _ = writeln!(script, "m 0 {xn} {yn} {pressure}");
    let _ = writeln!(script, "c");
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
        assert!(
            err.contains("x86_64"),
            "expected error to mention missing ABI, got: {err}"
        );
        assert!(
            err.contains("minitouch"),
            "expected error to mention the missing file name, got: {err}"
        );
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
    fn swipe_script_starts_with_down_at_from_in_device_coords() {
        // from=(0.5, 0.78), max=(32767, 32767) → ~16384, 25558
        let script = build_swipe_script((0.5, 0.78), (0.5, 0.22), 400, 300, 32767, 32767, 50);
        let first_line = script.lines().next().unwrap();
        assert_eq!(first_line, "d 0 16384 25558 50");
        // Always followed by a commit so minitouch starts processing
        // immediately rather than waiting for the next event.
        assert_eq!(script.lines().nth(1).unwrap(), "c");
    }

    #[test]
    fn swipe_script_ends_with_settle_move_then_up() {
        let script = build_swipe_script((0.5, 0.78), (0.5, 0.22), 400, 300, 32767, 32767, 50);
        let lines: Vec<&str> = script.lines().collect();
        // …w <settle>; m 0 to_x to_y 50; c; u 0; c
        let n = lines.len();
        assert_eq!(lines[n - 5], "w 300");
        assert_eq!(lines[n - 4], "m 0 16384 7209 50");
        assert_eq!(lines[n - 3], "c");
        assert_eq!(lines[n - 2], "u 0");
        assert_eq!(lines[n - 1], "c");
    }

    #[test]
    fn swipe_script_clamps_normalized_coords_into_valid_range() {
        // A misdetection might produce normalized coords outside
        // [0, 1]; we clamp so the output is always a valid device
        // pixel rather than wrapping or panicking on the u32 cast.
        let script = build_swipe_script((1.5, -0.2), (0.5, 0.5), 100, 100, 1000, 2000, 50);
        let first_line = script.lines().next().unwrap();
        // 1.5 → clamped to 1.0 → 1000; -0.2 → 0.0 → 0
        assert_eq!(first_line, "d 0 1000 0 50");
    }

    #[test]
    fn swipe_script_step_count_scales_with_duration() {
        // 16ms cadence → 60-frame swipe (~60fps) at 960ms,
        // ~25-frame at 400ms, ~3-frame at 50ms.
        assert_eq!(pick_step_count(960), 60);
        assert_eq!(pick_step_count(400), 25);
        assert_eq!(pick_step_count(50), 3);
    }

    #[test]
    fn swipe_script_step_count_capped_at_120() {
        // Pathologically long swipes can't generate megabytes of
        // command bytes. 120 frames * ~30 chars/frame ≈ 4KB cap.
        assert_eq!(pick_step_count(5_000), 120);
        assert_eq!(pick_step_count(60_000), 120);
    }

    #[test]
    fn swipe_script_step_count_floored_at_one() {
        // A 0-ms or sub-step swipe still needs at least one MOVE event
        // between DOWN and the settle phase so the gesture is
        // recognized as motion (not a long-press).
        assert_eq!(pick_step_count(0), 1);
        assert_eq!(pick_step_count(8), 1);
    }

    #[test]
    fn swipe_script_emits_one_move_per_step_plus_settle_move() {
        // 400ms swipe → 25 interpolation MOVEs + 1 settle MOVE.
        let script = build_swipe_script((0.5, 0.78), (0.5, 0.22), 400, 300, 1000, 1000, 50);
        let move_count = script.lines().filter(|l| l.starts_with("m 0 ")).count();
        assert_eq!(move_count, 25 + 1);
    }

    /// Banner parsing should accept the exact format minitouch emits
    /// at startup. Concrete sample captured from an arm64-v8a build:
    /// ```
    /// v 1
    /// ^ 10 32767 32767 100
    /// $ 1234
    /// ```
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

    // Helper: build a back-to-back TcpStream pair via a local loopback
    // listener. Lets us drive read_banner with controlled input
    // without needing a real device.
    fn pipe_streams() -> (TcpStream, TcpStream) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (server, _addr) = listener.accept().unwrap();
        (server, client)
    }

}
