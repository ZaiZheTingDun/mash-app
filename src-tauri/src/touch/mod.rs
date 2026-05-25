//! Pluggable touch-injection backend.
//!
//! The [`TouchBackend`] trait is the abstraction the `Runner` calls
//! through, so the gesture-emitting code stays decoupled from the
//! specific transport (today only `adb-input`; previously also tried
//! `minitouch` and raw `sendevent`, both retired). New backends can
//! be added by implementing the trait and wiring them into [`build`]
//! without touching the runner.
//!
//! All coordinates passed to the trait are **device pixels** (the
//! same space `Point::to_physical(screen_w, screen_h)` produces).
//! Backends that internally use a different range are responsible for
//! scaling.

use crate::adb::Adb;

pub mod adb_input;

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

/// Construct the runner's `TouchBackend`. Currently always returns
/// the `adb-input` backend — kept as a factory function so future
/// backends can be slotted in here without changing the call sites.
pub fn build(adb: &Adb) -> Box<dyn TouchBackend> {
    Box::new(adb_input::AdbInputBackend::new(adb.clone()))
}
