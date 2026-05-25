//! `adb shell sendevent` backend.
//!
//! Sends raw evdev events directly to `/dev/input/event*`, the same
//! interface the kernel-level touchscreen driver exposes. Each event
//! is processed in well under a millisecond — comparable to minitouch
//! — but without needing a pushed native binary or an `adb forward`
//! channel.
//!
//! The trade-off is that evdev requires per-device knowledge:
//!
//! - The exact event device path (`/dev/input/event2` differs by
//!   device / emulator / firmware).
//! - The multi-touch protocol (Type A vs Type B; modern Android is
//!   almost always Type B).
//! - The hardware coordinate ranges (often different from display
//!   pixels and not always advertised in the same way).
//!
//! These can be discovered at runtime via `adb shell getevent -p`,
//! which dumps every input device with its name, kind, and
//! advertised axes. The first device whose name contains
//! `touch` / `Touch` and that reports `ABS_MT_POSITION_X` is
//! generally the screen digitizer.
//!
//! This stub exists so the `TouchBackend` trait has a sendevent slot
//! ready to wire in once the protocol-level implementation lands.
//! Today `start` is a no-op error that triggers the factory's
//! auto-fallback to `AdbInputBackend`, so selecting
//! `MASH_TOUCH_BACKEND=sendevent` is harmless — it just behaves like
//! `MASH_TOUCH_BACKEND=adb`.

use crate::adb::Adb;
use crate::touch::TouchBackend;

pub struct SendeventBackend {
    /// Reserved for the upcoming sendevent implementation. Kept here
    /// (with `dead_code`) so the field layout matches the eventual
    /// shape and the factory wiring already compiles.
    #[allow(dead_code)]
    adb: Adb,
}

impl SendeventBackend {
    /// Bring-up: discover the touchscreen evdev device + protocol via
    /// `getevent -p`, parse its axis ranges, stash the result here.
    ///
    /// TODO: not implemented yet. Returns an error so the factory in
    /// `touch::build` falls back to `AdbInputBackend`.
    pub fn start(_adb: Adb) -> Result<Self, String> {
        Err("sendevent backend not implemented yet".to_string())
    }
}

impl TouchBackend for SendeventBackend {
    fn tap(&mut self, _x: u32, _y: u32) -> Result<(), String> {
        Err("sendevent backend not implemented yet".to_string())
    }

    fn swipe(
        &mut self,
        _from: (u32, u32),
        _to: (u32, u32),
        _duration_ms: u32,
    ) -> Result<(), String> {
        Err("sendevent backend not implemented yet".to_string())
    }

    fn swipe_with_settle(
        &mut self,
        _from: (u32, u32),
        _to: (u32, u32),
        _swipe_ms: u32,
        _settle_ms: u32,
    ) -> Result<(), String> {
        Err("sendevent backend not implemented yet".to_string())
    }

    fn name(&self) -> &'static str {
        "sendevent"
    }
}
