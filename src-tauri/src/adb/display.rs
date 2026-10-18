//! Android logical display selection (MuMu keep-alive uses a separate display
//! for each app). SurfaceFlinger's physical display IDs are a different space.

use super::{adb_command, Adb};
use std::time::Duration;

impl Adb {
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
