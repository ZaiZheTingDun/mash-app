//! Android logical display selection (MuMu keep-alive uses a separate display
//! for each app). SurfaceFlinger's physical display IDs are a different space.

use super::{adb_command, Adb};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) struct PixelSize {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DisplaySize {
    pub(crate) physical_size: PixelSize,
    pub(crate) override_size: Option<PixelSize>,
}

impl DisplaySize {
    fn parse(output: &str) -> Result<Self, String> {
        let parse = |value: &str| -> Result<PixelSize, String> {
            let (width, height) = value.trim().split_once('x').ok_or("分辨率格式无效")?;
            let size = PixelSize {
                width: width.parse().map_err(|_| "分辨率宽度无效")?,
                height: height.parse().map_err(|_| "分辨率高度无效")?,
            };
            if size.width == 0 || size.height == 0 {
                return Err("设备返回了空分辨率".into());
            }
            Ok(size)
        };
        let mut physical_size = None;
        let mut override_size = None;
        for line in output.lines().map(str::trim) {
            if let Some(value) = line.strip_prefix("Physical size:") {
                physical_size = Some(parse(value)?);
            } else if let Some(value) = line.strip_prefix("Override size:") {
                override_size = Some(parse(value)?);
            }
        }
        Ok(Self {
            physical_size: physical_size
                .ok_or_else(|| format!("无法读取设备默认分辨率：{output}"))?,
            override_size,
        })
    }
}

impl Adb {
    pub(crate) fn read_display_size(&self) -> Result<DisplaySize, String> {
        DisplaySize::parse(&self.wm_size_output(false)?)
    }

    pub(crate) fn reset_display_size(&self) -> Result<(), String> {
        self.wm_size_output(true).map(|_| ())
    }

    fn wm_size_output(&self, reset: bool) -> Result<String, String> {
        // Never let an unbound ADB invocation choose another connected device.
        self.serial().ok_or("ADB 尚未连接")?;
        let mut command = adb_command(self.path());
        command.args(self.base_args()).args(["shell", "wm", "size"]);
        if reset {
            command.arg("reset");
        }
        command.args(["-d", &self.display_id().to_string()]);
        let output = crate::platform::output_with_timeout(&mut command, Duration::from_secs(5))
            .map_err(|error| format!("执行设备分辨率命令失败：{error}"))?;
        if !output.status.success() {
            return Err(format!(
                "设备分辨率命令失败（{}）：{} {}",
                output.status,
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub(crate) fn screenshot_png(&self) -> Result<Vec<u8>, String> {
        let mut args = self.base_args();
        args.extend(["exec-out".into(), "screencap".into(), "-p".into()]);
        if self.display_id() != 0 {
            let mut command = adb_command(self.path());
            command
                .args(self.base_args())
                .args(["shell", "dumpsys", "display"]);
            let output = crate::platform::output_with_timeout(&mut command, Duration::from_secs(2))
                .map_err(|error| format!("查询截图显示屏失败: {error}"))?;
            let physical = output
                .status
                .success()
                .then(|| {
                    parse_physical_display_id(
                        &String::from_utf8_lossy(&output.stdout),
                        self.display_id(),
                    )
                })
                .flatten()
                .ok_or_else(|| {
                    format!("找不到逻辑显示屏 {} 对应的截图显示屏", self.display_id())
                })?;
            args.extend(["-d".into(), physical.to_string()]);
        }
        let output = adb_command(self.path())
            .args(args)
            .output()
            .map_err(|error| format!("adb screencap failed: {error}"))?;
        if !output.status.success() || output.stdout.is_empty() {
            return Err(format!(
                "adb screencap failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(output.stdout)
    }

    pub(super) fn input_args(&self) -> Vec<String> {
        let mut args = self.base_args();
        args.extend(["shell".into(), "input".into()]);
        if self.display_id() != 0 {
            args.extend(["-d".into(), self.display_id().to_string()]);
        }
        args
    }
}

fn parse_physical_display_id(output: &str, logical_id: u32) -> Option<u64> {
    let mut current = None;
    for line in output.lines().map(str::trim) {
        if let Some(id) = line
            .strip_prefix("Display ")
            .and_then(|line| line.strip_suffix(':'))
        {
            current = id.parse::<u32>().ok();
        }
        if current != Some(logical_id) {
            continue;
        }
        let physical = line
            .strip_prefix("mPrimaryDisplayDevice=")
            .and_then(|line| line.split_once("(local:"))
            .and_then(|(_, id)| id.strip_suffix(')'))
            .or_else(|| {
                line.strip_prefix("mBaseDisplayInfo=")
                    .and_then(|line| line.split_once("uniqueId \"local:"))
                    .and_then(|(_, rest)| rest.split('"').next())
            });
        if let Some(id) = physical.and_then(|id| id.parse().ok()) {
            return Some(id);
        }
    }
    None
}

pub(super) fn input_command(display_id: u32) -> String {
    if display_id == 0 {
        "input".into()
    } else {
        format!("input -d {display_id}")
    }
}

#[cfg(test)]
mod tests {
    use super::super::build_settle_swipe_script_for_display;
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn display_size_distinguishes_override_from_physical_size() {
        let size =
            DisplaySize::parse("Physical size: 1440x2560\r\nOverride size: 720x1280\r\n").unwrap();
        assert_eq!(
            size.physical_size,
            PixelSize {
                width: 1440,
                height: 2560
            }
        );
        assert_eq!(
            size.override_size,
            Some(PixelSize {
                width: 720,
                height: 1280
            })
        );
        assert_eq!(
            DisplaySize::parse("Physical size: 1440x2560")
                .unwrap()
                .override_size,
            None
        );
        for output in [
            "",
            "Error: display not found",
            "Physical size: 0x1920",
            "Physical size: 1080x1920\nOverride size: invalid",
        ] {
            assert!(DisplaySize::parse(output).is_err(), "{output}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn resolution_reset_is_device_and_display_scoped_and_preserves_failures() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("adb");
        std::fs::write(&path, r#"#!/bin/sh
printf '%s\n' "$*" >> "$0.calls"
case "$*" in
  '-s emulator-5554 shell wm size -d 0') printf 'Physical size: 1440x2560\nOverride size: 720x1280\n';;
  '-s emulator-5554 shell wm size reset -d 0') exit 0;;
  '-s emulator-5554 shell wm size reset -d 2') printf 'permission denied' >&2; exit 1;;
  *) exit 2;;
esac
"#).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut adb = Adb {
            adb_path: path.clone(),
            device: Some("emulator-5554".into()),
            selected_serial: None,
            display_id: 0,
            mumu_lookup: Default::default(),
            emulator_selection: Default::default(),
        };
        assert!(adb.read_display_size().unwrap().override_size.is_some());
        adb.reset_display_size().unwrap();
        adb.display_id = 2;
        assert!(adb
            .reset_display_size()
            .unwrap_err()
            .contains("permission denied"));
        let calls = std::fs::read_to_string(path.with_extension("calls")).unwrap();
        assert_eq!(
            calls.lines().collect::<Vec<_>>(),
            [
                "-s emulator-5554 shell wm size -d 0",
                "-s emulator-5554 shell wm size reset -d 0",
                "-s emulator-5554 shell wm size reset -d 2",
            ]
        );
        adb.device = None;
        assert!(adb.reset_display_size().is_err());
        assert_eq!(
            std::fs::read_to_string(path.with_extension("calls")).unwrap(),
            calls
        );
    }

    #[test]
    fn screenshot_maps_logical_id_to_its_own_physical_display() {
        let dump = "Logical Displays: size=3\n\
            Display 0:\n mPrimaryDisplayDevice=内置屏幕(local:4619827820427265280)\n\
            Display 2:\n mPrimaryDisplayDevice=mumuscreen001(local:4619827767814508545)\n\
            Display 3:\n mPrimaryDisplayDevice=mumuscreen002(local:4619826888814064386)";
        assert_eq!(
            parse_physical_display_id(dump, 2),
            Some(4619827767814508545)
        );
        assert_eq!(parse_physical_display_id(dump, 4), None);
        assert_eq!(parse_physical_display_id("Display 2:\n mPrimaryDisplayDevice=virtual:app\nDisplay 3:\n mPrimaryDisplayDevice=screen(local:123)", 2), None);
        assert_eq!(parse_physical_display_id("Display 2:\n mBaseDisplayInfo=DisplayInfo{uniqueId \"local:123\", app 1440 x 2560}", 2), Some(123));
    }

    #[test]
    fn input_on_secondary_display_is_explicit_and_device_scoped() {
        let mut adb = Adb {
            adb_path: PathBuf::from("adb"),
            device: Some("emulator-5554".into()),
            selected_serial: None,
            display_id: 2,
            mumu_lookup: Default::default(),
            emulator_selection: Default::default(),
        };
        assert_eq!(
            adb.input_args(),
            ["-s", "emulator-5554", "shell", "input", "-d", "2"]
        );
        adb.display_id = 0;
        assert_eq!(adb.input_args(), ["-s", "emulator-5554", "shell", "input"]);
    }

    #[test]
    fn every_event_in_settling_swipe_targets_same_display() {
        let script = build_settle_swipe_script_for_display((100, 200), (100, 500), 200, 300, 2);
        let events: Vec<_> = script
            .split("; ")
            .filter(|part| !part.starts_with("sleep "))
            .collect();
        assert!(events
            .iter()
            .all(|event| event.starts_with("input -d 2 motionevent ")));
        assert!(events.first().unwrap().contains("DOWN 100 200"));
        assert!(events.last().unwrap().contains("UP 100 500"));
    }
}
