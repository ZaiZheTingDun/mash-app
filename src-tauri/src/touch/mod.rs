//! Pluggable touch-injection backends.
//!
//! Different injection mechanisms have very different latency and
//! smoothness characteristics:
//!
//! | Backend         | Per-event latency | Smoothness    | Setup cost      | Requires       |
//! |-----------------|-------------------|---------------|-----------------|----------------|
//! | `adb-input`     | 30–80 ms          | choppy        | none            | API 1+         |
//! | `minitouch`     | <1 ms             | finger-smooth | binary push     | native binary  |
//! | `sendevent`     | <1 ms             | finger-smooth | event-dev probe | known evdev    |
//!
//! [`TouchBackend`] is the unified interface so the `Runner` can stay
//! ignorant of which one is in use. Selection happens at runner
//! construction time via [`TouchBackendKind::from_env`] (env var
//! `MASH_TOUCH_BACKEND`), with auto-fallback from minitouch to
//! adb-input when bring-up fails — so a missing native binary or
//! permission denial is non-fatal and the runner just runs slower.
//!
//! All coordinates passed to the trait are **device pixels** (the same
//! space `Point::to_physical(screen_w, screen_h)` produces). Backends
//! that internally use a different range (e.g. minitouch's banner
//! coords) are responsible for scaling.

use std::path::Path;

use crate::adb::Adb;

pub mod adb_input;
pub mod minitouch;
pub mod sendevent;

/// Unified touch-injection contract. See module docs for the
/// motivation; implementors live in this module's submodules.
pub trait TouchBackend: Send {
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
    fn swipe(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        duration_ms: u32,
    ) -> Result<(), String>;

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

/// Which backend the runner should construct. Defaults to `Auto`,
/// which tries minitouch first and falls back to adb-input — the same
/// behavior we shipped before the abstraction landed, just expressed
/// as an explicit choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchBackendKind {
    /// Try `Minitouch` → fall back to `AdbInput` on bring-up failure.
    Auto,
    /// `Adb::tap` / `Adb::swipe` / `Adb::swipe_with_settle`. Always
    /// available; slow and choppy on big swipes.
    AdbInput,
    /// Native minitouch server pushed to the device. Sub-ms event
    /// latency. Requires a bundled native binary at
    /// `<resources>/minitouch/<abi>/minitouch`.
    Minitouch,
    /// Raw evdev events via `adb shell sendevent`. No binary push
    /// needed but requires probing the device's input event paths.
    /// Placeholder implementation today; the goal is for an operator
    /// to compare it against `Minitouch` for raw smoothness.
    Sendevent,
}

impl Default for TouchBackendKind {
    fn default() -> Self {
        Self::Auto
    }
}

impl TouchBackendKind {
    /// Read the `MASH_TOUCH_BACKEND` env var at process startup.
    /// Unknown values fall back to `Auto` with a stderr warning so
    /// typos don't silently degrade input quality.
    pub fn from_env() -> Self {
        match std::env::var("MASH_TOUCH_BACKEND")
            .ok()
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            None | Some("") | Some("auto") => Self::Auto,
            Some("adb") | Some("adb-input") | Some("input") => Self::AdbInput,
            Some("minitouch") | Some("mt") => Self::Minitouch,
            Some("sendevent") => Self::Sendevent,
            Some(other) => {
                eprintln!(
                    "[touch] MASH_TOUCH_BACKEND={other:?} not recognized — using auto fallback"
                );
                Self::Auto
            }
        }
    }
}

/// Resolve which concrete backend `kind` maps to and bring it up.
///
/// The factory always returns *some* backend even when the requested
/// one fails — `Auto` and `Minitouch` both fall back to `AdbInput` on
/// bring-up errors, with a stderr warning. The only way this returns
/// `Err` is if `AdbInput` itself fails to construct, which currently
/// only happens for impossible `Adb` configurations.
pub fn build(
    kind: TouchBackendKind,
    adb: &Adb,
    resources_dir: &Path,
    screen: (u32, u32),
) -> Box<dyn TouchBackend> {
    match kind {
        TouchBackendKind::AdbInput => Box::new(adb_input::AdbInputBackend::new(adb.clone())),
        TouchBackendKind::Minitouch => {
            try_minitouch_or_adb(adb, resources_dir, screen, /* warn_on_fallback */ true)
        }
        TouchBackendKind::Sendevent => match sendevent::SendeventBackend::start(adb.clone(), screen) {
            Ok(be) => Box::new(be),
            Err(e) => {
                eprintln!("[touch] sendevent bring-up failed ({e}), falling back to adb-input");
                Box::new(adb_input::AdbInputBackend::new(adb.clone()))
            }
        },
        TouchBackendKind::Auto => {
            // The "what we did before this abstraction existed" path:
            // minitouch first, adb-input on failure. Warning is
            // suppressed here because `Auto` explicitly opts in to
            // silent fallback — chatty logs on every dev-mode startup
            // get tuned out.
            try_minitouch_or_adb(adb, resources_dir, screen, /* warn_on_fallback */ false)
        }
    }
}

fn try_minitouch_or_adb(
    adb: &Adb,
    resources_dir: &Path,
    screen: (u32, u32),
    warn_on_fallback: bool,
) -> Box<dyn TouchBackend> {
    match minitouch::MinitouchBackend::start(adb.clone(), resources_dir, screen) {
        Ok(be) => Box::new(be),
        Err(e) => {
            if warn_on_fallback {
                eprintln!("[touch] minitouch bring-up failed ({e}), falling back to adb-input");
            } else {
                eprintln!("[touch] auto: minitouch unavailable ({e}); using adb-input");
            }
            Box::new(adb_input::AdbInputBackend::new(adb.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_backend_kind_parses_known_aliases() {
        // SAFETY: tests run single-threaded in this crate (cargo's
        // default), so env-var mutation is OK; if we ever turn on
        // `--test-threads=N>1` we'll need to lock this.
        let cases = [
            (None, TouchBackendKind::Auto),
            (Some(""), TouchBackendKind::Auto),
            (Some("auto"), TouchBackendKind::Auto),
            (Some("AUTO"), TouchBackendKind::Auto),
            (Some("adb"), TouchBackendKind::AdbInput),
            (Some("adb-input"), TouchBackendKind::AdbInput),
            (Some("input"), TouchBackendKind::AdbInput),
            (Some("minitouch"), TouchBackendKind::Minitouch),
            (Some("MT"), TouchBackendKind::Minitouch),
            (Some("sendevent"), TouchBackendKind::Sendevent),
            (Some("garbage"), TouchBackendKind::Auto), // unknown → Auto
        ];
        for (input, expected) in cases {
            unsafe {
                match input {
                    Some(v) => std::env::set_var("MASH_TOUCH_BACKEND", v),
                    None => std::env::remove_var("MASH_TOUCH_BACKEND"),
                }
            }
            let parsed = TouchBackendKind::from_env();
            assert_eq!(
                parsed, expected,
                "MASH_TOUCH_BACKEND={input:?} should parse as {expected:?}, got {parsed:?}"
            );
        }
        unsafe { std::env::remove_var("MASH_TOUCH_BACKEND") }
    }
}
