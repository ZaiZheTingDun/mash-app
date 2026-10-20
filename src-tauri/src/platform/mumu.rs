//! MuMu SDK display lookup. The boot ID ties SDK instance selection to the
//! selected ADB transport, including aliases such as emulator-5554 / :5555.

use std::path::Path;
use std::sync::{Arc, Mutex};

/// SDK handles are process-global in MuMu. Serialize short connect/query/close
/// sequences through application-managed state rather than sharing raw handles.
#[derive(Default)]
pub(crate) struct MuMuLookupState(pub(crate) Arc<Mutex<()>>);

pub(crate) fn resolve_mumu_display(adb_path: &Path, serial: &str) -> Result<Option<u32>, String> {
    imp::resolve(adb_path, serial)
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    pub(super) fn resolve(_: &Path, _: &str) -> Result<Option<u32>, String> {
        Ok(None)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::platform::{external_command, output_with_timeout};
    use std::collections::BTreeSet;
    use std::ffi::CString;
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::time::Duration;

    struct Instance {
        root: PathBuf,
        index: i32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryExW(
            path: *const u16,
            file: *mut std::ffi::c_void,
            flags: u32,
        ) -> *mut std::ffi::c_void;
        fn GetProcAddress(module: *mut std::ffi::c_void, name: *const u8) -> *mut std::ffi::c_void;
        fn FreeLibrary(module: *mut std::ffi::c_void) -> i32;
    }

    struct Library(*mut std::ffi::c_void);

    impl Library {
        unsafe fn load(path: &Path) -> Result<Self, String> {
            let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            // Resolve dependencies in the SDK directory and trusted host paths,
            // excluding the application's current working directory.
            let handle = LoadLibraryExW(path.as_ptr(), std::ptr::null_mut(), 0x1100);
            if handle.is_null() {
                Err(format!(
                    "加载 MuMu SDK 失败: {}",
                    std::io::Error::last_os_error()
                ))
            } else {
                Ok(Self(handle))
            }
        }

        unsafe fn symbol(&self, name: &[u8]) -> Result<*mut std::ffi::c_void, String> {
            let address = GetProcAddress(self.0, name.as_ptr());
            if address.is_null() {
                Err(format!(
                    "MuMu SDK 缺少接口 {}",
                    String::from_utf8_lossy(name)
                ))
            } else {
                Ok(address)
            }
        }
    }

    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }

    fn run(path: &Path, args: &[&str]) -> Result<String, String> {
        let mut command = external_command(path);
        command.args(args);
        let output = output_with_timeout(&mut command, Duration::from_secs(5))
            .map_err(|error| format!("MuMu 设备查询失败: {error}"))?;
        // Android's pidof exits with 1 and no output when the app is stopped.
        let stopped = args.get(3) == Some(&"pidof")
            && output.status.code() == Some(1)
            && output.stdout.is_empty()
            && output.stderr.is_empty();
        if !output.status.success() && !stopped {
            return Err(format!(
                "MuMu 设备查询失败: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn installations() -> Result<Vec<(PathBuf, PathBuf)>, String> {
        // Resolve the actual running installation, including custom locations
        // and device-only sessions. Never load a DLL from the working directory.
        let paths = run(Path::new("powershell.exe"), &["-NoProfile", "-NonInteractive", "-Command",
            "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); Get-Process MuMuNxMain,MuMuNxDevice,MuMuPlayer -ErrorAction SilentlyContinue | ForEach-Object { $_.Path }; exit 0"])?;
        let mut roots = BTreeSet::new();
        for path in paths.lines().map(str::trim).filter(|path| !path.is_empty()) {
            for root in Path::new(path).ancestors().skip(1).take(5) {
                for relative in ["nx_main/MuMuManager.exe", "shell/MuMuManager.exe"] {
                    let manager = root.join(relative);
                    if manager.is_file() {
                        roots.insert((root.to_path_buf(), manager));
                    }
                }
            }
        }
        Ok(roots.into_iter().collect())
    }

    fn unrelated_device(display: &str) -> Result<Option<u32>, String> {
        if display.to_ascii_lowercase().contains("mumuscreen") {
            Err("无法将所选 MuMu 设备匹配到运行中的模拟器实例，请确认 MuMu 管理工具与模拟器均已启动".into())
        } else {
            Ok(None)
        }
    }

    pub(super) fn resolve(adb_path: &Path, serial: &str) -> Result<Option<u32>, String> {
        let displays = run(adb_path, &["-s", serial, "shell", "dumpsys", "display"])?;
        super::resolve_for_displays(&displays, || resolve_multiple(adb_path, serial, &displays))
    }

    fn resolve_multiple(
        adb_path: &Path,
        serial: &str,
        displays: &str,
    ) -> Result<Option<u32>, String> {
        let installations = installations()?;
        if installations.is_empty() {
            return unrelated_device(displays);
        }
        let identity = run(
            adb_path,
            &[
                "-s",
                serial,
                "shell",
                "cat",
                "/proc/sys/kernel/random/boot_id",
            ],
        )?;
        let identity = super::boot_id(&identity).ok_or("无法确定所选 ADB 设备的启动标识")?;
        let mut matches = Vec::new();
        for (root, manager) in installations {
            let indexes = run(&manager, &["info", "--vmindex", "all"])
                .and_then(|info| super::running_indexes(&info));
            let Ok(indexes) = indexes else {
                continue;
            };
            for index in indexes {
                let index_string = index.to_string();
                let candidate = run(
                    &manager,
                    &[
                        "adb",
                        "--vmindex",
                        &index_string,
                        "--cmd",
                        "shell cat /proc/sys/kernel/random/boot_id",
                    ],
                );
                if candidate
                    .as_deref()
                    .ok()
                    .and_then(super::boot_id)
                    .as_deref()
                    == Some(&identity)
                {
                    matches.push(Instance {
                        root: root.clone(),
                        index,
                    });
                }
            }
        }
        let instance = match matches.len() {
            0 => return unrelated_device(displays),
            1 => matches.remove(0),
            _ => return Err("多个 MuMu 实例匹配同一设备，无法确定目标实例".into()),
        };
        let package = super::running_game_package(|args| {
            let mut scoped = vec!["-s", serial, "shell"];
            scoped.extend_from_slice(args);
            run(adb_path, &scoped)
        })?;
        let id = sdk_display(&instance, &package)?;
        eprintln!(
            "[mumu] selected serial={serial}, instance={}, package={package}, display={id}",
            instance.index
        );
        Ok(Some(id))
    }

    fn sdk_display(instance: &Instance, package: &str) -> Result<u32, String> {
        let path = [
            "nx_main/sdk/external_renderer_ipc.dll",
            "shell/sdk/external_renderer_ipc.dll",
        ]
        .iter()
        .map(|relative| instance.root.join(relative))
        .find(|path| path.is_file())
        .ok_or("MuMu SDK 不存在，请更新 MuMu 模拟器")?;
        let root: Vec<u16> = instance
            .root
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let package_c = CString::new(package).map_err(|error| error.to_string())?;
        // SDK ABI: MaaXYZ/EmulatorExtras/Mumu/external_renderer_ipc.
        // All symbols and the handle stay within the loaded library's lifetime.
        unsafe {
            let library = Library::load(&path)?;
            let connect: unsafe extern "C" fn(*const u16, i32) -> i32 =
                std::mem::transmute(library.symbol(b"nemu_connect\0")?);
            let disconnect: unsafe extern "C" fn(i32) =
                std::mem::transmute(library.symbol(b"nemu_disconnect\0")?);
            let display: unsafe extern "C" fn(i32, *const std::ffi::c_char, i32) -> i32 =
                std::mem::transmute(library.symbol(b"nemu_get_display_id\0")?);
            let handle = connect(root.as_ptr(), instance.index);
            if handle <= 0 {
                return Err("连接 MuMu SDK 失败".into());
            }
            let id = display(handle, package_c.as_ptr(), 0);
            disconnect(handle);
            u32::try_from(id)
                .map_err(|_| format!("MuMu 找不到 FGO 的显示屏（{package}），请确认游戏正在运行"))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        #[ignore = "read-only live probe; set MASH_LIVE_ADB_SERIAL"]
        fn live_package_display_lookup() {
            let serial = std::env::var("MASH_LIVE_ADB_SERIAL").expect("set selected ADB serial");
            let adb = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/adb/adb.exe");
            let display = resolve(&adb, &serial)
                .expect("SDK lookup failed")
                .expect("not MuMu");
            println!("Resolved game on {serial} to display {display}");
        }
    }
}

/// Only logical display IDs belong here; physical/SurfaceFlinger IDs cannot
/// establish whether the default Android display is the only available one.
#[cfg(any(windows, test))]
fn resolve_for_displays(
    output: &str,
    lookup: impl FnOnce() -> Result<Option<u32>, String>,
) -> Result<Option<u32>, String> {
    let ids: std::collections::BTreeSet<u32> = output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix("Display ")
                .and_then(|rest| rest.strip_suffix(':'))
                .or_else(|| line.strip_prefix("mDisplayId="))
                .and_then(|id| id.parse().ok())
        })
        .collect();
    if ids.is_empty() {
        return Err("无法识别设备的逻辑显示屏，请检查 ADB 连接后重试".into());
    }
    if ids.len() == 1 && ids.contains(&0) {
        return Ok(Some(0));
    }
    lookup()
}

#[cfg(any(windows, test))]
fn running_game_package(
    mut query: impl FnMut(&[&str]) -> Result<String, String>,
) -> Result<String, String> {
    let packages = query(&["pm", "list", "packages"])?;
    let candidates: std::collections::BTreeSet<_> = packages
        .lines()
        .filter_map(|line| line.trim().strip_prefix("package:"))
        .filter(|package| {
            let lower = package.to_ascii_lowercase();
            ["fgo", "fatego", "fategrandorder"]
                .iter()
                .any(|keyword| lower.contains(keyword))
        })
        .collect();
    let mut running = Vec::new();
    for package in candidates {
        let pids = query(&["pidof", package])?;
        if !pids.trim().is_empty() {
            running.push(package);
        }
    }
    match running.as_slice() {
        [] => Err("请先启动 FGO".into()),
        [package] => Ok((*package).into()),
        _ => {
            let reason = match query(&["dumpsys", "window"]) {
                Ok(windows) => {
                    if let Some(package) = focused_package(&windows) {
                        if running.contains(&package) {
                            return Ok(package.into());
                        }
                    }
                    "无法确定前台目标游戏".to_string()
                }
                Err(error) => format!("查询前台窗口失败：{error}"),
            };
            let package = running[0];
            eprintln!(
                "[mumu] warning: {reason}，检测到多个运行中的 FGO {running:?}，选择首个目标游戏 {package} 继续运行"
            );
            Ok(package.into())
        }
    }
}

/// Android can retain a focused window on each keep-alive display. Only the
/// top-focused display represents the user's foreground when these differ.
#[cfg(any(windows, test))]
fn focused_package(output: &str) -> Option<&str> {
    // ANR history contains another display dump before the current state.
    // Keep the final display section and its following windows section only.
    const HEADER: &str = "WINDOW MANAGER DISPLAY CONTENTS (dumpsys window displays)";
    let lines: Vec<_> = output.lines().collect();
    let start = lines.iter().rposition(|line| line.trim_end() == HEADER)?;
    let current = &lines[start + 1..];
    let display_id = |value: &str| {
        value
            .split(|ch: char| !ch.is_ascii_digit())
            .next()
            .and_then(|id| id.parse::<u32>().ok())
    };
    let top = current.iter().find_map(|line| {
        line.trim()
            .strip_prefix("mTopFocusedDisplayId=")
            .and_then(display_id)
    });
    let mut current_display = None;
    let mut focused = std::collections::BTreeSet::new();
    for line in current.iter().map(|line| line.trim()) {
        if let Some(id) = line
            .strip_prefix("Display: mDisplayId=")
            .or_else(|| line.strip_prefix("Display #"))
            .and_then(display_id)
        {
            current_display = Some(id);
        }
        if top.is_some() && current_display.is_some() && current_display != top {
            continue;
        }
        if let Some(package) = line.strip_prefix("mCurrentFocus=").and_then(|focus| {
            focus
                .split_whitespace()
                .find_map(|part| part.split_once('/').map(|(package, _)| package))
        }) {
            focused.insert(package);
        }
    }
    (focused.len() == 1).then(|| *focused.first().unwrap())
}

#[cfg(any(windows, test))]
fn boot_id(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| {
            line.len() == 36
                && line.bytes().enumerate().all(|(index, byte)| {
                    if [8, 13, 18, 23].contains(&index) {
                        byte == b'-'
                    } else {
                        byte.is_ascii_hexdigit()
                    }
                })
        })
        .map(str::to_ascii_lowercase)
}

#[cfg(any(windows, test))]
fn running_indexes(info: &str) -> Result<Vec<i32>, String> {
    let info: serde_json::Value =
        serde_json::from_str(info).map_err(|error| format!("无法解析 MuMu 实例信息: {error}"))?;
    let instances = info.as_object().ok_or("MuMu 实例信息不是对象")?;
    Ok(instances
        .iter()
        .filter(|(_, value)| value["is_android_started"].as_bool() == Some(true))
        .filter_map(|(key, value)| {
            value["index"]
                .as_i64()
                .and_then(|index| i32::try_from(index).ok())
                .or_else(|| value["index"].as_str().unwrap_or(key).parse::<i32>().ok())
        })
        .filter(|index| *index >= 0)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_display_skips_all_package_and_sdk_queries() {
        for dump in [
            "Logical Displays: size=1\n  Display 0:\n    mDisplayId=0",
            "LogicalDisplay:\n    mDisplayId=0",
        ] {
            assert_eq!(
                resolve_for_displays(dump, || panic!("must not inspect apps or SDK")),
                Ok(Some(0))
            );
        }
    }

    #[test]
    fn extra_logical_displays_require_lookup_and_preserve_errors() {
        let dump = "Display 0:\n mDisplayId=0\nDisplay 2:\n mDisplayId=2";
        assert_eq!(resolve_for_displays(dump, || Ok(Some(2))), Ok(Some(2)));
        assert_eq!(resolve_for_displays(dump, || Ok(None)), Ok(None));
        assert_eq!(
            resolve_for_displays(dump, || Err("SDK failure".into())),
            Err("SDK failure".into())
        );
        assert!(resolve_for_displays("Display 2:", || Err("lookup".into())).is_err());
        for dump in [
            "",
            "device offline",
            "Physical Display 0:",
            "DisplayDeviceInfo{displayId 0}",
        ] {
            assert!(resolve_for_displays(dump, || panic!("unknown topology")).is_err());
        }
    }

    /// Simulate ADB responses; unexpected queries fail the test, especially
    /// foreground lookups when just one game is running in the background.
    fn discover(
        packages: &[&str],
        running: &[&str],
        windows: Option<&str>,
    ) -> Result<String, String> {
        running_game_package(|args| match args {
            ["pm", "list", "packages"] => Ok(packages
                .iter()
                .map(|package| format!("package:{package}\r\n"))
                .collect()),
            ["pidof", package] => Ok(if running.contains(package) {
                "123 456\n"
            } else {
                ""
            }
            .into()),
            ["dumpsys", "window"] => Ok(windows.expect("unexpected foreground lookup").into()),
            _ => panic!("unexpected ADB query: {args:?}"),
        })
    }

    #[test]
    fn official_and_channel_packages_are_discovered_without_server_selection() {
        for package in [
            "com.bilibili.fatego",
            "com.aniplex.fategrandorder",
            "com.bilibili.fgo.mi",
            "com.bilibili.fgo.mz",
            "com.tencent.tmgp.fgo",
            "com.bilibili.fgo.uc",
            "com.bilibili.fgo.huawei",
            "com.bilibili.fgo.vivo",
            "com.bilibili.fgo.nearme.gamecenter",
            "other.channel.FGO.custom",
        ] {
            // Return the exact installed package, including its case/suffix.
            assert_eq!(
                discover(&["com.android.launcher", package], &[package], None),
                Ok(package.into())
            );
        }
    }

    #[test]
    fn installed_but_stopped_games_and_unrelated_apps_are_not_selected() {
        assert_eq!(
            discover(&["com.bilibili.fgo.mi"], &[], None),
            Err("请先启动 FGO".into())
        );
        assert!(discover(&["com.android.launcher"], &["com.android.launcher"], None).is_err());
        assert!(discover(&[], &[], None).is_err());
        assert_eq!(
            discover(
                &["com.bilibili.fgo.mi", "com.aniplex.fategrandorder"],
                &["com.bilibili.fgo.mi"],
                None
            ),
            Ok("com.bilibili.fgo.mi".into())
        );
    }

    #[test]
    fn multiple_running_games_prefer_the_focused_candidate() {
        let packages = ["com.aniplex.fategrandorder", "com.bilibili.fgo.mi"];
        assert_eq!(
            discover(
                &packages,
                &packages,
                Some(
                    "WINDOW MANAGER DISPLAY CONTENTS (dumpsys window displays)\n\
                 mFocusedApp=ActivityRecord{ignored com.aniplex.fategrandorder/.Main}\n\
                 mCurrentFocus=Window{a1 u0 com.bilibili.fgo.mi/.MainActivity}"
                )
            ),
            Ok(packages[1].into())
        );
        assert_eq!(
            discover(
                &packages,
                &packages,
                Some(
                    "WINDOW MANAGER DISPLAY CONTENTS (dumpsys window displays)\n\
                 Display: mDisplayId=0\n\
                   mCurrentFocus=Window{a1 u0 com.aniplex.fategrandorder/.Main}\n\
                 Display: mDisplayId=2 (organized)\n\
                   mCurrentFocus=Window{b2 u0 com.bilibili.fgo.mi/.Main}\n\
                 WINDOW MANAGER WINDOWS (dumpsys window windows)\n\
                 mTopFocusedDisplayId=2"
                )
            ),
            Ok(packages[1].into())
        );
    }

    #[test]
    fn anr_history_does_not_override_current_foreground() {
        let packages = ["com.aniplex.fategrandorder", "com.bilibili.fgo.mi"];
        let windows = "WINDOW MANAGER LAST ANR (dumpsys window lastanr)\n\
            WINDOW MANAGER DISPLAY CONTENTS (dumpsys window displays)\n\
              Display: mDisplayId=1\n\
                mCurrentFocus=Window{a1 u0 com.aniplex.fategrandorder/.Main}\n\
              Display: mDisplayId=2\n\
                mCurrentFocus=Window{b2 u0 com.bilibili.fgo.mi/.Main}\n\
              mTopFocusedDisplayId=1\n\
            WINDOW MANAGER DISPLAY CONTENTS (dumpsys window displays)\n\
              Display: mDisplayId=1\n\
                mCurrentFocus=Window{a1 u0 com.aniplex.fategrandorder/.Main}\n\
              Display: mDisplayId=2\n\
                mCurrentFocus=Window{b2 u0 com.bilibili.fgo.mi/.Main}\n\
            WINDOW MANAGER WINDOWS (dumpsys window windows)\n\
              mTopFocusedDisplayId=2";
        assert_eq!(focused_package(windows), Some(packages[1]));
        assert_eq!(
            discover(&packages, &packages, Some(windows)),
            Ok(packages[1].into())
        );
    }

    #[test]
    fn uncertain_foreground_falls_back_to_first_running_game() {
        let packages = ["com.bilibili.fgo.mi", "com.aniplex.fategrandorder"];
        for focus in [
            "mCurrentFocus=Window{a1 u0 com.android.launcher/.Main}",
            "mCurrentFocus=null",
            "",
            "mCurrentFocus=Window{a1 u0 com.bilibili.fgo.mi/.Main}\n\
             mCurrentFocus=Window{b2 u0 com.aniplex.fategrandorder/.Main}",
        ] {
            let windows =
                format!("WINDOW MANAGER DISPLAY CONTENTS (dumpsys window displays)\n{focus}");
            assert_eq!(
                discover(&packages, &packages, Some(&windows)),
                Ok(packages[1].into())
            );
        }
        // Without the current-section header, do not trust any focus fields.
        let incomplete = "mCurrentFocus=Window{a1 u0 com.bilibili.fgo.mi/.Main}";
        assert_eq!(focused_package(incomplete), None);
        for windows in ["", incomplete] {
            assert_eq!(
                discover(&packages, &packages, Some(windows)),
                Ok(packages[1].into())
            );
        }
    }

    #[test]
    fn adb_query_failures_are_not_reported_as_stopped_games() {
        for failing in ["pm", "pidof"] {
            let result = running_game_package(|args| {
                if args[0] == failing {
                    return Err("device offline".into());
                }
                match args[0] {
                    "pm" => Ok("package:com.bilibili.fgo.mi\npackage:com.bilibili.fgo.uc".into()),
                    "pidof" => Ok("123".into()),
                    _ => panic!("unexpected query"),
                }
            });
            assert_eq!(result, Err("device offline".into()));
        }
    }

    #[test]
    fn foreground_query_failure_falls_back_to_first_running_game() {
        let packages = concat!(
            "package:com.bilibili.fgo.uc\n",
            "package:com.bilibili.fgo.mi\n",
            "package:com.aniplex.fategrandorder"
        );
        let result = running_game_package(|args| match args {
            ["pm", "list", "packages"] => Ok(packages.into()),
            ["pidof", "com.aniplex.fategrandorder"] => Ok(String::new()),
            ["pidof", _] => Ok("123".into()),
            ["dumpsys", "window"] => Err("window service unavailable".into()),
            _ => panic!("unexpected ADB query: {args:?}"),
        });
        assert_eq!(result, Ok("com.bilibili.fgo.mi".into()));
    }

    #[test]
    fn boot_identity_ignores_cli_noise_and_cannot_match_empty_or_android_id() {
        assert_eq!(
            boot_id("connected\r\n79882AE6-EAE2-475A-856F-2E45913666B9\r\n"),
            Some("79882ae6-eae2-475a-856f-2e45913666b9".into())
        );
        for invalid in [
            "",
            "device offline",
            "79882ae6eae2475a",
            "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
        ] {
            assert!(boot_id(invalid).is_none());
        }
    }

    #[test]
    fn instance_enumeration_excludes_stopped_and_invalid_instances() {
        let indexes = running_indexes(
            r#"{
            "0":{"index":"0","is_android_started":true},
            "2":{"index":"2","is_android_started":false},
            "4":{"is_android_started":true},
            "9":{"index":8,"is_android_started":true},
            "bad":{"index":"-1","is_android_started":true}
        }"#,
        )
        .unwrap();
        assert_eq!(indexes, [0, 4, 8]);
        assert!(running_indexes("not JSON").is_err());
    }
}
