//! `adb shell sendevent` backend.
//!
//! Writes raw evdev events to `/dev/input/event*`, the same interface
//! the kernel-level touchscreen driver exposes. Each `sendevent`
//! invocation inside the device is just an `open() + write() +
//! close()` on the event node, so per-event latency is well under a
//! millisecond — comparable to `minitouch`. Unlike minitouch, no
//! pushed native binary or forwarded `adb` port is needed, just
//! shell access plus the kernel's evdev path.
//!
//! Bring-up runs `adb shell getevent -pl`, parses the dump, picks
//! the touchscreen device (the one advertising `ABS_MT_POSITION_X/Y`
//! and `ABS_MT_TRACKING_ID`), and records its coordinate ranges. At
//! gesture time we build a chained `sendevent ...; sendevent ...;
//! sleep 0.X; ...` shell script and dispatch it in a single
//! `adb shell` invocation — that keeps the local ADB overhead to one
//! round-trip per gesture instead of one per event.
//!
//! Today this backend targets the Type B multi-touch protocol, which
//! every modern Android (4.0+) uses for capacitive screens. Type A
//! (older resistive panels, some industrial hardware) would need a
//! separate path; we error out at probe time if we can't find a
//! Type B device.

use std::fmt::Write as _;

use crate::adb::Adb;
use crate::touch::TouchBackend;

// ---- evdev constants ---------------------------------------------
//
// All from `linux/input-event-codes.h`. Stable for decades; not worth
// pulling in a crate for these few numbers.

const EV_SYN: u16 = 0x00;
const EV_ABS: u16 = 0x03;

const SYN_REPORT: u16 = 0x00;

const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;

/// Sentinel "no touch" value the kernel expects in `ABS_MT_TRACKING_ID`
/// to release a slot in Type B multi-touch.
const TRACKING_ID_RELEASE: i32 = -1;

/// Briefest "press, release" hold — same rationale as in `minitouch.rs`.
/// Some scroll handlers debounce taps under ~30 ms.
const TAP_HOLD_MS: u32 = 30;

/// Active-motion event cadence for `swipe` / `swipe_with_settle`.
/// 16 ms ≈ 60 fps; the device-side bottleneck is per-`sendevent`
/// process spawn (~1–5 ms on toybox), so this gives noticeably
/// smoother motion than `adb-input`'s ~30–80 ms per step.
const MOVE_STEP_MS: u32 = 16;

pub struct SendeventBackend {
    adb: Adb,
    /// Device evdev path discovered at probe time, e.g.
    /// `/dev/input/event2`. Used verbatim in every `sendevent` command.
    event_path: String,
    /// Display resolution (pixels) — the coordinate space the trait
    /// hands us coords in.
    screen_w: u32,
    screen_h: u32,
    /// Touchscreen digitizer's `ABS_MT_POSITION_X/Y` range. On most
    /// modern devices this matches the display resolution, but some
    /// digitizers report a wholly different range (e.g. 0..4095) so
    /// we always scale rather than passing pixel coords through.
    range_x: (i32, i32),
    range_y: (i32, i32),
    /// Per-gesture incrementing tracking ID. The kernel uses these to
    /// distinguish "same finger continuing" from "new finger", so we
    /// bump on every DOWN to be safe.
    next_tracking_id: i32,
}

impl SendeventBackend {
    /// Detect the touchscreen evdev device + its coordinate ranges
    /// via `getevent -pl`. Returns an error (which the factory turns
    /// into an auto-fallback) if no device with `ABS_MT_POSITION_X/Y`
    /// is found — this almost always means the device runs Type A
    /// multi-touch or a non-evdev touch driver, both of which need a
    /// different code path.
    pub fn start(adb: Adb, screen: (u32, u32)) -> Result<Self, String> {
        let dump = adb
            .shell_capture("getevent -pl")
            .map_err(|e| format!("failed to run `getevent -pl`: {e}"))?;
        let probe = parse_touchscreen_from_getevent(&dump)
            .ok_or_else(|| "no touchscreen device with ABS_MT_POSITION_X/Y found".to_string())?;
        eprintln!(
            "[sendevent] using {} ({}×{} digitizer)",
            probe.path,
            probe.range_x.1 - probe.range_x.0,
            probe.range_y.1 - probe.range_y.0,
        );
        Ok(Self {
            adb,
            event_path: probe.path,
            screen_w: screen.0,
            screen_h: screen.1,
            range_x: probe.range_x,
            range_y: probe.range_y,
            next_tracking_id: 1,
        })
    }

    fn allocate_tracking_id(&mut self) -> i32 {
        let id = self.next_tracking_id;
        // Wrap before hitting i32::MAX so we don't overflow on a
        // long-running session; the kernel only requires uniqueness
        // within a touch session, not globally.
        self.next_tracking_id = if id >= i32::MAX - 1 { 1 } else { id + 1 };
        id
    }

    fn to_evdev(&self, x: u32, y: u32) -> (i32, i32) {
        pixel_to_evdev(x, y, (self.screen_w, self.screen_h), self.range_x, self.range_y)
    }

    fn dispatch(&self, script: &str) -> Result<(), String> {
        self.adb.shell_run(script)
    }
}

impl TouchBackend for SendeventBackend {
    fn tap(&mut self, x: u32, y: u32) -> Result<(), String> {
        let (ex, ey) = self.to_evdev(x, y);
        let tid = self.allocate_tracking_id();
        let script = build_tap_script(&self.event_path, ex, ey, tid, TAP_HOLD_MS);
        self.dispatch(&script)
    }

    fn swipe(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        duration_ms: u32,
    ) -> Result<(), String> {
        let from_e = self.to_evdev(from.0, from.1);
        let to_e = self.to_evdev(to.0, to.1);
        let tid = self.allocate_tracking_id();
        let script = build_swipe_script(
            &self.event_path,
            from_e,
            to_e,
            tid,
            duration_ms,
            /* settle_ms */ 0,
        );
        self.dispatch(&script)
    }

    fn swipe_with_settle(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        swipe_ms: u32,
        settle_ms: u32,
    ) -> Result<(), String> {
        let from_e = self.to_evdev(from.0, from.1);
        let to_e = self.to_evdev(to.0, to.1);
        let tid = self.allocate_tracking_id();
        let script = build_swipe_script(
            &self.event_path,
            from_e,
            to_e,
            tid,
            swipe_ms,
            settle_ms,
        );
        self.dispatch(&script)
    }

    fn name(&self) -> &'static str {
        "sendevent"
    }
}

// ---- pure helpers ------------------------------------------------

/// Result of probing `getevent -pl` output: which evdev device to talk
/// to and the coord ranges it advertises.
#[derive(Debug, PartialEq, Eq)]
struct TouchscreenProbe {
    path: String,
    range_x: (i32, i32),
    range_y: (i32, i32),
}

/// Parse `getevent -pl` output, returning the first device that
/// advertises both `ABS_MT_POSITION_X` and `ABS_MT_POSITION_Y` and
/// `ABS_MT_TRACKING_ID`. The last requirement filters out Type A
/// devices and non-multitouch oddities (volume keys, sensors, etc.).
///
/// `getevent -pl` output is organized as `add device N: <path>`
/// blocks containing nested `ABS (0003): <name> : value V, min N,
/// max M, fuzz F, flat L, resolution R` lines. We only need the min
/// and max from the POSITION_X/Y lines.
fn parse_touchscreen_from_getevent(dump: &str) -> Option<TouchscreenProbe> {
    let mut current_path: Option<String> = None;
    let mut current_x_range: Option<(i32, i32)> = None;
    let mut current_y_range: Option<(i32, i32)> = None;
    let mut current_has_tracking_id = false;

    let flush = |path: &mut Option<String>,
                 x: &mut Option<(i32, i32)>,
                 y: &mut Option<(i32, i32)>,
                 has_tid: &mut bool|
     -> Option<TouchscreenProbe> {
        let result = if let (Some(p), Some(rx), Some(ry), true) =
            (path.as_ref(), x.as_ref(), y.as_ref(), *has_tid)
        {
            Some(TouchscreenProbe {
                path: p.clone(),
                range_x: *rx,
                range_y: *ry,
            })
        } else {
            None
        };
        *path = None;
        *x = None;
        *y = None;
        *has_tid = false;
        result
    };

    for line in dump.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("add device") {
            // New device block — try to finalize the previous one
            // (the touchscreen could legitimately be device 0 with no
            // earlier devices, but more likely it's not the first).
            if let Some(probe) =
                flush(&mut current_path, &mut current_x_range, &mut current_y_range, &mut current_has_tracking_id)
            {
                return Some(probe);
            }
            // `add device N: /dev/input/eventN` — path is after the ':'.
            if let Some(idx) = rest.find(':') {
                current_path = Some(rest[idx + 1..].trim().to_string());
            }
            continue;
        }

        // Match lines like:
        //   ABS_MT_POSITION_X     : value 0, min 0, max 1079, ...
        if let Some(range) = parse_abs_range_line(trimmed, "ABS_MT_POSITION_X") {
            current_x_range = Some(range);
        } else if let Some(range) = parse_abs_range_line(trimmed, "ABS_MT_POSITION_Y") {
            current_y_range = Some(range);
        } else if trimmed.contains("ABS_MT_TRACKING_ID") {
            current_has_tracking_id = true;
        }
    }
    // EOF — finalize the last device if it's the match.
    flush(&mut current_path, &mut current_x_range, &mut current_y_range, &mut current_has_tracking_id)
}

/// Extract `(min, max)` from a `getevent -pl` ABS line, e.g.:
/// `ABS_MT_POSITION_X     : value 0, min 0, max 1079, fuzz 0, ...`.
/// Returns `None` if `line` is for a different code or doesn't
/// contain both `min` and `max`.
fn parse_abs_range_line(line: &str, code_name: &str) -> Option<(i32, i32)> {
    // The code name appears before the `:` separator; everything
    // after is the comma-separated value/min/max/... attribute list.
    let (before_colon, after_colon) = line.split_once(':')?;
    if !before_colon.trim_end().ends_with(code_name) {
        return None;
    }
    let mut min = None;
    let mut max = None;
    for attr in after_colon.split(',') {
        let mut parts = attr.trim().split_whitespace();
        match parts.next() {
            Some("min") => {
                min = parts.next().and_then(|v| v.parse::<i32>().ok());
            }
            Some("max") => {
                max = parts.next().and_then(|v| v.parse::<i32>().ok());
            }
            _ => {}
        }
    }
    match (min, max) {
        (Some(lo), Some(hi)) => Some((lo, hi)),
        _ => None,
    }
}

/// Convert a display-pixel coordinate into the touchscreen digitizer's
/// `ABS_MT_POSITION_X/Y` range. Mirrors `minitouch::pixel_to_minitouch`
/// but the target range can have a non-zero min (rare, but valid per
/// the evdev contract).
fn pixel_to_evdev(
    px: u32,
    py: u32,
    screen: (u32, u32),
    range_x: (i32, i32),
    range_y: (i32, i32),
) -> (i32, i32) {
    let nx = if screen.0 == 0 { 0.0 } else { px as f64 / screen.0 as f64 };
    let ny = if screen.1 == 0 { 0.0 } else { py as f64 / screen.1 as f64 };
    let span_x = (range_x.1 - range_x.0).max(0) as f64;
    let span_y = (range_y.1 - range_y.0).max(0) as f64;
    let ex = range_x.0 + (nx.clamp(0.0, 1.0) * span_x).round() as i32;
    let ey = range_y.0 + (ny.clamp(0.0, 1.0) * span_y).round() as i32;
    (ex, ey)
}

/// Append a single `sendevent <path> <type> <code> <value>` command
/// to `out`, prefixed with `; ` so several can chain in one shell.
fn write_event(out: &mut String, path: &str, ty: u16, code: u16, value: i32) {
    if !out.is_empty() {
        out.push_str("; ");
    }
    let _ = write!(out, "sendevent {path} {ty} {code} {value}");
}

/// Append the "commit this frame" SYN_REPORT event.
fn write_sync(out: &mut String, path: &str) {
    write_event(out, path, EV_SYN, SYN_REPORT, 0);
}

/// Append a shell `sleep <s>` with millisecond precision. toybox
/// `sleep` (used by Android since 6.0) supports fractional seconds.
fn write_sleep_ms(out: &mut String, ms: u32) {
    if ms == 0 {
        return;
    }
    if !out.is_empty() {
        out.push_str("; ");
    }
    let _ = write!(out, "sleep {:.3}", ms as f64 / 1000.0);
}

/// Pure script builder: DOWN-at-(x,y), hold for `hold_ms`, UP. Slot 0
/// + a fresh tracking id; the kernel sees this as a single finger
/// touching and lifting.
fn build_tap_script(path: &str, x: i32, y: i32, tracking_id: i32, hold_ms: u32) -> String {
    let mut s = String::new();
    write_event(&mut s, path, EV_ABS, ABS_MT_SLOT, 0);
    write_event(&mut s, path, EV_ABS, ABS_MT_TRACKING_ID, tracking_id);
    write_event(&mut s, path, EV_ABS, ABS_MT_POSITION_X, x);
    write_event(&mut s, path, EV_ABS, ABS_MT_POSITION_Y, y);
    write_sync(&mut s, path);
    write_sleep_ms(&mut s, hold_ms);
    write_event(&mut s, path, EV_ABS, ABS_MT_TRACKING_ID, TRACKING_ID_RELEASE);
    write_sync(&mut s, path);
    s
}

/// Pure script builder: smooth swipe from `from` to `to` with
/// optional fling-suppressing `settle_ms` hold at the destination
/// before lift-off. The motion is split into `pick_step_count(swipe_ms)`
/// frames, each terminated by a SYN_REPORT so the kernel-side scroll
/// handler observes continuous motion rather than a single jump.
fn build_swipe_script(
    path: &str,
    from: (i32, i32),
    to: (i32, i32),
    tracking_id: i32,
    swipe_ms: u32,
    settle_ms: u32,
) -> String {
    let steps = pick_step_count(swipe_ms);
    let step_ms = (swipe_ms / steps).max(1);

    let mut s = String::new();
    // DOWN at `from`.
    write_event(&mut s, path, EV_ABS, ABS_MT_SLOT, 0);
    write_event(&mut s, path, EV_ABS, ABS_MT_TRACKING_ID, tracking_id);
    write_event(&mut s, path, EV_ABS, ABS_MT_POSITION_X, from.0);
    write_event(&mut s, path, EV_ABS, ABS_MT_POSITION_Y, from.1);
    write_sync(&mut s, path);

    // Interpolated MOVE frames.
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let x = (from.0 as f64 + (to.0 as f64 - from.0 as f64) * t).round() as i32;
        let y = (from.1 as f64 + (to.1 as f64 - from.1 as f64) * t).round() as i32;
        write_sleep_ms(&mut s, step_ms);
        write_event(&mut s, path, EV_ABS, ABS_MT_POSITION_X, x);
        write_event(&mut s, path, EV_ABS, ABS_MT_POSITION_Y, y);
        write_sync(&mut s, path);
    }

    if settle_ms > 0 {
        // Settle: zero-motion window before UP so the velocity
        // tracker reads ~0 px/s at lift-off and skips fling. The
        // redundant POSITION_X/Y at `to` after the wait is a belt-
        // and-suspenders zero-velocity sample.
        write_sleep_ms(&mut s, settle_ms);
        write_event(&mut s, path, EV_ABS, ABS_MT_POSITION_X, to.0);
        write_event(&mut s, path, EV_ABS, ABS_MT_POSITION_Y, to.1);
        write_sync(&mut s, path);
    }

    // UP: TRACKING_ID = -1 releases slot 0.
    write_event(&mut s, path, EV_ABS, ABS_MT_TRACKING_ID, TRACKING_ID_RELEASE);
    write_sync(&mut s, path);
    s
}

/// Number of interpolated MOVE frames for an active swipe phase. Same
/// math as `minitouch::pick_step_count`: ~60 fps cadence with a 120-
/// frame cap so pathologically long swipes don't generate megabyte
/// scripts.
fn pick_step_count(swipe_ms: u32) -> u32 {
    let raw = (swipe_ms / MOVE_STEP_MS).max(1);
    raw.min(120)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal `getevent -pl` fixture: a single Type B touchscreen on
    /// `/dev/input/event2`. Trimmed to the lines our parser actually
    /// inspects, but the format matches real Android output verbatim.
    const FIXTURE_GETEVENT_TYPE_B: &str = "\
add device 1: /dev/input/event0
  name:     \"gpio-keys\"
  events:
    KEY (0001): KEY_POWER             KEY_VOLUMEUP
  input props:
    <none>
add device 2: /dev/input/event2
  name:     \"goodix_ts\"
  events:
    ABS (0003): ABS_MT_SLOT           : value 0, min 0, max 9, fuzz 0, flat 0, resolution 0
                ABS_MT_TOUCH_MAJOR    : value 0, min 0, max 255, fuzz 0, flat 0, resolution 0
                ABS_MT_POSITION_X     : value 0, min 0, max 1079, fuzz 0, flat 0, resolution 12
                ABS_MT_POSITION_Y     : value 0, min 0, max 2399, fuzz 0, flat 0, resolution 12
                ABS_MT_TRACKING_ID    : value 0, min 0, max 65535, fuzz 0, flat 0, resolution 0
  input props:
    INPUT_PROP_DIRECT
";

    /// Fixture without `ABS_MT_TRACKING_ID` — Type A or a non-touch
    /// device. Parser should skip it and (in this single-device dump)
    /// return None.
    const FIXTURE_GETEVENT_TYPE_A_ONLY: &str = "\
add device 1: /dev/input/event3
  name:     \"legacy_resistive_ts\"
  events:
    ABS (0003): ABS_MT_POSITION_X     : value 0, min 0, max 4095, fuzz 0, flat 0, resolution 0
                ABS_MT_POSITION_Y     : value 0, min 0, max 4095, fuzz 0, flat 0, resolution 0
  input props:
    INPUT_PROP_DIRECT
";

    #[test]
    fn getevent_parser_finds_type_b_touchscreen() {
        let probe = parse_touchscreen_from_getevent(FIXTURE_GETEVENT_TYPE_B).unwrap();
        assert_eq!(probe.path, "/dev/input/event2");
        assert_eq!(probe.range_x, (0, 1079));
        assert_eq!(probe.range_y, (0, 2399));
    }

    #[test]
    fn getevent_parser_skips_non_touch_devices() {
        // event0 is gpio-keys (no MT axes); the parser must not pick
        // it just because it appears earlier in the dump.
        let probe = parse_touchscreen_from_getevent(FIXTURE_GETEVENT_TYPE_B).unwrap();
        assert_ne!(probe.path, "/dev/input/event0");
    }

    #[test]
    fn getevent_parser_skips_type_a_devices() {
        // Type A (no TRACKING_ID) → not supported by this backend.
        let probe = parse_touchscreen_from_getevent(FIXTURE_GETEVENT_TYPE_A_ONLY);
        assert!(probe.is_none(), "Type A device should be skipped, got: {probe:?}");
    }

    #[test]
    fn getevent_parser_returns_none_for_empty_dump() {
        assert!(parse_touchscreen_from_getevent("").is_none());
        assert!(parse_touchscreen_from_getevent("no devices found").is_none());
    }

    #[test]
    fn parse_abs_range_extracts_min_max_from_real_line() {
        let line = "ABS_MT_POSITION_X     : value 0, min 0, max 1079, fuzz 0, flat 0, resolution 12";
        assert_eq!(parse_abs_range_line(line, "ABS_MT_POSITION_X"), Some((0, 1079)));
    }

    #[test]
    fn parse_abs_range_returns_none_for_other_code() {
        // The line is well-formed but for a different code — the
        // parser must not return its ranges as if they were for the
        // requested code.
        let line = "ABS_MT_SLOT           : value 0, min 0, max 9, fuzz 0, flat 0, resolution 0";
        assert_eq!(parse_abs_range_line(line, "ABS_MT_POSITION_X"), None);
    }

    #[test]
    fn parse_abs_range_returns_none_for_missing_attrs() {
        let line = "ABS_MT_POSITION_X     : value 0, min 0";
        assert_eq!(parse_abs_range_line(line, "ABS_MT_POSITION_X"), None);
    }

    #[test]
    fn pixel_to_evdev_scales_proportionally() {
        // 1080×1920 display, 1080×2399 digitizer (typical real device
        // where digitizer range ≈ display y resolution).
        let (ex, ey) = pixel_to_evdev(540, 1500, (1080, 1920), (0, 1079), (0, 2399));
        // 540/1080 = 0.5 → 0.5*1079 = 539.5 → 540 (round half to even
        // gives 540 since 540 is even)
        assert_eq!(ex, 540);
        // 1500/1920 = 0.78125 → 0.78125*2399 = 1874.21875 → 1874
        assert_eq!(ey, 1874);
    }

    #[test]
    fn pixel_to_evdev_respects_nonzero_min() {
        // Some digitizers have a non-zero min (rare, but valid).
        // Pixel (0, 0) should map to (min_x, min_y).
        let (ex, ey) = pixel_to_evdev(0, 0, (1080, 1920), (100, 1179), (200, 2599));
        assert_eq!((ex, ey), (100, 200));
    }

    #[test]
    fn pixel_to_evdev_clamps_to_max_range() {
        // Off-screen pixel (jitter overflow on a small phone) must
        // not wrap past the digitizer max.
        let (ex, ey) = pixel_to_evdev(9999, 9999, (1080, 1920), (0, 1079), (0, 2399));
        assert_eq!((ex, ey), (1079, 2399));
    }

    #[test]
    fn tap_script_emits_protocol_correct_sequence() {
        let s = build_tap_script("/dev/input/event2", 540, 1500, 42, 30);
        // Expected sequence:
        //   ABS_MT_SLOT 0; ABS_MT_TRACKING_ID 42;
        //   ABS_MT_POSITION_X 540; ABS_MT_POSITION_Y 1500;
        //   SYN_REPORT;
        //   sleep 0.030;
        //   ABS_MT_TRACKING_ID -1;
        //   SYN_REPORT
        let parts: Vec<&str> = s.split("; ").collect();
        assert_eq!(parts[0], "sendevent /dev/input/event2 3 47 0");
        assert_eq!(parts[1], "sendevent /dev/input/event2 3 57 42");
        assert_eq!(parts[2], "sendevent /dev/input/event2 3 53 540");
        assert_eq!(parts[3], "sendevent /dev/input/event2 3 54 1500");
        assert_eq!(parts[4], "sendevent /dev/input/event2 0 0 0");
        assert_eq!(parts[5], "sleep 0.030");
        assert_eq!(parts[6], "sendevent /dev/input/event2 3 57 -1");
        assert_eq!(parts[7], "sendevent /dev/input/event2 0 0 0");
        assert_eq!(parts.len(), 8, "unexpected extra commands: {s}");
    }

    #[test]
    fn swipe_script_brackets_motion_with_down_and_up_releases() {
        let s = build_swipe_script("/dev/input/event2", (100, 200), (900, 800), 7, 400, 0);
        // Must START with: SLOT 0, TRACKING_ID 7, POSITION_X 100,
        // POSITION_Y 200, SYN.
        assert!(
            s.starts_with(
                "sendevent /dev/input/event2 3 47 0; \
                 sendevent /dev/input/event2 3 57 7; \
                 sendevent /dev/input/event2 3 53 100; \
                 sendevent /dev/input/event2 3 54 200; \
                 sendevent /dev/input/event2 0 0 0; "
            ),
            "got: {s}"
        );
        // Must END with: TRACKING_ID -1, SYN.
        assert!(
            s.ends_with(
                "sendevent /dev/input/event2 3 57 -1; \
                 sendevent /dev/input/event2 0 0 0"
            ),
            "got: {s}"
        );
    }

    #[test]
    fn swipe_script_emits_settle_frame_before_up() {
        // settle_ms > 0 → sleep + redundant POSITION at `to` + SYN
        // before the release sequence. Without it, the script ends
        // at the last interpolation frame.
        let s = build_swipe_script("/dev/input/event2", (100, 200), (900, 800), 7, 400, 300);
        // The settle phase appears between the last interpolated
        // frame and the release. Look for the trailing `sleep 0.300;
        // POSITION_X 900; POSITION_Y 800; SYN; TRACKING_ID -1; SYN`.
        assert!(
            s.contains(
                "sleep 0.300; \
                 sendevent /dev/input/event2 3 53 900; \
                 sendevent /dev/input/event2 3 54 800; \
                 sendevent /dev/input/event2 0 0 0; \
                 sendevent /dev/input/event2 3 57 -1; \
                 sendevent /dev/input/event2 0 0 0"
            ),
            "got: {s}"
        );
    }

    #[test]
    fn swipe_script_step_count_scales_with_duration() {
        // 16ms cadence → 60 frames at 960ms, 25 at 400ms, capped.
        assert_eq!(pick_step_count(960), 60);
        assert_eq!(pick_step_count(400), 25);
        assert_eq!(pick_step_count(60_000), 120);
        assert_eq!(pick_step_count(0), 1);
    }

    #[test]
    fn swipe_script_interpolates_each_frame() {
        // 6 frames from (100, 200) → (700, 800): the i-th MOVE
        // POSITION_X should be 100 + (700-100)*(i/6) = 100 + i*100.
        let s = build_swipe_script("/dev/input/event2", (100, 200), (700, 800), 1, 6 * 16, 0);
        for (i, expected_x) in [(1u32, 200i32), (2, 300), (3, 400), (4, 500), (5, 600), (6, 700)] {
            assert!(
                s.contains(&format!("sendevent /dev/input/event2 3 53 {expected_x}")),
                "missing MOVE x={expected_x} for frame {i}: {s}"
            );
        }
    }

    #[test]
    fn swipe_script_uses_sleep_between_frames_proportional_to_step_ms() {
        // 6 frames * 16ms → 6 `sleep 0.016` commands (one before
        // each interpolated frame).
        let s = build_swipe_script("/dev/input/event2", (100, 200), (700, 800), 1, 96, 0);
        let sleep_count = s.matches("sleep 0.016").count();
        assert_eq!(sleep_count, 6);
    }
}
