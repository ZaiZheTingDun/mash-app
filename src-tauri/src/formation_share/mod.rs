//! Offline formation transport. No Tauri, catalog, server, or filesystem state.
//!
//! Keep this boundary independent of persisted models: protocol versions own
//! their names/defaults, and application adapters own migration into models.

mod identity;
mod wire;

use base64::{engine::general_purpose::STANDARD, Engine};
use flate2::{bufread::GzDecoder, Compression, GzBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Read, Write};

pub(crate) const MAX_TEXT_BYTES: usize = 128 * 1024;
pub(crate) const MAX_JSON_BYTES: usize = 1024 * 1024;
const HEADER_LEN: usize = 10;
const CONTAINER_VERSION: u8 = 1;
const GZIP: u8 = 1;
const SCHEMA_VERSION: u8 = 1;
const SUPPORTED_FEATURES: &[u8] = &[1, 2, 3, 4];

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    v: u8,
    f: Vec<u8>,
    c: Value,
    /// Reserved for informational annotations only. Execution fields never
    /// belong here, so old readers may safely ignore future annotations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x: Option<Value>,
}

pub(crate) fn encode(entry: &Value) -> Result<String, String> {
    let entry = wire::portable_entry(entry)?;
    identity::validate(&entry)?;
    let features = wire::required_features(&entry);
    let compact = wire::pack(entry)?;
    let json = serde_json::to_vec(&Envelope {
        v: SCHEMA_VERSION,
        f: features,
        c: compact,
        x: None,
    })
    .map_err(|error| format!("生成编队文本失败：{error}"))?;
    if json.len() > MAX_JSON_BYTES {
        return Err("编队配置过大，无法生成分享码".into());
    }
    let mut encoder = GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), Compression::best());
    encoder
        .write_all(&json)
        .map_err(|error| error.to_string())?;
    let compressed = encoder.finish().map_err(|error| error.to_string())?;
    let mut packet = Vec::with_capacity(HEADER_LEN + compressed.len());
    packet.extend_from_slice(b"MASH");
    packet.extend_from_slice(&[CONTAINER_VERSION, GZIP]);
    packet.extend_from_slice(&(json.len() as u32).to_be_bytes());
    packet.extend_from_slice(&compressed);
    let text = STANDARD.encode(packet);
    if text.len() > MAX_TEXT_BYTES {
        return Err("编队分享码超过长度限制".into());
    }
    Ok(text)
}

pub(crate) fn decode(text: &str) -> Result<Value, String> {
    if text.len() > MAX_TEXT_BYTES {
        return Err("分享码过长".into());
    }
    let text: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let packet = STANDARD
        .decode(text)
        .map_err(|_| "分享码不是完整有效的标准 Base64 文本".to_string())?;
    if packet.len() < HEADER_LEN || &packet[..4] != b"MASH" {
        return Err("不是 Mash 编队分享码".into());
    }
    if packet[4] != CONTAINER_VERSION || packet[5] != GZIP {
        return Err("不支持的分享码格式或压缩算法，请升级 Mash".into());
    }
    let expected = u32::from_be_bytes(packet[6..10].try_into().unwrap()) as usize;
    if expected > MAX_JSON_BYTES {
        return Err("分享码声明的配置大小超过限制".into());
    }
    let mut decoder = GzDecoder::new(&packet[HEADER_LEN..]);
    let mut json = Vec::new();
    decoder
        .by_ref()
        .take((MAX_JSON_BYTES + 1) as u64)
        .read_to_end(&mut json)
        .map_err(|_| "分享码压缩数据损坏或不完整".to_string())?;
    if json.len() > MAX_JSON_BYTES || json.len() != expected || !decoder.get_ref().is_empty() {
        return Err("分享码长度校验失败或包含多余数据".into());
    }
    let strict: wire::StrictValue =
        serde_json::from_slice(&json).map_err(|error| format!("分享码配置无法解析：{error}"))?;
    let envelope: Envelope =
        serde_json::from_value(strict.0).map_err(|error| format!("分享码协议结构错误：{error}"))?;
    if envelope.v != SCHEMA_VERSION {
        return Err(format!("不支持的编队配置版本 {}，请升级 Mash", envelope.v));
    }
    if envelope
        .f
        .iter()
        .any(|feature| !SUPPORTED_FEATURES.contains(feature))
    {
        return Err("编队使用当前版本不支持的功能，请升级 Mash".into());
    }
    let entry = wire::unpack(envelope.c)?;
    if wire::required_features(&entry)
        .iter()
        .any(|feature| !envelope.f.contains(feature))
    {
        return Err("分享码缺少必需功能声明".into());
    }
    identity::validate(&entry)?;
    Ok(entry)
}

/// Serde models intentionally accept old optional fields. The share adapter
/// additionally rejects fields that the current models would silently discard.
pub(crate) fn reject_discarded_fields(input: &Value, modeled: &Value) -> Result<(), String> {
    match (input, modeled) {
        (Value::Object(source), Value::Object(target)) => {
            for (key, value) in source {
                let modeled = target
                    .get(key)
                    .ok_or_else(|| format!("无法识别的配置字段：{key}"))?;
                reject_discarded_fields(value, modeled)?;
            }
        }
        (Value::Array(source), Value::Array(target)) => {
            for (source, target) in source.iter().zip(target) {
                reject_discarded_fields(source, target)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests;
