use super::*;
use crate::device_operation::DeviceOperationCoordinator;

fn operation(kind: DeviceOperation) -> (DeviceOperationCoordinator, CaptureOperation) {
    let coordinator = DeviceOperationCoordinator::default();
    let lease = coordinator.reserve(kind).unwrap();
    let responses = ResponseStore::default();
    responses.mark_stopped("保留上次状态");
    let operation = CaptureOperation {
        cancellation: lease.cancellation(),
        _lease: lease,
        server: Server::Jp,
        selected: Some("selected-device".into()),
        targets: CaptureTargetStore::default(),
        adb_path: PathBuf::from("unused-adb"),
        responses,
    };
    (coordinator, operation)
}

fn status(operation: &CaptureOperation) -> serde_json::Value {
    // These cases never enter Waiting, so polling cannot access a device.
    serde_json::to_value(operation.responses.poll(&operation.adb_path).status()).unwrap()
}

#[test]
fn preparation_binds_one_target_and_holds_ownership_until_the_worker_finishes() {
    for kind in [
        DeviceOperation::InstallCaptureCertificate,
        DeviceOperation::ConfigurePacketCapture,
        DeviceOperation::StartPacketCapture,
    ] {
        let (coordinator, operation) = operation(kind);
        let target = operation
            .prepare_using(|| {
                Ok((
                    "selected-device".into(),
                    "com.aniplex.fategrandorder".into(),
                ))
            })
            .unwrap();
        assert_eq!(target.server, Server::Jp);
        assert_eq!(target.serial, "selected-device");
        assert_eq!(target.package, "com.aniplex.fategrandorder");
        assert_eq!(
            operation.cancellation.device().as_deref(),
            Some("selected-device")
        );
        assert_eq!(status(&operation)["message"], "");
        assert!(coordinator.require_idle().is_err());
        drop(target);
        assert!(coordinator.require_idle().is_err());
        drop(operation);
        assert!(coordinator.require_idle().is_ok());
    }
}

#[test]
fn failed_resolution_preserves_the_previous_response_and_releases_ownership_on_exit() {
    let (coordinator, operation) = operation(DeviceOperation::StartPacketCapture);
    let before = status(&operation);
    let error = operation
        .prepare_using(|| Err("客户端已变更".into()))
        .unwrap_err();
    assert_eq!(error, "客户端已变更");
    assert_eq!(status(&operation), before);
    assert!(operation.cancellation.device().is_none());
    drop(operation);
    assert!(coordinator.require_idle().is_ok());
}

#[test]
fn cancellation_before_or_during_resolution_does_not_clear_the_previous_response() {
    for cancel_during_resolution in [false, true] {
        let (_, operation) = operation(DeviceOperation::ConfigurePacketCapture);
        let before = status(&operation);
        if !cancel_during_resolution {
            operation.cancellation.cancel();
        }
        let error = operation
            .prepare_using(|| {
                assert!(
                    cancel_during_resolution,
                    "cancelled work must not query the device"
                );
                operation.cancellation.cancel();
                Ok((
                    "selected-device".into(),
                    "com.aniplex.fategrandorder".into(),
                ))
            })
            .unwrap_err();
        assert_eq!(error, "操作已停止");
        assert_eq!(status(&operation), before);
        assert!(operation.cancellation.device().is_none());
    }
}

#[test]
fn an_active_capture_blocks_preparation_before_any_target_is_queried() {
    let (_, operation) = operation(DeviceOperation::ConfigurePacketCapture);
    operation
        .responses
        .begin("capturing-device".into(), Default::default());
    let error = operation
        .prepare_using(|| panic!("must preserve the active capture"))
        .unwrap_err();
    assert!(error.contains("请先停止抓包"));
    assert!(operation
        .responses
        .ensure_client_selection_allowed()
        .is_err());
    assert!(operation.cancellation.device().is_none());
}

#[test]
fn a_bound_device_cannot_be_replaced_during_preparation() {
    let (_, operation) = operation(DeviceOperation::ConfigurePacketCapture);
    operation.cancellation.bind_device("first-device").unwrap();
    let before = status(&operation);
    let error = operation
        .prepare_using(|| Ok(("second-device".into(), "com.aniplex.fategrandorder".into())))
        .unwrap_err();
    assert!(error.contains("操作设备发生变化"));
    assert_eq!(
        operation.cancellation.device().as_deref(),
        Some("first-device")
    );
    assert_eq!(status(&operation), before);
}
