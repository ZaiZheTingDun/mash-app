use super::*;
use std::time::Duration;

#[test]
fn stopping_setup_and_start_waits_for_the_worker_closes_its_device_and_logs_the_outcome() {
    for operation in [
        DeviceOperation::ConfigurePacketCapture,
        DeviceOperation::StartPacketCapture,
    ] {
        for failure in [false, true] {
            let coordinator = DeviceOperationCoordinator::default();
            let lease = coordinator.reserve(operation).unwrap();
            let cancellation = lease.cancellation();
            cancellation.bind_device("pinned-device").unwrap();
            let worker = std::thread::spawn(move || {
                cancellation.wait(Duration::from_secs(2));
                assert!(cancellation.is_cancelled());
                drop(lease);
            });
            let responses = ResponseStore::default();
            let mut logs = Vec::new();
            let mut stops = 0;
            let result = stop_using(
                &coordinator,
                &responses,
                &mut |serial| {
                    assert_eq!(serial, "pinned-device");
                    assert!(coordinator
                        .reserve(DeviceOperation::ResetResolution)
                        .unwrap_err()
                        .contains("抓包正在停止"));
                    stops += 1;
                    if failure {
                        Err("无法确认 Reqable 已停止".into())
                    } else {
                        Ok(())
                    }
                },
                &mut || {
                    panic!("pending jobs must not use the response store's possibly stale device")
                },
                &mut |level, message| logs.push((level, message.to_string())),
            );
            worker.join().unwrap();
            assert_eq!(stops, 1);
            assert!(coordinator.require_idle().is_ok());
            assert_eq!(logs[0].0, LogLevel::Info);
            assert!(logs[0].1.contains("停止请求"));
            if failure {
                assert!(result.is_err());
                assert_eq!(
                    responses.interrupted_stop_target().as_deref(),
                    Some("pinned-device")
                );
                assert_eq!(logs[1].0, LogLevel::Warn);
                assert!(logs[1].1.contains("无法确认 Reqable 已停止"));
            } else {
                assert!(result.is_ok());
                assert!(responses.interrupted_stop_target().is_none());
                assert_eq!(logs[1].0, LogLevel::Info);
                assert_eq!(
                    logs[1].1,
                    if operation == DeviceOperation::ConfigurePacketCapture {
                        "环境配置已停止"
                    } else {
                        "抓包已停止"
                    }
                );
            }
        }
    }
}

#[test]
fn stopping_an_active_capture_logs_success_or_failure_without_claiming_failed_stops_succeeded() {
    for failure in [false, true] {
        let mut logs = Vec::new();
        let result = stop_using(
            &DeviceOperationCoordinator::default(),
            &ResponseStore::default(),
            &mut |_| panic!(),
            &mut || {
                if failure {
                    Err("设备离线".into())
                } else {
                    Ok(())
                }
            },
            &mut |level, message| logs.push((level, message.to_string())),
        );
        assert_eq!(result.is_err(), failure);
        assert_eq!(
            logs.last().unwrap().0,
            if failure {
                LogLevel::Warn
            } else {
                LogLevel::Info
            }
        );
        assert_eq!(
            logs.last().unwrap().1,
            if failure {
                "停止抓包任务失败：设备离线"
            } else {
                "抓包已停止"
            }
        );
    }
}

#[test]
fn failed_interrupted_stop_is_retryable_on_the_original_device_without_using_the_capture_api() {
    let coordinator = DeviceOperationCoordinator::default();
    let responses = ResponseStore::default();
    responses.mark_stop_failed("original-device", "设备离线");
    let mut logs = Vec::new();
    stop_using(
        &coordinator,
        &responses,
        &mut |serial| {
            assert_eq!(serial, "original-device");
            Ok(())
        },
        &mut || panic!("a force-stopped Reqable may no longer have a running API"),
        &mut |level, message| logs.push((level, message.to_string())),
    )
    .unwrap();
    assert!(responses.interrupted_stop_target().is_none());
    assert_eq!(logs.last().unwrap().1, "抓包已停止");
}
