//! A short-lived scrcpy 2.7 control-only connection. No desktop clipboard or CV.
//! It exists only while the user is asked to copy their Reqable API key.
use super::run_adb;
use crate::{device_operation::OperationCancellation, platform};
use std::{
    io::Read,
    net::{Shutdown, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    time::{Duration, Instant},
};

const MAX_TEXT: usize = 64 * 1024;

pub(super) struct ClipboardReader {
    adb_path: PathBuf,
    serial: String,
    jar: String,
    forward: Option<String>,
    child: Option<Child>,
    socket: Option<TcpStream>,
    buffer: Vec<u8>,
}

impl ClipboardReader {
    pub(super) fn start(
        adb_path: &Path,
        jar_path: &Path,
        serial: &str,
        cancellation: &OperationCancellation,
    ) -> Result<Self, String> {
        cancellation.check()?;
        if !jar_path.is_file() {
            return Err("找不到 scrcpy-server.jar，无法读取模拟器剪贴板".into());
        }
        let id = uuid::Uuid::new_v4();
        let scid = u32::from_be_bytes(id.as_bytes()[..4].try_into().unwrap()) & 0x7fff_ffff;
        let mut reader = Self {
            adb_path: adb_path.into(),
            serial: serial.into(),
            jar: format!("/data/local/tmp/mash-clipboard-{}.jar", id.simple()),
            forward: None,
            child: None,
            socket: None,
            buffer: Vec::new(),
        };
        let jar_path = jar_path.to_str().ok_or("scrcpy 资源路径无效")?;
        run_adb(adb_path, &["-s", serial, "push", jar_path, &reader.jar])
            .map_err(|_| "无法准备模拟器剪贴板读取服务")?;
        cancellation.check()?;
        let output = run_adb(
            adb_path,
            &[
                "-s",
                serial,
                "forward",
                "tcp:0",
                &format!("localabstract:scrcpy_{scid:08x}"),
            ],
        )
        .map_err(|_| "无法连接模拟器剪贴板服务")?;
        let port = parse_port(&output)?;
        reader.forward = Some(format!("tcp:{port}"));
        let mut command = crate::adb::adb_command(adb_path);
        command
            .args(["-s", serial, "shell", &server_command(&reader.jar, scid)])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        reader.child = Some(command.spawn().map_err(|_| "无法启动模拟器剪贴板服务")?);
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            cancellation.check()?;
            if reader
                .child
                .as_mut()
                .unwrap()
                .try_wait()
                .map_err(|_| "无法检查剪贴板服务")?
                .is_some()
            {
                return Err("模拟器剪贴板服务已退出，请检查 Android 版本和 ADB 权限".into());
            }
            if let Ok(mut socket) = TcpStream::connect_timeout(&address, Duration::from_millis(250))
            {
                socket
                    .set_read_timeout(Some(Duration::from_millis(250)))
                    .map_err(|_| "无法配置剪贴板连接")?;
                // ADB forwarding can accept before scrcpy's abstract socket is ready.
                // The server emits this dummy byte only after accepting control.
                let mut dummy = [255];
                if socket.read_exact(&mut dummy).is_ok() && dummy == [0] {
                    reader.socket = Some(socket);
                    return Ok(reader);
                }
            }
            cancellation.wait(Duration::from_millis(100));
        }
        Err("连接模拟器剪贴板超时，请重试".into())
    }

    pub(super) fn next_text(&mut self) -> Result<Option<String>, String> {
        if let Some(text) = decode_clipboard(&mut self.buffer)? {
            return Ok(Some(text));
        }
        let mut bytes = [0; 4096];
        match self
            .socket
            .as_mut()
            .ok_or("剪贴板连接尚未就绪")?
            .read(&mut bytes)
        {
            Ok(0) => Err("模拟器剪贴板连接已断开，请重试".into()),
            Ok(length) => {
                self.buffer.extend_from_slice(&bytes[..length]);
                decode_clipboard(&mut self.buffer)
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                Ok(None)
            }
            Err(_) => Err("无法读取模拟器剪贴板，请重试".into()),
        }
    }
}

fn parse_port(output: &[u8]) -> Result<u16, String> {
    std::str::from_utf8(output)
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok())
        .filter(|port| *port != 0)
        .ok_or_else(|| "模拟器剪贴板转发端口无效".into())
}

fn server_command(jar: &str, scid: u32) -> String {
    // Clipboard autosync here only sends device messages into this private socket.
    // There is no host-clipboard synchronization and this process is closed once
    // one valid key is received, cancelled, or timed out. No COPY/CUT/PASTE is sent.
    format!("CLASSPATH={jar} app_process / com.genymobile.scrcpy.Server 2.7 scid={scid:08x} video=false audio=false control=true tunnel_forward=true send_device_meta=false clipboard_autosync=true power_on=false cleanup=false log_level=error")
}

fn decode_clipboard(buffer: &mut Vec<u8>) -> Result<Option<String>, String> {
    if buffer.first().is_some_and(|kind| *kind != 0) {
        return Err("模拟器剪贴板协议无效".into());
    }
    if buffer.len() < 5 {
        return Ok(None);
    }
    let length = u32::from_be_bytes(buffer[1..5].try_into().unwrap()) as usize;
    if length > MAX_TEXT {
        return Err("模拟器剪贴板文本过长，请只复制 API key".into());
    }
    if buffer.len() < length + 5 {
        return Ok(None);
    }
    let text = std::str::from_utf8(&buffer[5..5 + length])
        .map_err(|_| "模拟器剪贴板文本编码无效")?
        .to_owned();
    buffer.drain(..5 + length);
    Ok(Some(text))
}

impl Drop for ClipboardReader {
    fn drop(&mut self) {
        if let Some(socket) = self.socket.take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(forward) = &self.forward {
            let _ = run_adb(
                &self.adb_path,
                &["-s", &self.serial, "forward", "--remove", forward],
            );
        }
        let _ = platform::output_with_timeout(
            crate::adb::adb_command(&self.adb_path).args([
                "-s",
                &self.serial,
                "shell",
                &format!("rm -f {}", self.jar),
            ]),
            Duration::from_secs(2),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn packet(text: &str) -> Vec<u8> {
        let mut bytes = vec![0];
        bytes.write_all(&(text.len() as u32).to_be_bytes()).unwrap();
        bytes.extend_from_slice(text.as_bytes());
        bytes
    }

    #[test]
    fn fragmented_utf8_and_consecutive_copies_preserve_message_boundaries() {
        let first = packet("测试 key");
        let mut buffer = Vec::new();
        for byte in &first[..first.len() - 1] {
            buffer.push(*byte);
            assert_eq!(decode_clipboard(&mut buffer).unwrap(), None);
        }
        buffer.push(*first.last().unwrap());
        buffer.extend(packet("next"));
        assert_eq!(
            decode_clipboard(&mut buffer).unwrap().as_deref(),
            Some("测试 key")
        );
        assert_eq!(
            decode_clipboard(&mut buffer).unwrap().as_deref(),
            Some("next")
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn empty_invalid_oversized_and_non_utf8_payloads_are_handled_without_echoing_data() {
        assert_eq!(decode_clipboard(&mut packet("")).unwrap(), Some("".into()));
        for mut bytes in [vec![1], vec![0, 0, 1, 0, 1], vec![0, 0, 0, 0, 1, 255]] {
            assert!(decode_clipboard(&mut bytes).is_err());
        }
        assert!(parse_port(b"0").is_err());
        assert_eq!(parse_port(b"12345\r\n").unwrap(), 12345);
    }

    #[test]
    fn server_only_enables_a_temporary_control_channel() {
        let command = server_command("/data/local/tmp/test.jar", 42);
        for option in [
            "video=false",
            "audio=false",
            "control=true",
            "send_device_meta=false",
            "power_on=false",
            "scid=0000002a",
        ] {
            assert!(command.contains(option));
        }
    }
}
