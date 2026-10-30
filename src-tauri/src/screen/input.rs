//! Narrow input handle; the sidecar owns sockets, Rust owns gesture policy.

use super::protocol::{request, SidecarCommand};
use super::transport::SidecarTransport;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScrcpyTouchEvent {
    pub action: u8,
    pub x: u32,
    pub y: u32,
    pub at_ms: u32,
}

#[derive(Clone)]
pub(crate) struct ScrcpyControl {
    transport: Arc<Mutex<SidecarTransport>>,
    stream_id: String,
    pub size: (u32, u32),
}

impl ScrcpyControl {
    pub(super) fn from_stream_response(
        transport: Arc<Mutex<SidecarTransport>>,
        response: &serde_json::Value,
    ) -> Option<Self> {
        let control = &response["control"];
        if control["version"].as_u64()? != 1 {
            return None;
        }
        let width = u16::try_from(response["width"].as_u64()?).ok()?;
        let height = u16::try_from(response["height"].as_u64()?).ok()?;
        let stream_id = control["streamId"].as_str()?;
        if width == 0 || height == 0 || stream_id.is_empty() {
            return None;
        }
        Some(Self {
            transport,
            stream_id: stream_id.to_string(),
            size: (u32::from(width), u32::from(height)),
        })
    }

    pub fn send(&mut self, events: &[ScrcpyTouchEvent]) -> Result<(), String> {
        let duration = events.last().map_or(0, |event| event.at_ms);
        let request = request(
            SidecarCommand::ScrcpyTouch,
            serde_json::json!({
                "streamId": self.stream_id,
                "width": self.size.0,
                "height": self.size.1,
                "events": events,
            }),
        )?;
        let response = self
            .transport
            .lock()
            .map_err(|_| "sidecar transport lock poisoned".to_string())?
            .send_recv(
                &request,
                Duration::from_millis(u64::from(duration) + 10_000),
            )
            .map_err(|error| error.to_string())?;
        if response["ok"].as_bool() != Some(true) {
            return Err(format!(
                "scrcpy touch failed: {}",
                response["error"].as_str().unwrap_or("unknown error")
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_requires_version_session_and_valid_wire_dimensions() {
        let (_, line_rx) = std::sync::mpsc::channel();
        let transport = Arc::new(Mutex::new(SidecarTransport {
            child: None,
            line_rx,
            next_id: 1,
        }));
        let mut response = serde_json::json!({"width":1920,"height":1080,"control":{"version":1,"streamId":"session"}});
        assert!(ScrcpyControl::from_stream_response(transport.clone(), &response).is_some());
        response["width"] = serde_json::json!(65536);
        assert!(ScrcpyControl::from_stream_response(transport.clone(), &response).is_none());
        response["width"] = serde_json::json!(1920);
        response["control"]["version"] = serde_json::json!(2);
        assert!(ScrcpyControl::from_stream_response(transport.clone(), &response).is_none());
        assert!(ScrcpyControl::from_stream_response(transport, &serde_json::json!({})).is_none());
    }

    #[test]
    fn touch_event_fields_match_python_protocol() {
        let event = serde_json::to_value(ScrcpyTouchEvent {
            action: 2,
            x: 40,
            y: 80,
            at_ms: 20,
        })
        .unwrap();
        assert_eq!(
            event,
            serde_json::json!({"action":2,"x":40,"y":80,"atMs":20})
        );
    }
}
