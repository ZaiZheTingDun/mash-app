//! AP recovery item mapping and page candidate ordering.

use super::*;

const AP_RECOVERY_CLOSE_POLL: Duration = Duration::from_millis(300);
const AP_RECOVERY_CLOSE_TIMEOUT: Duration = Duration::from_secs(6);

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
        Ok(self
            .sidecar()
            .find_element(
                None,
                AP_RECOVERY_LIST_LABEL_TEMPLATE,
                AP_RECOVERY_ITEMS_REGION,
                0.8,
            )?
            .is_some())
    }

    fn tap_ap_recovery_item(&mut self, item: ApRecoveryTemplate, point: Point) -> bool {
        self.emit("APRecovery", &format!("行动力不足，使用{}", item.label));
        if !self.tap_at("APRecovery", point) {
            return false;
        }
        thread::sleep(ACTION_DELAY);
        if !self.tap_at("APRecovery", AP_RECOVERY_CONFIRM_BUTTON) {
            return false;
        }
        self.wait_for_ap_recovery_to_close()
    }

    fn wait_for_ap_recovery_to_close(&mut self) -> bool {
        let start = Instant::now();
        loop {
            if self.is_cancelled() {
                return false;
            }
            thread::sleep(AP_RECOVERY_CLOSE_POLL);
            match self.sidecar().detect(None) {
                Ok(Screen::APRecovery) => {}
                Ok(_) => return true,
                Err(err) => {
                    eprintln!("[runner] APRecovery close wait detect failed: {err}");
                }
            }
            if start.elapsed() >= AP_RECOVERY_CLOSE_TIMEOUT {
                self.emit_warn("APRecovery", "等待行动力回复页面关闭超时，返回主循环重试");
                return false;
            }
        }
    }

    pub(crate) fn handle_ap_recovery(&mut self) {
        if self.config.ap_recovery_items.is_empty() {
            self.emit("APRecovery", "行动力不足且未配置自动吃苹果，停止");
            self.set_state(RunnerState::Finished);
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
                    self.tap_ap_recovery_item(item, point);
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
            self.set_state(RunnerState::Finished);
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
                    self.tap_ap_recovery_item(item, point);
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
        self.set_state(RunnerState::Finished);
    }
}
