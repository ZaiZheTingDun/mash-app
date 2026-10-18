//! MuMu SDK display lookup. The boot ID ties SDK instance selection to the
//! selected ADB transport, including aliases such as emulator-5554 / :5555.

use std::path::Path;
use std::sync::{Arc, Mutex};

/// SDK handles are process-global in MuMu. Serialize short connect/query/close
/// sequences through application-managed state rather than sharing raw handles.
#[derive(Default)]
pub(crate) struct MuMuLookupState(pub(crate) Arc<Mutex<()>>);

pub(crate) fn resolve_mumu_display(
    adb_path: &Path,
    serial: &str,
    package: &str,
    selected_root: Option<&Path>,
) -> Result<Option<u32>, String> {
    imp::resolve(adb_path, serial, package, selected_root)
}

pub(crate) fn discover_mumu_installations() -> Result<Vec<std::path::PathBuf>, String> {
    imp::discover()
}

pub(crate) fn validate_mumu_installation(root: &Path) -> Result<(), String> {
    imp::validate(root)
}

pub(crate) fn is_mumu_device(adb_path: &Path, serial: &str) -> Result<bool, String> {
    imp::is_mumu_device(adb_path, serial)
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    pub(super) fn resolve(
        _: &Path,
        _: &str,
        _: &str,
        _: Option<&Path>,
    ) -> Result<Option<u32>, String> {
        Ok(None)
    }
    pub(super) fn discover() -> Result<Vec<std::path::PathBuf>, String> {
        Ok(Vec::new())
    }
    pub(super) fn validate(_: &Path) -> Result<(), String> {
        Err("仅 Windows 支持 MuMu 安装路径".into())
    }
    pub(super) fn is_mumu_device(_: &Path, _: &str) -> Result<bool, String> {
        Ok(false)
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
        if !output.status.success() {
            return Err(format!(
                "MuMu 设备查询失败: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn manager_path(root: &Path) -> Option<PathBuf> {
        ["nx_main/MuMuManager.exe", "shell/MuMuManager.exe"]
            .iter()
            .map(|part| root.join(part))
            .find(|path| path.is_file())
    }

    fn sdk_path(root: &Path) -> Option<PathBuf> {
        let direct = [
            "nx_device/15.0/shell/sdk/external_renderer_ipc.dll",
            "nx_device/15.0/shell/sdk/external_renderer_ipc/external_renderer_ipc.dll",
            "nx_device/12.0/shell/sdk/external_renderer_ipc.dll",
            "nx_device/12.0/shell/sdk/external_renderer_ipc/external_renderer_ipc.dll",
            "nx_main/sdk/external_renderer_ipc.dll",
            "shell/sdk/external_renderer_ipc.dll",
            "shell/sdk/external_renderer_ipc/external_renderer_ipc.dll",
        ]
        .iter()
        .map(|part| root.join(part))
        .find(|path| path.is_file());
        if direct.is_some() {
            return direct;
        }
        let versions = std::fs::read_dir(root.join("nx_device")).ok()?;
        for version in versions.flatten().filter(|entry| entry.path().is_dir()) {
            for part in [
                "shell/sdk/external_renderer_ipc.dll",
                "shell/sdk/external_renderer_ipc/external_renderer_ipc.dll",
            ] {
                let candidate = version.path().join(part);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    pub(super) fn validate(root: &Path) -> Result<(), String> {
        if !root.is_dir() {
            return Err("MuMu 安装目录不存在".into());
        }
        if manager_path(root).is_none() {
            return Err("所选目录中找不到 MuMuManager.exe，请选择 MuMu 安装根目录".into());
        }
        Ok(())
    }

    pub(super) fn discover() -> Result<Vec<PathBuf>, String> {
        // Process paths cover portable installations; uninstall records also
        // make an installation available when only the device process runs.
        let paths = run(Path::new("powershell.exe"), &["-NoProfile", "-NonInteractive", "-Command",
            "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); Get-Process MuMuNxMain,MuMuNxDevice,MuMuPlayer -ErrorAction SilentlyContinue | ForEach-Object { $_.Path }; exit 0"])?;
        let mut roots = BTreeSet::new();
        for path in paths.lines().map(str::trim).filter(|path| !path.is_empty()) {
            for root in Path::new(path).ancestors().skip(1).take(6) {
                if validate(root).is_ok() {
                    roots.insert(root.to_path_buf());
                }
            }
        }
        let registered = run(Path::new("powershell.exe"), &["-NoProfile", "-NonInteractive", "-Command",
            "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $keys = @('HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*','HKLM:\\SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*','HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*'); Get-ItemProperty $keys -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -match 'MuMu' } | ForEach-Object { $_.InstallLocation }; exit 0"])?;
        for path in registered
            .lines()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            let root = PathBuf::from(path.trim_matches('"'));
            if validate(&root).is_ok() {
                roots.insert(root);
            }
        }
        let mut paths: Vec<_> = roots.into_iter().collect();
        paths.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
        paths.dedup_by(|left, right| {
            left.to_string_lossy()
                .eq_ignore_ascii_case(&right.to_string_lossy())
        });
        Ok(paths)
    }

    pub(super) fn is_mumu_device(adb_path: &Path, serial: &str) -> Result<bool, String> {
        let display = run(adb_path, &["-s", serial, "shell", "dumpsys", "display"])?;
        Ok(display.to_ascii_lowercase().contains("mumuscreen"))
    }

    fn unrelated_device(adb_path: &Path, serial: &str) -> Result<Option<u32>, String> {
        if is_mumu_device(adb_path, serial)? {
            Err("无法匹配所选 MuMu 设备。请在右下角核对“选择设备”，并在“选择模拟器”中指定 MuMu 安装路径".into())
        } else {
            Ok(None)
        }
    }

    pub(super) fn resolve(
        adb_path: &Path,
        serial: &str,
        package: &str,
        selected_root: Option<&Path>,
    ) -> Result<Option<u32>, String> {
        let roots = match selected_root {
            Some(root) => {
                validate(root).map_err(|error| {
                    format!("所选 MuMu 安装路径不可用，请在“选择模拟器”中重新选择：{error}")
                })?;
                vec![root.to_path_buf()]
            }
            None => discover().map_err(|error| {
                format!("自动发现 MuMu 失败，请在右下角“选择模拟器”中指定安装路径：{error}")
            })?,
        };
        let installations: Vec<_> = roots
            .into_iter()
            .filter_map(|root| manager_path(&root).map(|manager| (root, manager)))
            .collect();
        if installations.is_empty() {
            return unrelated_device(adb_path, serial);
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
        let mut query_errors = Vec::new();
        let mut running_count = 0usize;
        for (root, manager) in installations {
            let indexes = run(&manager, &["info", "--vmindex", "all"])
                .and_then(|info| super::running_indexes(&info));
            let indexes = match indexes {
                Ok(indexes) => indexes,
                Err(error) => {
                    query_errors.push(format!("{}: {error}", root.display()));
                    continue;
                }
            };
            running_count += indexes.len();
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
                match candidate {
                    Ok(value) => {
                        if super::boot_id(&value).as_deref() == Some(&identity) {
                            matches.push(Instance {
                                root: root.clone(),
                                index,
                            });
                        }
                    }
                    Err(error) => {
                        query_errors.push(format!("{} 实例 {index}: {error}", root.display()))
                    }
                }
            }
        }
        let instance = match matches.len() {
            0 if selected_root.is_some() => {
                if !query_errors.is_empty() {
                    return Err(format!(
                        "所选 MuMu 安装目录查询失败：{}",
                        query_errors.join("；")
                    ));
                }
                if running_count == 0 {
                    return Err("所选 MuMu 安装目录没有运行中的实例，请启动模拟器或在“选择模拟器”中切换路径".into());
                }
                return Err(
                    "所选 MuMu 安装目录中的实例与 ADB 设备不一致，请核对“选择设备”和“选择模拟器”"
                        .into(),
                );
            }
            0 if !query_errors.is_empty() && is_mumu_device(adb_path, serial)? => {
                return Err(format!(
                    "MuMu 实例查询失败，请在“选择模拟器”中核对安装路径：{}",
                    query_errors.join("；")
                ));
            }
            0 => return unrelated_device(adb_path, serial),
            1 => matches.remove(0),
            _ => {
                return Err(
                    "多个 MuMu 实例匹配同一 ADB 设备，请在“选择模拟器”中指定安装路径".into(),
                )
            }
        };
        // The SDK returns display 0 for any package when keep-alive is off.
        // Check the package is running before trusting that result.
        let process = run(adb_path, &["-s", serial, "shell", "pidof", package])
            .map_err(|_| format!("请先启动当前区服的 FGO（{package}）"))?;
        if process.trim().is_empty() {
            return Err(format!("请先启动当前区服的 FGO（{package}）"));
        }
        let id = sdk_display(&instance, package)?;
        eprintln!(
            "[mumu] selected serial={serial}, instance={}, package={package}, display={id}",
            instance.index
        );
        Ok(Some(id))
    }

    fn sdk_display(instance: &Instance, package: &str) -> Result<u32, String> {
        let path = sdk_path(&instance.root).ok_or("MuMu SDK 不存在，请更新 MuMu 模拟器")?;
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
        #[ignore = "read-only live probe; set MASH_LIVE_ADB_SERIAL and MASH_LIVE_GAME_PACKAGE"]
        fn live_package_display_lookup() {
            let serial = std::env::var("MASH_LIVE_ADB_SERIAL").expect("set selected ADB serial");
            let package = std::env::var("MASH_LIVE_GAME_PACKAGE").expect("set target package");
            let adb = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/adb/adb.exe");
            let display = resolve(&adb, &serial, &package, None)
                .expect("SDK lookup failed")
                .expect("not MuMu");
            println!("SDK resolved {package} on {serial} to display {display}");
            assert!(resolve(&adb, &serial, "com.mash.nonexistent.test.package", None).is_err());
        }
    }
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
