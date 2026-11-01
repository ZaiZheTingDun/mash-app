//! Read Reqable's first-run UI; agreement acceptance remains on the device.
use super::{read_shell_bytes, CERT_PATHS};
use crate::runner::LogLevel;
use quick_xml::{events::Event, Reader};
use std::time::{Duration, Instant};

const MAX_XML: usize = 1024 * 1024;
const INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(3);
const CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(120);
const MODE_TIMEOUT: Duration = Duration::from_secs(30);
const INVALID_XML: &str = "Reqable 界面结构无效，请保持应用打开后重试";
type Shell<'a> = dyn FnMut(&str) -> Result<Vec<u8>, String> + 'a;

#[derive(Debug, PartialEq, Eq)]
struct Agreement {
    accept_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TapTarget {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl TapTarget {
    fn parse(raw: &str) -> Option<Self> {
        let raw = raw.strip_prefix('[')?.strip_suffix(']')?;
        let (first, last) = raw.split_once("][")?;
        let (left, top) = first.split_once(',')?;
        let (right, bottom) = last.split_once(',')?;
        let result = Self {
            left: left.parse().ok()?,
            top: top.parse().ok()?,
            right: right.parse().ok()?,
            bottom: bottom.parse().ok()?,
        };
        (result.right.saturating_sub(result.left) > 2
            && result.bottom.saturating_sub(result.top) > 2
            && result.right <= 100_000
            && result.bottom <= 100_000)
            .then_some(result)
    }

    fn contains(self, target: Self) -> bool {
        target.left >= self.left
            && target.top >= self.top
            && target.right <= self.right
            && target.bottom <= self.bottom
    }

    fn tap_command(self) -> String {
        let (dx, dy) = crate::touch::jitter_offset();
        let x = ((self.left + self.right) as i32 / 2 + dx)
            .clamp(self.left as i32 + 1, self.right as i32 - 1);
        let y = ((self.top + self.bottom) as i32 / 2 + dy)
            .clamp(self.top as i32 + 1, self.bottom as i32 - 1);
        format!("input tap {x} {y}")
    }
}

#[derive(Debug, PartialEq, Eq)]
struct WorkModes {
    standalone: TapTarget,
    next: Option<TapTarget>,
}

#[derive(Debug, PartialEq, Eq)]
enum UiState {
    Agreement(Agreement),
    WorkModes(WorkModes),
    Ready,
    OtherApp,
}

fn inspect_ui(xml: &[u8]) -> Result<UiState, String> {
    if xml.len() > MAX_XML {
        return Err("Reqable 界面结构超过读取上限".into());
    }
    let xml = std::str::from_utf8(xml).map_err(|_| INVALID_XML)?;
    let mut reader = Reader::from_str(xml);
    let mut title = false;
    let mut accept = None;
    let mut hierarchy = false;
    let mut depth = 0usize;
    let mut foreground_reqable = None;
    let mut screen_bounds = None;
    let mut work_modes = false;
    let mut work_modes_title = None;
    let mut standalone = None;
    let mut next_candidates = Vec::new();
    loop {
        let event = reader.read_event().map_err(|_| INVALID_XML)?;
        match &event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.checked_sub(1).ok_or(INVALID_XML)?,
            Event::Eof => break,
            _ => {}
        }
        let node = match event {
            Event::Start(node) | Event::Empty(node) => node,
            _ => continue,
        };
        if node.name().as_ref() == b"hierarchy" {
            hierarchy = true;
        }
        if node.name().as_ref() != b"node" {
            continue;
        }
        let mut package = String::new();
        let mut labels = Vec::new();
        let mut class = String::new();
        let mut enabled = None;
        let mut clickable = false;
        let mut bounds = None;
        for attr in node.attributes() {
            let attr = attr.map_err(|_| INVALID_XML)?;
            let value = attr
                .decode_and_unescape_value(reader.decoder())
                .map_err(|_| INVALID_XML)?;
            match attr.key.as_ref() {
                b"package" => package = value.into_owned(),
                b"class" => class = value.into_owned(),
                b"text" | b"content-desc" => labels.push(value.trim().to_lowercase()),
                b"enabled" => {
                    enabled = match value.as_ref() {
                        "true" => Some(true),
                        "false" => Some(false),
                        _ => None,
                    }
                }
                b"clickable" => clickable = value == "true",
                b"bounds" => bounds = TapTarget::parse(&value),
                _ => {}
            }
        }
        if foreground_reqable.is_none() {
            foreground_reqable = Some(package == "com.reqable.android");
            screen_bounds = bounds;
        }
        if package != "com.reqable.android" {
            continue;
        }
        title |= labels.iter().any(|label| {
            matches!(
                label.as_str(),
                "license agreement" | "许可协议" | "用户协议" | "用户许可协议"
            )
        });
        let is_work_modes_title = labels
            .iter()
            .any(|label| matches!(label.as_str(), "work modes" | "工作模式"));
        if is_work_modes_title {
            work_modes = true;
            work_modes_title = bounds;
        }
        if enabled == Some(true)
            && clickable
            && (labels.iter().all(String::is_empty)
                || labels
                    .iter()
                    .any(|label| matches!(label.as_str(), "next" | "下一步")))
        {
            if let Some(target) = bounds {
                next_candidates.push(target);
            }
        }
        if enabled == Some(true)
            && clickable
            && labels.iter().any(|label| {
                matches!(
                    label.lines().next().map(str::trim),
                    Some("standalone mode" | "独立模式")
                )
            })
        {
            let target = bounds
                .filter(|target| screen_bounds.is_some_and(|screen| screen.contains(*target)))
                .ok_or("Reqable 独立模式选项的坐标无效，请重试环境配置")?;
            if standalone.is_some_and(|previous| previous != target) {
                return Err("Reqable 独立模式选项不唯一，请在设备上选择独立模式后重试".into());
            }
            standalone = Some(target);
        }
        if class == "android.widget.Button"
            && labels
                .iter()
                .any(|label| matches!(label.as_str(), "accept" | "agree" | "同意" | "接受"))
        {
            // Flutter exposes the disabled button with clickable=false too.
            accept = enabled;
        }
    }
    if !hierarchy || depth != 0 {
        return Err(INVALID_XML.into());
    }
    if foreground_reqable != Some(true) {
        return Ok(UiState::OtherApp);
    }
    if !title {
        if work_modes {
            let standalone = standalone
                .ok_or("无法识别可点击的 Reqable 独立模式选项，请在设备上选择独立模式后重试")?;
            // The reported Flutter Next button has no label. Restrict the fallback
            // to one enabled control on the title row at the right of the screen.
            let mut next = next_candidates.into_iter().filter(|target| {
                screen_bounds
                    .zip(work_modes_title)
                    .is_some_and(|(screen, title)| {
                        screen.contains(*target)
                            && target.left >= screen.left + (screen.right - screen.left) * 3 / 4
                            && target.top < title.bottom
                            && target.bottom > title.top
                            && target.left > title.right
                    })
            });
            let first = next.next();
            let next = if next.next().is_none() { first } else { None };
            return Ok(UiState::WorkModes(WorkModes { standalone, next }));
        }
        return Ok(UiState::Ready);
    }
    let accept_enabled =
        accept.ok_or("已检测到 Reqable 许可协议，但无法识别确认按钮，请在设备上完成首次初始化")?;
    Ok(UiState::Agreement(Agreement { accept_enabled }))
}

fn probe(shell: &mut Shell<'_>) -> Result<UiState, String> {
    // Unique, bounded temporary dump; never persist or log the complete device UI.
    let path = format!(
        "/data/local/tmp/mash-reqable-{}.xml",
        uuid::Uuid::new_v4().simple()
    );
    // Some MuMu builds write a complete dump, then crash while shutting down
    // uiautomator. Accept the fresh file in that case; inspect_ui still rejects
    // incomplete/invalid XML. A failed dump without a file remains an error.
    let script = format!("set -e; p={path}; trap 'rm -f \"$p\"' EXIT; uiautomator dump \"$p\" >/dev/null 2>&1 || test -s \"$p\"; head -c {} \"$p\"", MAX_XML + 1);
    let xml = shell(&script).map_err(|_| "无法读取 Reqable 界面结构，请保持应用打开后重试")?;
    inspect_ui(&xml)
}

pub(super) fn certificate_after_confirmation(
    root: &str,
    shell: &mut Shell<'_>,
    log: &mut dyn FnMut(LogLevel, &str),
    cancellation: &crate::device_operation::OperationCancellation,
) -> Result<(&'static str, Vec<u8>), String> {
    let started = Instant::now();
    wait_using(
        root,
        shell,
        log,
        &mut |duration| cancellation.wait(duration),
        &|| started.elapsed(),
    )
}

fn wait_using(
    root: &str,
    shell: &mut Shell<'_>,
    log: &mut dyn FnMut(LogLevel, &str),
    pause: &mut dyn FnMut(Duration),
    elapsed: &dyn Fn() -> Duration,
) -> Result<(&'static str, Vec<u8>), String> {
    let mut waiting = false;
    let mut mode_detected = false;
    let mut mode_selected = false;
    let mut next_attempted = false;
    loop {
        let page = probe(shell)?;
        if let UiState::Agreement(page) = page {
            if !waiting {
                let message = if page.accept_enabled {
                    "Reqable 首次启动：请在设备上阅读并确认许可协议，确认后自动继续环境配置（最多等待 120 秒）"
                } else {
                    "Reqable 首次启动：请在设备上阅读许可协议；Accept 按钮目前不可用，完成协议确认后自动继续环境配置（最多等待 120 秒）"
                };
                log(LogLevel::Info, message);
                log(
                    LogLevel::Info,
                    "当前为服务条款页面，请滑动到底部并点击底部按钮接受条款",
                );
                waiting = true;
            }
        } else if let UiState::WorkModes(page) = page {
            if !mode_detected {
                log(
                    LogLevel::Info,
                    "检测到 Reqable 工作模式选择页，正在自动处理，如无响应请手动选择独立模式并进入下一步",
                );
                mode_detected = true;
                mode_selected = shell(&page.standalone.tap_command()).is_ok();
                if !mode_selected {
                    log(
                        LogLevel::Warn,
                        "Reqable 独立模式自动点击失败，等待手动选择独立模式并进入下一步",
                    );
                }
            } else if mode_selected && !next_attempted {
                // Use a fresh dump after the selection, never the old toolbar coordinates.
                if let Some(next) = page.next {
                    next_attempted = true;
                    if shell(&next.tap_command()).is_err() {
                        log(
                            LogLevel::Warn,
                            "Reqable 下一步自动点击失败，等待手动进入下一步",
                        );
                    }
                }
            }
        } else if page == UiState::Ready {
            for path in CERT_PATHS {
                if let Ok(bytes) = read_shell_bytes(root, &format!("head -c 65537 {path}"), shell) {
                    if !bytes.is_empty() {
                        if waiting {
                            log(
                                LogLevel::Info,
                                "Reqable 许可协议页已关闭，初始化证书已生成，继续环境配置",
                            );
                        }
                        return Ok((path, bytes));
                    }
                }
            }
        }
        let timeout = if waiting {
            CONFIRMATION_TIMEOUT
        } else if mode_detected {
            MODE_TIMEOUT
        } else {
            INITIALIZATION_TIMEOUT
        };
        if elapsed() >= timeout {
            return Err(if mode_detected {
                "等待 Reqable 独立模式初始化超时，请在设备上选择独立模式并进入下一步后重试环境配置"
            } else if waiting {
                "等待 Reqable 首次初始化超时，请在设备上完成许可协议确认后重试环境配置"
            } else {
                "未找到 Reqable CA，请先完成 Reqable 的首次初始化后重试"
            }
            .into());
        }
        pause(if waiting {
            Duration::from_secs(1)
        } else {
            Duration::from_millis(300)
        });
    }
}

#[cfg(test)]
mod tests;
