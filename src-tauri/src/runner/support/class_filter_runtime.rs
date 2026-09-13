//! Device interaction for the CN EXTRA-class filter dialog.

use super::*;

impl Runner {
    pub(super) fn configure_cn_extra_class_filter(&mut self, extra: ExtraClassFilter) -> bool {
        loop {
            if self.is_cancelled() {
                return false;
            }
            if !self.press_at(
                "SupportSelect",
                SUPPORT_TAB_EXTRA,
                SUPPORT_EXTRA_FILTER_LONG_PRESS_MS,
            ) {
                return false;
            }
            let wait = self.wait_for_support_extra_filter_dialog_once(true);
            if wait == SupportExtraFilterDialogWait::Matched {
                break;
            }
            if !should_retry_support_extra_filter_dialog(wait) {
                return false;
            }
            self.emit(
                "SupportSelect",
                "EXTRA 职阶筛选弹窗未出现，重新长按 EXTRA 页签",
            );
        }
        thread::sleep(SUPPORT_EXTRA_FILTER_ACTION_SETTLE);

        if !self.tap_at("SupportSelect", SUPPORT_EXTRA_FILTER_RESET_BUTTON) {
            return false;
        }
        thread::sleep(SUPPORT_EXTRA_FILTER_ACTION_SETTLE);
        if !self.tap_at("SupportSelect", extra.point) {
            return false;
        }
        thread::sleep(SUPPORT_EXTRA_FILTER_ACTION_SETTLE);
        // `adb input tap` is effectively a zero-duration press. On this
        // modal it can dismiss on DOWN and leak the UP into the support row
        // underneath, selecting a servant before OCR runs. A 100-ms
        // stationary press matches a human tap and is consumed by the modal.
        if !self.press_at(
            "SupportSelect",
            SUPPORT_EXTRA_FILTER_CONFIRM_BUTTON,
            SUPPORT_EXTRA_FILTER_CONFIRM_PRESS_MS,
        ) {
            return false;
        }
        self.wait_for_support_extra_filter_dialog(false)
    }

    fn wait_for_support_extra_filter_dialog(&mut self, expected_visible: bool) -> bool {
        match self.wait_for_support_extra_filter_dialog_once(expected_visible) {
            SupportExtraFilterDialogWait::Matched => true,
            SupportExtraFilterDialogWait::Aborted => false,
            SupportExtraFilterDialogWait::TimedOut => {
                let state = if expected_visible { "打开" } else { "关闭" };
                self.fail_action(
                    "SupportSelect",
                    &format!("等待 EXTRA 职阶筛选弹窗{state}"),
                    "超时".into(),
                );
                false
            }
        }
    }

    fn wait_for_support_extra_filter_dialog_once(
        &mut self,
        expected_visible: bool,
    ) -> SupportExtraFilterDialogWait {
        let deadline = Instant::now() + SUPPORT_EXTRA_FILTER_DIALOG_TIMEOUT;
        loop {
            if self.is_cancelled() {
                return SupportExtraFilterDialogWait::Aborted;
            }
            match self.sidecar().find_element_by_name(
                None,
                SUPPORT_SELECT_SCREEN,
                SUPPORT_EXTRA_FILTER_DIALOG_ELEMENT,
            ) {
                Ok(matched) if matched.found == expected_visible => {
                    return SupportExtraFilterDialogWait::Matched;
                }
                Ok(_) => {}
                Err(err) => {
                    self.fail_action("SupportSelect", "识别 EXTRA 职阶筛选弹窗", err);
                    return SupportExtraFilterDialogWait::Aborted;
                }
            }
            if Instant::now() >= deadline {
                return SupportExtraFilterDialogWait::TimedOut;
            }
            thread::sleep(SUPPORT_EXTRA_FILTER_DIALOG_POLL);
        }
    }
}
