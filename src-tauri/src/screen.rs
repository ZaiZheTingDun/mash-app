//! Python CV sidecar client and typed recognition results.
//!
//! Process lifecycle and transport live in `client`; individual CV requests
//! live in `operations`. Callers continue to use the stable exports here.

mod client;
mod operations;
mod protocol;
mod types;

pub use client::{SidecarClient, TemplateLoadSpec};
pub use types::*;

pub const SIDECAR_STARTUP_REDOWNLOAD_MESSAGE: &str =
    "CV 运行时启动失败，请前往资源管理重新下载 CV 运行时。";

pub fn sidecar_startup_user_message(error: &str) -> Option<&'static str> {
    if error.contains("sidecar did not become ready")
        || error.contains("OpenCV bindings requires")
        || error.contains("invalid JSON from sidecar")
    {
        Some(SIDECAR_STARTUP_REDOWNLOAD_MESSAGE)
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
