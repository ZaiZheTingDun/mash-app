//! Device probes, input primitives, and runner event emission.

use super::*;

impl CraftEssenceEnhancementRunner {
    pub(super) fn detect_screen(&mut self) -> Screen {
        let mut snapshot = ProbeSnapshot::default();
        for probe in PROBES {
            if self.probe(probe.element) {
                snapshot.found.push(probe.key);
            }
        }
        if self.probe("button_enhancement_ce_select_ce_desc") {
            snapshot.found.push("button_enhancement_ce_select_ce_desc");
        }
        classify_screen(&snapshot)
    }

    pub(super) fn probe(&mut self, element: &str) -> bool {
        self.sidecar()
            .find_element_by_name(None, SCREEN_NAME, element)
            .map(|result| result.found)
            .unwrap_or(false)
    }

    pub(super) fn probe_score(&mut self, element: &str) -> f64 {
        self.sidecar()
            .find_element_by_name(None, SCREEN_NAME, element)
            .map(|result| result.score)
            .unwrap_or(0.0)
    }

    pub(super) fn tap_probe_or_point(
        &mut self,
        screen: &str,
        element: &str,
        fallback: Point,
    ) -> bool {
        let point = self
            .sidecar()
            .find_element_by_name(None, SCREEN_NAME, element)
            .ok()
            .and_then(|result| result.found.then_some(Point::new(result.x, result.y)))
            .unwrap_or(fallback);
        self.tap_at(screen, point)
    }

    pub(super) fn tap_at(&mut self, screen: &str, point: Point) -> bool {
        let (px, py) = point.to_physical(self.screen_w, self.screen_h);
        match self.touch.tap(px, py) {
            Ok(()) => true,
            Err(err) => {
                self.fail(screen, format!("点击失败: {err}"));
                false
            }
        }
    }

    pub(super) fn swipe_at(
        &mut self,
        screen: &str,
        from: Point,
        to: Point,
        duration_ms: u32,
    ) -> bool {
        let (from_x, from_y) = from.to_physical(self.screen_w, self.screen_h);
        let (to_x, to_y) = to.to_physical(self.screen_w, self.screen_h);
        match self
            .touch
            .swipe_with_settle((from_x, from_y), (to_x, to_y), duration_ms, 180)
        {
            Ok(()) => true,
            Err(err) => {
                self.fail(screen, format!("滑动失败: {err}"));
                false
            }
        }
    }

    pub(super) fn sidecar(&mut self) -> &mut SidecarClient {
        self.sidecar
            .as_mut()
            .expect("CE enhancement sidecar missing")
    }

    pub(super) fn read_material_selected_count(&mut self) -> Result<u8, String> {
        let ocr = self
            .sidecar()
            .ocr_region(None, MATERIAL_COUNTER_REGION)
            .map_err(|error| format!("读取页面素材计数失败: {error}"))?;
        let selected = parse_selected_count(&ocr.full_text)
            .ok_or_else(|| format!("无法安全识别页面素材计数: {:?}", ocr.full_text))?;
        u8::try_from(selected)
            .ok()
            .filter(|count| *count <= PACKET_BATCH_SIZE)
            .ok_or_else(|| format!("页面素材计数超出安全范围: {selected}/20"))
    }

    pub(super) fn read_material_level_max_reached(&mut self) -> Result<bool, String> {
        let ocr = self
            .sidecar()
            .ocr_region(None, ITEM_GRID_REGION)
            .map_err(|error| format!("确认素材选择是否达到等级上限失败: {error}"))?;
        Ok(material_level_max_text_detected(&ocr.full_text))
    }

    pub(super) fn transition(&self, event: LifecycleEvent) {
        let mut state = self.state.lock().unwrap();
        *state = lifecycle_transition(state.clone(), event);
    }

    pub(super) fn fail(&self, screen: &str, message: String) {
        self.transition(LifecycleEvent::Failed {
            message: message.clone(),
        });
        self.emit(screen, &message);
    }

    pub(super) fn emit(&self, screen: &str, message: &str) {
        let (state, status) = {
            let state = self.state.lock().unwrap();
            (format!("{:?}", *state), state.status())
        };
        let _ = self.app_handle.emit(
            EVENT_NAME,
            CraftEssenceEnhancementAutomationEvent {
                state,
                status,
                current_screen: screen.into(),
                message: message.into(),
                level: LogLevel::Info,
            },
        );
    }
}
