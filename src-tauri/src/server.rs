//! Server selection and stream-size invariants shared across commands and runners.

use std::fmt;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Server selection (global app setting). Drives which template/config bundle
// the sidecar loads, which OCR model RapidOCR pins, and how
// `load_servant_metadata` localizes the servant + NP names it sends into
// `find_supports`. Default is JP because that's the only data the project
// originally shipped — flipping the default here would break every existing
// install whose templates assume Japanese UI text.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Server {
    Jp,
    Cn,
}

impl Default for Server {
    fn default() -> Self {
        Self::Jp
    }
}

impl Server {
    /// Lowercase directory token used under `resources/servers/{token}/...`.
    /// Kept intentionally tiny so the resource resolvers stay one-liners.
    pub fn dir_token(&self) -> &'static str {
        match self {
            Self::Jp => "jp",
            Self::Cn => "cn",
        }
    }
}

impl fmt::Display for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Jp => write!(f, "JP"),
            Self::Cn => write!(f, "CN"),
        }
    }
}

impl FromStr for Server {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "JP" => Ok(Self::Jp),
            "CN" => Ok(Self::Cn),
            other => Err(format!("unknown server: {other}")),
        }
    }
}

// ---------------------------------------------------------------------------
// scrcpy stream tunables. Use 1080p as the minimum supported CV input:
// lower resolutions make support skill icons and two-digit levels too unstable.
// Bit rate is the H.264 budget. The frame-rate cap keeps the emulator's
// encoder and the host-side decoder from processing frames far faster than
// the automation polling loop can consume them.
// ---------------------------------------------------------------------------
pub const STREAM_MAX_SIZE: u32 = 1920;
pub(crate) const STREAM_MIN_LONG_SIDE: u32 = 1920;
pub(crate) const STREAM_MIN_SHORT_SIDE: u32 = 1080;
pub const STREAM_BIT_RATE: u32 = 12_000_000;
pub const STREAM_MAX_FPS: u32 = 15;

pub fn stream_meets_minimum_resolution(width: u32, height: u32) -> bool {
    let long = width.max(height);
    let short = width.min(height);
    long >= STREAM_MIN_LONG_SIDE && short >= STREAM_MIN_SHORT_SIDE
}

pub fn stream_resolution_error(width: u32, height: u32) -> String {
    format!(
        "当前视频流分辨率为 {width}x{height}，低于最低支持的 1920x1080。请将模拟器/设备分辨率调整到至少 1080p 后重试。"
    )
}
