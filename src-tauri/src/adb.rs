use std::path::PathBuf;
use std::process::Command;

pub struct Adb {
    device: Option<String>,
    use_bluestack: bool,
}

impl Adb {
    pub fn new(use_bluestack: bool) -> Self {
        Self {
            device: None,
            use_bluestack,
        }
    }

    fn base_args(&self) -> Vec<String> {
        match &self.device {
            Some(d) => vec!["-s".into(), d.clone()],
            None => vec![],
        }
    }

    fn parse_first_ready_device(output: &str) -> Option<String> {
        output.lines().skip(1).find_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            let mut parts = trimmed.split('\t');
            let serial = parts.next()?.trim();
            let status = parts.next()?.trim();
            if status == "device" {
                Some(serial.to_string())
            } else {
                None
            }
        })
    }

    fn parse_size_token(text: &str) -> Option<(u32, u32)> {
        let token = text.split_whitespace().find(|part| part.contains('x'))?;
        let (w, h) = token.split_once('x')?;
        let width = w.parse::<u32>().ok()?;
        let height = h.parse::<u32>().ok()?;
        Some((width, height))
    }

    /// Detect and connect to a device. Must be called before other operations.
    pub fn connect(&mut self) -> Result<(), String> {
        if self.use_bluestack {
            Command::new("adb")
                .args(["connect", "127.0.0.1:5555"])
                .output()
                .ok();
        }

        let output = Command::new("adb")
            .arg("devices")
            .output()
            .map_err(|e| format!("failed to run adb: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        self.device = Self::parse_first_ready_device(&stdout);

        if self.device.is_some() {
            Ok(())
        } else {
            Err("no device found".into())
        }
    }

    pub fn screen_size(&self) -> Option<(u32, u32)> {
        let mut args = self.base_args();
        args.extend(["shell".into(), "wm".into(), "size".into()]);
        let output = Command::new("adb").args(&args).output().ok()?;
        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().find_map(|line| {
            let trimmed = line.trim();
            if let Some((_, rhs)) = trimmed.split_once(':') {
                Self::parse_size_token(rhs.trim())
            } else {
                Self::parse_size_token(trimmed)
            }
        })
    }

    /// Capture a screenshot and write the PNG to a temp file.
    /// Returns the path. Caller is responsible for deleting the file.
    pub fn screenshot_to_file(&self) -> Result<PathBuf, String> {
        let mut args = self.base_args();
        args.extend(["exec-out".into(), "screencap".into(), "-p".into()]);

        let output = Command::new("adb")
            .args(&args)
            .output()
            .map_err(|e| format!("adb screencap failed: {e}"))?;

        if !output.status.success() {
            return Err(format!("adb screencap exited with: {}", output.status));
        }

        if output.stdout.is_empty() {
            return Err("adb screencap returned empty output".into());
        }

        let tmp = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .map_err(|e| format!("failed to create temp file: {e}"))?;

        let path = tmp.path().to_path_buf();

        // Persist the file so it outlives this scope (caller deletes it)
        tmp.persist(&path)
            .map_err(|e| format!("failed to persist temp file: {e}"))?;

        std::fs::write(&path, &output.stdout)
            .map_err(|e| format!("failed to write screenshot: {e}"))?;

        Ok(path)
    }

    pub fn tap(&self, x: u32, y: u32) -> Result<(), String> {
        let mut args = self.base_args();
        args.extend([
            "shell".into(),
            "input".into(),
            "tap".into(),
            x.to_string(),
            y.to_string(),
        ]);

        let output = Command::new("adb")
            .args(&args)
            .output()
            .map_err(|e| format!("adb tap failed: {e}"))?;
        if !output.status.success() {
            return Err(format!("adb tap exited with: {}", output.status));
        }

        Ok(())
    }

    pub fn swipe(
        &self,
        from: (u32, u32),
        to: (u32, u32),
        duration_ms: u32,
    ) -> Result<(), String> {
        let mut args = self.base_args();
        args.extend([
            "shell".into(),
            "input".into(),
            "swipe".into(),
            from.0.to_string(),
            from.1.to_string(),
            to.0.to_string(),
            to.1.to_string(),
            duration_ms.to_string(),
        ]);

        let output = Command::new("adb")
            .args(&args)
            .output()
            .map_err(|e| format!("adb swipe failed: {e}"))?;
        if !output.status.success() {
            return Err(format!("adb swipe exited with: {}", output.status));
        }

        Ok(())
    }
}
