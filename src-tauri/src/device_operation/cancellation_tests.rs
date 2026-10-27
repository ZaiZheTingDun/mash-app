use super::*;
use std::time::Duration;

#[test]
fn stop_cancels_capture_waits_for_cleanup_and_preserves_exclusive_ownership() {
    for operation in [
        DeviceOperation::ConfigurePacketCapture,
        DeviceOperation::StartPacketCapture,
        DeviceOperation::InstallCaptureCertificate,
    ] {
        let coordinator = DeviceOperationCoordinator::default();
        let worker = coordinator.reserve(operation).unwrap();
        let cancellation = worker.cancellation();
        cancellation.bind_device("original-device").unwrap();
        let stop_coordinator = coordinator.clone();
        let stop = std::thread::spawn(move || stop_coordinator.reserve_capture_stop().unwrap());
        cancellation.wait(Duration::from_secs(2));
        assert!(cancellation.is_cancelled());
        assert!(
            !stop.is_finished(),
            "stop must wait for the worker lease and rollback"
        );
        assert_eq!(
            coordinator
                .reserve(DeviceOperation::ResetResolution)
                .unwrap_err(),
            "抓包正在停止，请稍后重试"
        );
        assert!(coordinator.reserve_capture_stop().is_err());
        drop(worker);
        let (lease, interrupted) = stop.join().unwrap();
        let (kind, token) = interrupted.unwrap();
        assert_eq!(kind, operation);
        assert_eq!(token.device().as_deref(), Some("original-device"));
        assert_eq!(
            coordinator.active(),
            Some(DeviceOperation::StopPacketCapture)
        );
        assert!(coordinator.require_idle().is_err());
        drop(lease);
        let next = coordinator
            .reserve(DeviceOperation::StartPacketCapture)
            .unwrap();
        assert!(!next.cancellation().is_cancelled());
        assert!(next.cancellation().device().is_none());
    }
}

#[test]
fn capture_stop_never_cancels_unrelated_automation_and_device_binding_cannot_switch() {
    let coordinator = DeviceOperationCoordinator::default();
    let worker = coordinator
        .reserve(DeviceOperation::Automation(AutomationKind::Battle))
        .unwrap();
    let cancellation = worker.cancellation();
    assert!(coordinator
        .reserve_capture_stop()
        .unwrap_err()
        .contains("战斗自动化"));
    assert!(!cancellation.is_cancelled());
    cancellation.bind_device("first").unwrap();
    assert!(cancellation.bind_device("other").is_err());
    assert_eq!(cancellation.device().as_deref(), Some("first"));
    drop(worker);
    let (stop, interrupted) = coordinator.reserve_capture_stop().unwrap();
    assert!(interrupted.is_none());
    assert!(coordinator.reserve_capture_stop().is_err());
    drop(stop);
    assert!(coordinator.require_idle().is_ok());
}
