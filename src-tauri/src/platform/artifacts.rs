//! Host naming conventions for bundled executables and runtime artifacts.

#[cfg(windows)]
pub(crate) fn adb_executable_name() -> &'static str {
    "adb.exe"
}

#[cfg(not(windows))]
pub(crate) fn adb_executable_name() -> &'static str {
    "adb"
}

pub(crate) fn runtime_platform_key() -> String {
    runtime_platform_key_from(std::env::consts::OS, std::env::consts::ARCH)
}

pub(crate) fn runtime_platform_key_from(os: &str, arch: &str) -> String {
    let os = match os {
        "macos" => "darwin",
        other => other,
    };
    format!("{os}-{arch}")
}

pub(crate) fn runtime_exe_name() -> &'static str {
    if cfg!(windows) {
        "mash-cv.exe"
    } else {
        "mash-cv"
    }
}
