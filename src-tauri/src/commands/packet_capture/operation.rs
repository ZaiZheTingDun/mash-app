//! Snapshot and prepare a device target while retaining exclusive ownership.
use crate::commands::settings::AdbDeviceSettings;
use crate::device_operation::{DeviceOperation, DeviceOperationLease, OperationCancellation};
use crate::packet_capture::{CaptureClientContext, CaptureTargetStore, ResponseStore};
use crate::Server;
use std::{path::PathBuf, sync::Mutex};
use tauri::Manager;

#[derive(Debug)]
pub(super) struct CaptureTarget {
    pub(super) server: Server,
    pub(super) serial: String,
    pub(super) package: String,
}

pub(super) struct CaptureOperation {
    server: Server,
    selected: Option<String>,
    targets: CaptureTargetStore,
    pub(super) adb_path: PathBuf,
    pub(super) cancellation: OperationCancellation,
    pub(super) responses: ResponseStore,
    // Keep ownership through worker cleanup and release it after the other fields.
    _lease: DeviceOperationLease,
}

impl CaptureOperation {
    pub(super) fn reserve(
        app: &tauri::AppHandle,
        operation: DeviceOperation,
    ) -> Result<Self, String> {
        let lease = app
            .state::<crate::device_operation::DeviceOperationCoordinator>()
            .reserve(operation)?;
        let cancellation = lease.cancellation();
        let server = *app.state::<Mutex<Server>>().lock().unwrap();
        let selected = app
            .state::<Mutex<AdbDeviceSettings>>()
            .lock()
            .unwrap()
            .selected_adb_serial
            .clone();
        Ok(Self {
            _lease: lease,
            server,
            selected,
            targets: app.state::<CaptureTargetStore>().inner().clone(),
            responses: app.state::<ResponseStore>().inner().clone(),
            adb_path: crate::adb::resolve_adb_path(app),
            cancellation,
        })
    }

    pub(super) fn prepare(
        &self,
        expected: Option<&CaptureClientContext>,
    ) -> Result<CaptureTarget, String> {
        self.prepare_using(|| {
            self.targets.resolve(
                self.server,
                &self.adb_path,
                self.selected.as_deref(),
                expected,
            )
        })
    }

    fn prepare_using(
        &self,
        resolve: impl FnOnce() -> Result<(String, String), String>,
    ) -> Result<CaptureTarget, String> {
        self.cancellation.check()?;
        self.responses.ensure_client_selection_allowed()?;
        let (serial, package) = resolve()?;
        self.cancellation.bind_device(&serial)?;
        self.cancellation.check()?;
        // Failed resolution or cancellation must preserve the previous response.
        self.responses.reset();
        Ok(CaptureTarget {
            server: self.server,
            serial,
            package,
        })
    }
}

#[cfg(test)]
mod tests;
