//! Pluggable touch-injection backend.
//!
//! The [`TouchBackend`] trait is the abstraction the `Runner` calls
//! through, so the gesture-emitting code stays decoupled from the
//! specific transport (`scrcpy`, with `adb-input` as an initialization fallback). New backends can
//! be added by implementing the trait and wiring them into [`build`]
//! without touching the runner.
//!
//! All coordinates passed to the trait are **device pixels** (the
//! same space `Point::to_physical(screen_w, screen_h)` produces).
//! Backends that internally use a different range are responsible for
//! scaling.

use crate::adb::Adb;

const TOUCH_JITTER_PX: i32 = 6;

pub mod adb_input;
mod scrcpy;

/// Delivery model, independent of the backend's display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchTiming {
    CommandBased,
    Streamed,
}

/// Unified touch-injection contract. See module docs for the
/// motivation; implementors live in this module's submodules.
pub trait TouchBackend: Send {
    /// Unknown backends retain conservative command-based waiting policies.
    fn timing(&self) -> TouchTiming {
        TouchTiming::CommandBased
    }

    /// Single tap at device pixel `(x, y)`. Backends may emit
    /// multiple ADB / protocol commands under the hood but observers
    /// should see one DOWN-UP touch cycle.
    fn tap(&mut self, x: u32, y: u32) -> Result<(), String>;

    /// Linear swipe from `from` to `to` over `duration_ms`. May
    /// trigger Android's fling momentum at low durations / high
    /// velocity; callers who need to stop exactly at `to` should
    /// use [`swipe_with_settle`] instead.
    ///
    /// [`swipe_with_settle`]: TouchBackend::swipe_with_settle
    fn swipe(&mut self, from: (u32, u32), to: (u32, u32), duration_ms: u32) -> Result<(), String>;

    /// "Press, drag, hold, release" swipe. After completing the
    /// active motion in `swipe_ms`, the contact stays at `to` for
    /// `settle_ms` before lifting off, giving Android's velocity
    /// tracker a window of zero motion to read — which suppresses
    /// the fling response and lands the cursor exactly at `to`.
    fn swipe_with_settle(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        swipe_ms: u32,
        settle_ms: u32,
    ) -> Result<(), String>;

    /// Stable human-readable identifier for logging. Surfaces in the
    /// status-bar debug log alongside scroll diagnostics so an operator
    /// can tell which backend produced which gesture.
    fn name(&self) -> &'static str;
}

/// Prefer the existing scrcpy session. Unsupported/older sidecars use ADB.
/// `MASH_TOUCH_BACKEND=adb` allows comparison without changing persistent settings.
pub(crate) fn scrcpy_enabled(mode: crate::commands::settings::TouchControlMode) -> bool {
    scrcpy_enabled_with_override(mode, std::env::var("MASH_TOUCH_BACKEND").ok().as_deref())
}

fn scrcpy_enabled_with_override(
    mode: crate::commands::settings::TouchControlMode,
    override_value: Option<&str>,
) -> bool {
    mode == crate::commands::settings::TouchControlMode::Scrcpy
        && !matches!(override_value, Some("adb" | "adb-input"))
}

pub fn build(
    adb: &Adb,
    sidecar: &crate::screen::SidecarClient,
    app: &tauri::AppHandle,
    screen_size: (u32, u32),
) -> Box<dyn TouchBackend> {
    let inner: Box<dyn TouchBackend> = match sidecar.scrcpy_control(adb) {
        Some(control) => Box::new(scrcpy::ScrcpyTouchBackend::new(control, screen_size)),
        None => Box::new(adb_input::AdbInputBackend::new(adb.clone())),
    };
    let backend: Box<dyn TouchBackend> = Box::new(HumanizedTouch::new(inner, screen_size));
    let message = format!("[touch] backend selected: {}", backend.name());
    eprintln!("{message}");
    crate::operation_log::emit_debug(app, crate::operation_log::DebugSource::App, message);
    backend
}

/// Applies the product-wide touch policy once, at the lowest shared layer.
/// A single offset is used for both ends of a gesture so its direction and
/// distance remain stable.
struct HumanizedTouch {
    inner: Box<dyn TouchBackend>,
    screen_size: (u32, u32),
}

impl HumanizedTouch {
    fn new(inner: Box<dyn TouchBackend>, screen_size: (u32, u32)) -> Self {
        Self { inner, screen_size }
    }

    fn jittered(&self, point: (u32, u32), offset: (i32, i32)) -> (u32, u32) {
        jittered_point(point, self.screen_size, offset)
    }
}

impl TouchBackend for HumanizedTouch {
    fn timing(&self) -> TouchTiming {
        self.inner.timing()
    }

    fn tap(&mut self, x: u32, y: u32) -> Result<(), String> {
        let (x, y) = self.jittered((x, y), jitter_offset());
        self.inner.tap(x, y)
    }

    fn swipe(&mut self, from: (u32, u32), to: (u32, u32), duration_ms: u32) -> Result<(), String> {
        let offset = jitter_offset();
        self.inner.swipe(
            self.jittered(from, offset),
            self.jittered(to, offset),
            duration_ms,
        )
    }

    fn swipe_with_settle(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        swipe_ms: u32,
        settle_ms: u32,
    ) -> Result<(), String> {
        let offset = jitter_offset();
        self.inner.swipe_with_settle(
            self.jittered(from, offset),
            self.jittered(to, offset),
            swipe_ms,
            settle_ms,
        )
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
}

fn jittered_point(point: (u32, u32), screen_size: (u32, u32), offset: (i32, i32)) -> (u32, u32) {
    let max_x = screen_size.0.saturating_sub(1) as i32;
    let max_y = screen_size.1.saturating_sub(1) as i32;
    (
        (point.0 as i32 + offset.0).clamp(0, max_x) as u32,
        (point.1 as i32 + offset.1).clamp(0, max_y) as u32,
    )
}

pub(crate) fn jitter_offset() -> (i32, i32) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos())
        .unwrap_or(0);
    let span = (TOUCH_JITTER_PX * 2 + 1) as u32;
    (
        (nanos % span) as i32 - TOUCH_JITTER_PX,
        ((nanos / span) % span) as i32 - TOUCH_JITTER_PX,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_adb_choice_disables_scrcpy_with_or_without_debug_override() {
        use crate::commands::settings::TouchControlMode;
        assert!(scrcpy_enabled_with_override(TouchControlMode::Scrcpy, None));
        assert!(!scrcpy_enabled_with_override(TouchControlMode::Adb, None));
        assert!(!scrcpy_enabled_with_override(
            TouchControlMode::Adb,
            Some("scrcpy")
        ));
        assert!(!scrcpy_enabled_with_override(
            TouchControlMode::Scrcpy,
            Some("adb")
        ));
    }

    #[test]
    fn humanized_wrapper_preserves_backend_timing() {
        struct Backend(TouchTiming);
        impl TouchBackend for Backend {
            fn timing(&self) -> TouchTiming {
                self.0
            }
            fn tap(&mut self, _: u32, _: u32) -> Result<(), String> {
                Ok(())
            }
            fn swipe(&mut self, _: (u32, u32), _: (u32, u32), _: u32) -> Result<(), String> {
                Ok(())
            }
            fn swipe_with_settle(
                &mut self,
                _: (u32, u32),
                _: (u32, u32),
                _: u32,
                _: u32,
            ) -> Result<(), String> {
                Ok(())
            }
            fn name(&self) -> &'static str {
                "test"
            }
        }
        for timing in [TouchTiming::CommandBased, TouchTiming::Streamed] {
            let wrapper = HumanizedTouch::new(Box::new(Backend(timing)), (1920, 1080));
            assert_eq!(wrapper.timing(), timing);
        }
    }

    #[test]
    fn jitter_is_clamped_to_screen_edges() {
        assert_eq!(jittered_point((1, 1), (100, 50), (-6, -6)), (0, 0));
        assert_eq!(jittered_point((98, 48), (100, 50), (6, 6)), (99, 49));
    }

    #[test]
    fn same_offset_preserves_swipe_vector_away_from_edges() {
        let offset = (4, -3);
        let from = jittered_point((10, 20), (100, 100), offset);
        let to = jittered_point((60, 80), (100, 100), offset);
        assert_eq!((to.0 - from.0, to.1 - from.1), (50, 60));
    }
}
