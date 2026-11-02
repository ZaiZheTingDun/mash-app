use super::super::tests::encoded_shell_output;
use super::*;
use std::cell::Cell;

const LICENSE: &str = include_str!("../../../tests/fixtures/packet-capture/reqable-license.xml");
const LICENSE_ZH: &str =
    include_str!("../../../tests/fixtures/packet-capture/reqable-license-zh-cn.xml");
const MODES_ZH: &str =
    include_str!("../../../tests/fixtures/packet-capture/reqable-work-modes-zh-cn.xml");
const READY: &[u8] =
    br#"<hierarchy><node package="com.reqable.android" content-desc="Reqable" /></hierarchy>"#;

#[cfg(unix)]
#[test]
fn reads_a_fresh_dump_after_uiautomator_crashes_but_rejects_missing_or_truncated_xml() {
    for (exit, xml, expected) in [
        (0, Some(READY), Ok(UiState::Ready)),
        (139, Some(READY), Ok(UiState::Ready)),
        (
            139,
            None,
            Err("无法读取 Reqable 界面结构，请保持应用打开后重试".into()),
        ),
        (
            0,
            None,
            Err("无法读取 Reqable 界面结构，请保持应用打开后重试".into()),
        ),
        (
            139,
            Some(b"<hierarchy><node>".as_slice()),
            Err(INVALID_XML.into()),
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let result = probe(&mut |script| {
            // Execute the production shell script with an isolated fake dump
            // command: no emulator, root permission or real device is needed.
            let script = script.replace("/data/local/tmp", dir.path().to_str().unwrap());
            let output = std::process::Command::new("/bin/sh")
                .args(["-c", &format!(
                    "uiautomator() {{ if [ \"$MOCK_WRITE\" = 1 ]; then printf %s \"$MOCK_XML\" > \"$2\"; fi; return \"$MOCK_EXIT\"; }}; {script}"
                )])
                .env("MOCK_WRITE", if xml.is_some() { "1" } else { "0" })
                .env("MOCK_XML", std::str::from_utf8(xml.unwrap_or_default()).unwrap())
                .env("MOCK_EXIT", exit.to_string())
                .output()
                .unwrap();
            if output.status.success() {
                Ok(output.stdout)
            } else {
                Err("dump failed".into())
            }
        });
        assert_eq!(result, expected);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

#[test]
fn recognizes_reported_chinese_license_and_work_modes_with_unlabeled_next() {
    assert_eq!(
        inspect_ui(LICENSE_ZH.as_bytes()).unwrap(),
        UiState::Agreement(Agreement {
            accept_enabled: false
        })
    );
    assert_eq!(
        inspect_ui(
            LICENSE_ZH
                .replace("enabled=\"false\"", "enabled=\"true\"")
                .as_bytes()
        )
        .unwrap(),
        UiState::Agreement(Agreement {
            accept_enabled: true
        })
    );
    assert_eq!(
        inspect_ui(MODES_ZH.as_bytes()).unwrap(),
        inspect_ui(MODES.as_bytes()).unwrap()
    );
    for xml in [LICENSE_ZH, MODES_ZH] {
        assert_eq!(
            inspect_ui(xml.replace("com.reqable.android", "other.app").as_bytes()).unwrap(),
            UiState::OtherApp
        );
    }
}

#[test]
fn recognizes_real_flutter_content_descriptions_including_a_disabled_accept_button() {
    assert_eq!(
        inspect_ui(LICENSE.as_bytes()).unwrap(),
        UiState::Agreement(Agreement {
            accept_enabled: false
        })
    );
    assert_eq!(inspect_ui(READY).unwrap(), UiState::Ready);
    assert_eq!(
        inspect_ui(
            LICENSE
                .replace("com.reqable.android", "other.app")
                .as_bytes()
        )
        .unwrap(),
        UiState::OtherApp
    );
    assert_eq!(
        inspect_ui(
            LICENSE
                .replace(
                    "content-desc=\"License Agreement\"",
                    "content-desc=\"许可协议\""
                )
                .as_bytes()
        )
        .unwrap(),
        UiState::Agreement(Agreement {
            accept_enabled: false
        })
    );
    for xml in [
        b"not XML".as_slice(),
        b"<hierarchy><node>",
        b"<hierarchy><node></hierarchy>",
    ] {
        assert!(inspect_ui(xml).is_err());
    }
    assert!(inspect_ui(&vec![b'x'; MAX_XML + 1]).is_err());
}

#[test]
fn an_existing_certificate_does_not_bypass_the_license_and_confirmation_resumes_automatically() {
    let elapsed = Cell::new(Duration::ZERO);
    let mut probes = 0;
    let mut ca_reads = 0;
    let mut logs = Vec::new();
    let result = wait_using(
        "su -c",
        &mut |command| {
            assert!(!command.contains("input "));
            if command.contains("uiautomator dump") {
                assert!(command.contains("trap 'rm -f"));
                assert!(command.contains("head -c"));
                probes += 1;
                return Ok(match probes {
                    1 => LICENSE.as_bytes().to_vec(),
                    2 => LICENSE
                        .replace("enabled=\"false\"", "enabled=\"true\"")
                        .into_bytes(),
                    _ => READY.to_vec(),
                });
            }
            // Even a readable old CA must not be treated as agreement acceptance.
            assert!(probes >= 3);
            ca_reads += 1;
            assert!(command.starts_with("su -c"));
            Ok(encoded_shell_output(include_bytes!(
                "../../../tests/fixtures/packet-capture/ca.pem"
            )))
        },
        &mut |level, message| logs.push((level, message.to_string())),
        &mut |duration| elapsed.set(elapsed.get() + duration),
        &|| elapsed.get(),
    )
    .unwrap();
    assert_eq!(result.0, CERT_PATHS[0]);
    assert!(super::super::certificate::parse(&result.1).is_ok());
    assert_eq!(ca_reads, 1);
    assert_eq!(logs.len(), 3);
    assert!(logs[0].1.contains("Accept 按钮目前不可用"));
    assert_eq!(
        logs[1].1,
        "当前为服务条款页面，请滑动到底部并点击底部按钮接受条款"
    );
    assert!(logs[2].1.contains("继续环境配置"));
}

#[test]
fn waits_for_certificate_generation_after_the_dialog_closes_and_times_out_without_mutations() {
    for never_confirm in [false, true] {
        let elapsed = Cell::new(Duration::ZERO);
        let mut probes = 0;
        let result = wait_using(
            "",
            &mut |command| {
                assert!(!command.contains("input ") && !command.contains("MASH_CA_INSTALLED"));
                if command.contains("uiautomator dump") {
                    probes += 1;
                    return Ok(if never_confirm || probes == 1 {
                        LICENSE.as_bytes().to_vec()
                    } else {
                        READY.to_vec()
                    });
                }
                if probes < 4 {
                    return Err("not initialized".into());
                }
                Ok(encoded_shell_output(&[1]))
            },
            &mut |_, _| {},
            &mut |duration| elapsed.set(elapsed.get() + duration),
            &|| elapsed.get(),
        );
        if never_confirm {
            assert!(result.unwrap_err().contains("许可协议确认后重试"));
        } else {
            assert!(result.is_ok());
            assert!(probes >= 4);
        }
    }
}

#[test]
fn reports_dump_failures_without_logging_device_ui_and_keeps_normal_initialization_short() {
    let error = wait_using(
        "",
        &mut |_| Err("private-device-output".into()),
        &mut |_, _| panic!(),
        &mut |_| panic!(),
        &|| Duration::ZERO,
    )
    .unwrap_err();
    assert!(error.contains("无法读取 Reqable 界面结构"));
    assert!(!error.contains("private-device-output"));
    let elapsed = Cell::new(Duration::ZERO);
    let error = wait_using(
        "",
        &mut |command| {
            if command.contains("uiautomator dump") {
                Ok(READY.to_vec())
            } else {
                Err("missing".into())
            }
        },
        &mut |_, _| panic!(),
        &mut |duration| elapsed.set(elapsed.get() + duration),
        &|| elapsed.get(),
    )
    .unwrap_err();
    assert!(error.contains("未找到 Reqable CA"));
    assert!(elapsed.get() < Duration::from_secs(4));
}

#[test]
fn leaving_reqable_is_not_treated_as_confirmation_even_if_an_old_certificate_exists() {
    let elapsed = Cell::new(Duration::ZERO);
    let mut probes = 0;
    let result = wait_using(
        "",
        &mut |command| {
            assert!(
                command.contains("uiautomator dump"),
                "must not read/install certificates while another app is foreground"
            );
            probes += 1;
            Ok(if probes == 1 {
                LICENSE.as_bytes().to_vec()
            } else {
                br#"<hierarchy><node package="other.app" /></hierarchy>"#.to_vec()
            })
        },
        &mut |_, _| {},
        &mut |duration| elapsed.set(elapsed.get() + duration),
        &|| elapsed.get(),
    );
    assert!(result.unwrap_err().contains("许可协议确认后重试"));
}

const MODES: &str = include_str!("../../../tests/fixtures/packet-capture/reqable-work-modes.xml");

#[test]
fn recognizes_only_the_enabled_standalone_option_and_validates_device_bounds() {
    assert_eq!(
        inspect_ui(MODES.as_bytes()).unwrap(),
        UiState::WorkModes(WorkModes {
            standalone: TapTarget {
                left: 24,
                top: 393,
                right: 1896,
                bottom: 602
            },
            next: Some(TapTarget {
                left: 1819,
                top: 65,
                right: 1896,
                bottom: 143
            })
        })
    );
    for xml in [
        MODES.replace("Standalone Mode", "Collaborative Mode"),
        MODES.replace("enabled=\"true\"", "enabled=\"false\""),
        MODES.replace("[24,393][1896,602]", "[24,393][9999,602]"),
        MODES.replace("[24,393][1896,602]", "invalid"),
    ] {
        assert!(inspect_ui(xml.as_bytes()).is_err());
    }
    for bad in [
        "[-1,0][2,3]",
        "[4294967295,0][1,3]",
        "[1,2][0,0]",
        "[0,0][1,1]",
    ] {
        assert!(TapTarget::parse(bad).is_none());
    }
    assert_eq!(
        inspect_ui(MODES.replace("com.reqable.android", "other.app").as_bytes()).unwrap(),
        UiState::OtherApp
    );
}

#[test]
fn selects_standalone_then_next_once_using_fresh_bounds_before_reading_certificates() {
    for (license, modes) in [(LICENSE, MODES), (LICENSE_ZH, MODES_ZH)] {
        let elapsed = Cell::new(Duration::ZERO);
        let mut probes = 0;
        let mut taps = 0;
        let mut messages = Vec::new();
        let result = wait_using(
            "",
            &mut |command| {
                if command.contains("uiautomator dump") {
                    probes += 1;
                    return Ok(match probes {
                        1 => license.as_bytes().to_vec(),
                        2 => modes.as_bytes().to_vec(),
                        3 | 4 => modes
                            .replace("[1819,65][1896,143]", "[1700,65][1777,143]")
                            .into_bytes(),
                        _ => READY.to_vec(),
                    });
                }
                if command.starts_with("input tap ") {
                    taps += 1;
                    let coords: Vec<u32> = command
                        .split_whitespace()
                        .skip(2)
                        .map(|n| n.parse().unwrap())
                        .collect();
                    if taps == 1 {
                        assert_eq!(probes, 2);
                        assert!((954..=966).contains(&coords[0]));
                        assert!((491..=503).contains(&coords[1]));
                    } else {
                        assert_eq!(taps, 2);
                        assert_eq!(probes, 3);
                        assert!((1732..=1744).contains(&coords[0]));
                        assert!((98..=110).contains(&coords[1]));
                    }
                    return Ok(Vec::new());
                }
                assert!(probes >= 5, "no CA reads before leaving Work Modes");
                Ok(encoded_shell_output(&[1]))
            },
            &mut |_, message| messages.push(message.to_string()),
            &mut |duration| elapsed.set(elapsed.get() + duration),
            &|| elapsed.get(),
        )
        .unwrap();
        assert_eq!(result.0, CERT_PATHS[0]);
        assert_eq!(taps, 2);
        assert!(messages
            .iter()
            .any(|message| message == "当前为服务条款页面，请滑动到底部并点击底部按钮接受条款"));
        assert!(messages.iter().any(|message| {
            message
        == "检测到 Reqable 工作模式选择页，正在自动处理，如无响应请手动选择独立模式并进入下一步"
        }));
    }
}

#[test]
fn failed_or_unconfirmed_mode_taps_wait_for_manual_action_without_retries_or_installation() {
    for failed_tap in [Some(1), Some(2), None] {
        let elapsed = Cell::new(Duration::ZERO);
        let mut taps = 0;
        let mut warnings = Vec::new();
        let error = wait_using(
            "",
            &mut |command| {
                if command.contains("uiautomator dump") {
                    return Ok(MODES.as_bytes().to_vec());
                }
                assert!(command.starts_with("input tap "));
                taps += 1;
                if failed_tap == Some(taps) {
                    Err("transport failed".into())
                } else {
                    Ok(Vec::new())
                }
            },
            &mut |level, message| {
                if level == LogLevel::Warn {
                    warnings.push(message.to_string());
                }
            },
            &mut |duration| elapsed.set(elapsed.get() + duration),
            &|| elapsed.get(),
        )
        .unwrap_err();
        assert_eq!(taps, if failed_tap == Some(1) { 1 } else { 2 });
        assert!(error.contains("初始化超时"));
        assert_eq!(warnings.len(), usize::from(failed_tap.is_some()));
        assert!(warnings.iter().all(|message| message.contains("等待手动")));
    }
}

#[test]
fn only_a_unique_enabled_toolbar_target_can_be_used_for_next() {
    for xml in [
        MODES.replace("[1819,65][1896,143]", "[24,65][100,143]"),
        MODES.replace("[1819,65][1896,143]", "[1819,700][1896,778]"),
        MODES.replace("[1819,65][1896,143]", "[1819,65][9999,143]"),
        MODES.replace("[1819,65][1896,143]", "invalid"),
        MODES.replace("NAF=\"true\" text=\"\"", "NAF=\"true\" text=\"Other action\""),
        MODES.replace("clickable=\"true\" enabled=\"true\" bounds=\"[1819", "clickable=\"true\" enabled=\"false\" bounds=\"[1819"),
        MODES.replace("</hierarchy>", "<node package=\"com.reqable.android\" clickable=\"true\" enabled=\"true\" bounds=\"[1700,65][1777,143]\" /></hierarchy>"),
    ] {
        let UiState::WorkModes(page) = inspect_ui(xml.as_bytes()).unwrap() else { panic!() };
        assert_eq!(page.next, None);
    }
    let UiState::WorkModes(page) = inspect_ui(
        MODES
            .replace("NAF=\"true\" text=\"\"", "NAF=\"true\" text=\"Next\"")
            .as_bytes(),
    )
    .unwrap() else {
        panic!()
    };
    assert!(page.next.is_some());
}

#[test]
fn missing_next_or_failed_taps_allow_manual_completion_without_reading_ca_early() {
    for missing_next in [true, false] {
        let elapsed = Cell::new(Duration::ZERO);
        let mut probes = 0;
        let mut taps = 0;
        let result = wait_using(
            "",
            &mut |command| {
                if command.contains("uiautomator dump") {
                    probes += 1;
                    return Ok(if probes >= 4 {
                        READY.to_vec()
                    } else if missing_next {
                        MODES.replace("[1819,65][1896,143]", "invalid").into_bytes()
                    } else {
                        MODES.as_bytes().to_vec()
                    });
                }
                if command.starts_with("input tap ") {
                    taps += 1;
                    return if missing_next {
                        Ok(Vec::new())
                    } else {
                        Err("tap failed".into())
                    };
                }
                assert!(probes >= 4);
                Ok(encoded_shell_output(&[1]))
            },
            &mut |_, _| {},
            &mut |duration| elapsed.set(elapsed.get() + duration),
            &|| elapsed.get(),
        )
        .unwrap();
        assert_eq!(taps, 1);
        assert_eq!(result.0, CERT_PATHS[0]);
    }
}

#[test]
fn a_mode_timeout_after_accepting_the_license_requests_the_remaining_next_step() {
    let elapsed = Cell::new(Duration::ZERO);
    let mut probes = 0;
    let mut taps = 0;
    let error = wait_using(
        "",
        &mut |command| {
            if command.contains("uiautomator dump") {
                probes += 1;
                return Ok(if probes == 1 { LICENSE } else { MODES }
                    .as_bytes()
                    .to_vec());
            }
            assert!(command.starts_with("input tap "));
            taps += 1;
            Ok(Vec::new())
        },
        &mut |_, _| {},
        &mut |duration| elapsed.set(elapsed.get() + duration),
        &|| elapsed.get(),
    )
    .unwrap_err();
    assert_eq!(taps, 2);
    assert!(error.contains("选择独立模式并进入下一步"));
    assert!(!error.contains("许可协议确认"));
}
