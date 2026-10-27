use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[derive(Debug, Default)]
struct State {
    cancelled: bool,
    device: Option<String>,
}

#[derive(Debug, Default)]
struct Signal {
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct OperationCancellation(Arc<Signal>);

impl OperationCancellation {
    pub(crate) fn cancel(&self) {
        self.0.state.lock().unwrap().cancelled = true;
        self.0.changed.notify_all();
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.state.lock().unwrap().cancelled
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("操作已停止".into())
        } else {
            Ok(())
        }
    }

    pub(crate) fn bind_device(&self, serial: &str) -> Result<(), String> {
        let mut state = self.0.state.lock().unwrap();
        if state.cancelled {
            return Err("操作已停止".into());
        }
        if state
            .device
            .as_deref()
            .is_some_and(|previous| previous != serial)
        {
            return Err("操作设备发生变化，已取消后续操作".into());
        }
        state.device = Some(serial.into());
        Ok(())
    }

    pub(crate) fn device(&self) -> Option<String> {
        self.0.state.lock().unwrap().device.clone()
    }

    /// Interrupt polling waits immediately, but leave atomic device commands to finish safely.
    pub(crate) fn wait(&self, duration: Duration) {
        let state = self.0.state.lock().unwrap();
        let _ = self
            .0
            .changed
            .wait_timeout_while(state, duration, |state| !state.cancelled)
            .unwrap();
    }

    pub(super) fn same_operation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
