//! AP recovery item mapping and page candidate ordering.

use super::*;

const AP_RECOVERY_CLOSE_POLL: Duration = Duration::from_millis(300);
const AP_RECOVERY_CLOSE_TIMEOUT: Duration = Duration::from_secs(6);
const AP_RECOVERY_CONFIRM_APPEAR_POLL: Duration = Duration::from_millis(200);
const AP_RECOVERY_CONFIRM_APPEAR_TIMEOUT: Duration = Duration::from_secs(3);
pub(crate) const AP_RECOVERY_TAP_SETTLE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApRecoveryPage {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ApRecoveryTemplate {
    pub(crate) item: ApRecoveryItem,
    pub(crate) page: ApRecoveryPage,
    pub(crate) label: &'static str,
    pub(crate) template_key: &'static str,
}

pub(crate) fn ap_recovery_list_label_probes(
    server: Server,
) -> &'static [(&'static str, Option<f64>)] {
    const JP_PROBES: &[(&str, Option<f64>)] = &[(AP_RECOVERY_LIST_LABEL_TEMPLATE, None)];
    const CN_PROBES: &[(&str, Option<f64>)] = &[
        (AP_RECOVERY_LIST_LABEL_TEMPLATE, None),
        (
            AP_RECOVERY_LIST_LABEL_NEW_TEMPLATE,
            Some(AP_RECOVERY_LIST_LABEL_NEW_REFERENCE_WIDTH),
        ),
    ];

    match server {
        Server::Jp => JP_PROBES,
        Server::Cn => CN_PROBES,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApRecoveryCloseObservation {
    StillOpen,
    Closed,
    Obscured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApRecoveryUseOutcome {
    Confirmed,
    Pending,
    Failed,
}

pub(crate) fn resolve_pending_ap_recovery_item(
    pending: Option<ApRecoveryItem>,
    screen: Screen,
) -> (Option<ApRecoveryItem>, Option<ApRecoveryItem>) {
    let Some(item) = pending else {
        return (None, None);
    };
    match screen {
        Screen::Unknown => (Some(item), None),
        Screen::APRecovery => (None, None),
        _ => (None, Some(item)),
    }
}

pub(crate) fn classify_ap_recovery_close_observation(screen: Screen) -> ApRecoveryCloseObservation {
    match screen {
        Screen::APRecovery => ApRecoveryCloseObservation::StillOpen,
        Screen::Unknown => ApRecoveryCloseObservation::Obscured,
        _ => ApRecoveryCloseObservation::Closed,
    }
}

pub(crate) fn ap_recovery_confirm_region(item: ApRecoveryItem) -> NormRect {
    match item {
        ApRecoveryItem::Rainbow | ApRecoveryItem::Gold => AP_RECOVERY_CONFIRM_UPPER_REGION,
        ApRecoveryItem::Silver | ApRecoveryItem::Bronze | ApRecoveryItem::Copper => {
            AP_RECOVERY_CONFIRM_LOWER_REGION
        }
    }
}

pub(crate) fn ap_recovery_template(item: ApRecoveryItem) -> ApRecoveryTemplate {
    match item {
        // Frontend `Rainbow` maps to the premium Saint Quartz recovery option.
        ApRecoveryItem::Rainbow => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Top,
            label: "圣晶石",
            template_key: "items/item_saint_quartz",
        },
        ApRecoveryItem::Gold => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Top,
            label: "黄金苹果",
            template_key: "items/item_apple_gold",
        },
        ApRecoveryItem::Silver => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Top,
            label: "白银苹果",
            template_key: "items/item_apple_silver",
        },
        ApRecoveryItem::Bronze => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Bottom,
            label: "青铜苹果",
            template_key: "items/item_apple_bronzed_cobalt",
        },
        ApRecoveryItem::Copper => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Bottom,
            label: "赤铜苹果",
            template_key: "items/item_apple_bronze",
        },
    }
}

pub(crate) fn ap_recovery_candidates_for_page(
    configured: &[ApRecoveryItem],
    page: ApRecoveryPage,
) -> Vec<ApRecoveryTemplate> {
    [
        ApRecoveryItem::Gold,
        ApRecoveryItem::Silver,
        ApRecoveryItem::Bronze,
        ApRecoveryItem::Copper,
        ApRecoveryItem::Rainbow,
    ]
    .into_iter()
    .filter(|item| configured.contains(item))
    .map(ap_recovery_template)
    .filter(|template| template.page == page)
    .collect()
}

impl Runner {
    pub(crate) fn ap_recovery_candidates(&self, page: ApRecoveryPage) -> Vec<ApRecoveryTemplate> {
        ap_recovery_candidates_for_page(&self.config.ap_recovery_items, page)
    }

    pub(crate) fn find_ap_recovery_item(
        &mut self,
        item: ApRecoveryTemplate,
    ) -> Result<Option<Point>, String> {
        self.sidecar().find_enabled_ap_recovery_item(
            None,
            item.template_key,
            AP_RECOVERY_ITEMS_REGION,
            AP_RECOVERY_ITEM_THRESHOLD,
        )
    }

    pub(crate) fn ap_recovery_list_visible(&mut self) -> Result<bool, String> {
        for &(template_key, reference_width) in ap_recovery_list_label_probes(self.server) {
            let found = match reference_width {
                Some(reference_width) => self.sidecar().find_element_with_reference_width(
                    None,
                    template_key,
                    AP_RECOVERY_ITEMS_REGION,
                    0.8,
                    reference_width,
                )?,
                None => self.sidecar().find_element(
                    None,
                    template_key,
                    AP_RECOVERY_ITEMS_REGION,
                    0.8,
                )?,
            };
            if found.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn tap_ap_recovery_item(
        &mut self,
        item: ApRecoveryTemplate,
        point: Point,
    ) -> ApRecoveryUseOutcome {
        self.emit("APRecovery", &format!("行动力不足，使用{}", item.label));
        if !self.tap_at("APRecovery", point) {
            return ApRecoveryUseOutcome::Failed;
        }
        thread::sleep(AP_RECOVERY_TAP_SETTLE);

        let Some(confirm_point) = self.wait_for_ap_recovery_confirm(item.item) else {
            return ApRecoveryUseOutcome::Failed;
        };
        if !self.tap_at("APRecovery", confirm_point) {
            return ApRecoveryUseOutcome::Failed;
        }
        thread::sleep(AP_RECOVERY_TAP_SETTLE);
        self.wait_for_ap_recovery_to_close()
    }

    fn wait_for_ap_recovery_confirm(&mut self, item: ApRecoveryItem) -> Option<Point> {
        let start = Instant::now();
        loop {
            if self.is_cancelled() {
                return None;
            }
            match self.sidecar().find_element(
                None,
                AP_RECOVERY_CONFIRM_TEMPLATE,
                ap_recovery_confirm_region(item),
                0.8,
            ) {
                Ok(Some(point)) => return Some(point),
                Ok(None) => {}
                Err(err) => {
                    self.fail_action("APRecovery", "识别苹果使用确认弹窗", err);
                    return None;
                }
            }
            if start.elapsed() >= AP_RECOVERY_CONFIRM_APPEAR_TIMEOUT {
                self.emit_warn("APRecovery", "未检测到苹果使用确认弹窗，返回主循环重试");
                return None;
            }
            thread::sleep(AP_RECOVERY_CONFIRM_APPEAR_POLL);
        }
    }

    fn wait_for_ap_recovery_to_close(&mut self) -> ApRecoveryUseOutcome {
        let start = Instant::now();
        loop {
            if self.is_cancelled() {
                return ApRecoveryUseOutcome::Failed;
            }
            match self.sidecar().detect(None) {
                Ok(screen) => match classify_ap_recovery_close_observation(screen) {
                    ApRecoveryCloseObservation::StillOpen => {}
                    ApRecoveryCloseObservation::Closed => return ApRecoveryUseOutcome::Confirmed,
                    ApRecoveryCloseObservation::Obscured => {
                        self.emit_debug("APRecovery", "确认后画面暂时无法识别，交回主循环处理");
                        return ApRecoveryUseOutcome::Pending;
                    }
                },
                Err(err) => {
                    eprintln!("[runner] APRecovery close wait detect failed: {err}");
                }
            }
            if start.elapsed() >= AP_RECOVERY_CLOSE_TIMEOUT {
                self.emit_warn("APRecovery", "等待行动力回复页面关闭超时，返回主循环重试");
                return ApRecoveryUseOutcome::Failed;
            }
            thread::sleep(AP_RECOVERY_CLOSE_POLL);
        }
    }

    pub(crate) fn resolve_pending_ap_recovery(&mut self, screen: Screen) {
        let (pending, confirmed) =
            resolve_pending_ap_recovery_item(self.pending_ap_recovery_item, screen);
        self.pending_ap_recovery_item = pending;
        if let Some(item) = confirmed {
            self.record_ap_recovery_usage(item);
        }
    }

    fn finish_ap_recovery_attempt(&mut self, item: ApRecoveryItem, outcome: ApRecoveryUseOutcome) {
        match outcome {
            ApRecoveryUseOutcome::Confirmed => self.record_ap_recovery_usage(item),
            ApRecoveryUseOutcome::Pending => self.pending_ap_recovery_item = Some(item),
            ApRecoveryUseOutcome::Failed => {}
        }
    }

    pub(crate) fn handle_ap_recovery(&mut self) {
        if self.config.ap_recovery_items.is_empty() {
            self.emit("APRecovery", "行动力不足且未配置自动吃苹果，停止");
            self.transition_lifecycle(RunnerLifecycleEvent::Finished);
            return;
        }

        match self.ap_recovery_list_visible() {
            Ok(true) => {}
            Ok(false) => {
                self.emit("APRecovery", "未定位到道具列表，等待重试…");
                thread::sleep(ACTION_DELAY);
                return;
            }
            Err(err) => {
                self.fail_action("APRecovery", "定位道具列表", err);
                return;
            }
        }

        for item in self.ap_recovery_candidates(ApRecoveryPage::Top) {
            match self.find_ap_recovery_item(item) {
                Ok(Some(point)) => {
                    let outcome = self.tap_ap_recovery_item(item, point);
                    self.finish_ap_recovery_attempt(item.item, outcome);
                    return;
                }
                Ok(None) => {}
                Err(err) => {
                    self.fail_action("APRecovery", &format!("识别{}", item.label), err);
                    return;
                }
            }
        }

        let bottom_items = self.ap_recovery_candidates(ApRecoveryPage::Bottom);
        if bottom_items.is_empty() {
            self.emit("APRecovery", "已配置苹果数量不足，停止");
            self.transition_lifecycle(RunnerLifecycleEvent::Finished);
            return;
        }

        self.emit("APRecovery", "上半页未找到可用道具，滚动到底部继续查找");
        if !self.swipe_at(
            "APRecovery",
            AP_RECOVERY_SCROLL_FROM,
            AP_RECOVERY_SCROLL_TO,
            350,
        ) {
            return;
        }
        thread::sleep(ACTION_DELAY);

        for item in bottom_items {
            match self.find_ap_recovery_item(item) {
                Ok(Some(point)) => {
                    let outcome = self.tap_ap_recovery_item(item, point);
                    self.finish_ap_recovery_attempt(item.item, outcome);
                    return;
                }
                Ok(None) => {}
                Err(err) => {
                    self.fail_action("APRecovery", &format!("识别{}", item.label), err);
                    return;
                }
            }
        }

        self.emit("APRecovery", "所有已配置苹果数量不足，停止");
        self.transition_lifecycle(RunnerLifecycleEvent::Finished);
    }
}
