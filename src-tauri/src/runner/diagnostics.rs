//! Runner-only recognition logs and opt-in battle screenshot capture.

use super::*;

pub(crate) fn format_shadow_battle_model_digits(nps: &[NoblePhantasmMatch]) -> Option<String> {
    let mut slots: Vec<&NoblePhantasmMatch> = nps.iter().collect();
    slots.sort_by_key(|np| np.slot);
    if slots.len() != 3 {
        return None;
    }
    let digit_summary = slots[0]
        .turn_count_model_label
        .as_deref()
        .and_then(|turn_count| {
            let gauges = slots
                .iter()
                .enumerate()
                .map(|(index, np)| {
                    let digits = np.gauge_digit_model_labels.as_ref()?;
                    if digits.len() != 3 {
                        return None;
                    }
                    Some(format!(
                        "宝具{}[百={} 十={} 个={}]",
                        index + 1,
                        digits[0],
                        digits[1],
                        digits[2]
                    ))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(format!(
                "影子模式数字验证：回合={turn_count}；{}",
                gauges.join("；")
            ))
        });
    let sequence_summary = slots[0].turn_sequence_value.as_deref().map(|turn_count| {
        let gauges = slots
            .iter()
            .enumerate()
            .map(|(index, np)| {
                format!(
                    "宝具{}={}({:.3}{})",
                    index + 1,
                    np.gauge_sequence_value.as_deref().unwrap_or("未识别"),
                    np.gauge_sequence_confidence.unwrap_or(0.0),
                    if np.gauge_sequence_accepted == Some(true) {
                        ""
                    } else {
                        " 拒绝"
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("；");
        format!("CNN-CTC 数字验证：回合={turn_count}；{gauges}")
    });
    match (digit_summary, sequence_summary) {
        (Some(digit), Some(sequence)) => Some(format!("{digit}；{sequence}")),
        (Some(digit), None) => Some(digit),
        (None, Some(sequence)) => Some(sequence),
        (None, None) => None,
    }
}

pub(crate) fn battle_before_attack_screenshot_dir_in_root(root: &Path, server: Server) -> PathBuf {
    root.join("debug")
        .join("battle-before-attack")
        .join(server.dir_token())
}

pub(crate) fn battle_before_attack_screenshot_filename(
    timestamp: std::time::SystemTime,
    server: Server,
    completed_mission_runs: u32,
    scene_index: usize,
    turn_index: usize,
) -> String {
    let millis = timestamp
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!(
        "battle-before-attack-{}-{millis:013}-run{:04}-scene{:02}-turn{:02}.png",
        server.dir_token(),
        completed_mission_runs + 1,
        scene_index + 1,
        turn_index + 1,
    )
}

impl Runner {
    pub(crate) fn emit_np_recognition_diagnostics(&self, screen: &str, nps: &[NoblePhantasmMatch]) {
        if let Some(message) = format_shadow_battle_model_digits(nps) {
            self.emit(screen, &message);
        }
    }

    /// Called after pre-attack NP sampling and before tapping the Attack button.
    pub(crate) fn capture_before_attack_if_enabled(&mut self) {
        if !self.config.auto_capture_battle_before_attack {
            return;
        }
        match self.capture_battle_before_attack_screenshot() {
            Ok(path) => self.emit_debug(
                "Battle",
                &format!("点击攻击前截图已保存: {}", path.display()),
            ),
            Err(err) => self.emit_warn(
                "Battle",
                &format!("点击攻击前截图保存失败，继续攻击: {err}"),
            ),
        }
    }

    fn capture_battle_before_attack_screenshot(&mut self) -> Result<PathBuf, String> {
        let dir = battle_before_attack_screenshot_dir_in_root(
            &crate::app_data_dir(&self.app_handle),
            self.server,
        );
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("创建点击攻击前截图目录失败: {err}"))?;
        let path = dir.join(battle_before_attack_screenshot_filename(
            std::time::SystemTime::now(),
            self.server,
            self.completed_mission_runs,
            self.battle.current_scene_index,
            self.battle.current_turn_index,
        ));
        let png = self
            .sidecar()
            .get_frame_png(0.0)
            .map_err(|err| format!("获取点击攻击前视频帧失败: {err}"))?;
        std::fs::write(&path, png).map_err(|err| format!("写入点击攻击前截图失败: {err}"))?;
        Ok(path)
    }
}
