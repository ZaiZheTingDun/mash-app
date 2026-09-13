use super::*;

pub(crate) fn np_card_read_complete(nps: &[NoblePhantasmMatch]) -> bool {
    nps.len() == 3 && nps.iter().all(|np| np.card_ready.is_some())
}

pub(crate) fn np_gauge_read_complete(nps: &[NoblePhantasmMatch]) -> bool {
    nps.len() == 3 && nps.iter().all(|np| np.np_glow_score.is_some())
}

pub(crate) fn np_gauge_digit_read_complete(nps: &[NoblePhantasmMatch]) -> bool {
    nps.len() == 3 && nps.iter().all(|np| np.gauge_hundreds_visible.is_some())
}

pub(crate) fn apply_np_detection_mode(
    nps: &mut [NoblePhantasmMatch],
    mode: NoblePhantasmDetectionMode,
) {
    match mode {
        NoblePhantasmDetectionMode::Card => {
            for np in nps {
                if let Some(card_ready) = np.card_ready {
                    np.ready = card_ready;
                    np.ready_source = Some("card".into());
                }
            }
        }
        NoblePhantasmDetectionMode::Gauge => {}
        NoblePhantasmDetectionMode::GaugeBeforeAttack => {}
    }
}

pub(crate) fn reads_np_gauge_before_attack(mode: NoblePhantasmDetectionMode) -> bool {
    matches!(mode, NoblePhantasmDetectionMode::GaugeBeforeAttack)
}

/// Aggregate one-second NP-gauge samples by slot.
///
/// The median rejects a single bright frame caused by a dialogue or visual
/// effect. The representative record is updated with the median score so its
/// readiness fields remain consistent with the value the runner uses.
pub(crate) fn aggregate_np_gauge_samples(
    samples: &[Vec<NoblePhantasmMatch>],
) -> Vec<NoblePhantasmMatch> {
    let mut slot_ids = Vec::new();
    for sample in samples {
        for slot in sample {
            if !slot_ids.contains(&slot.slot) {
                slot_ids.push(slot.slot);
            }
        }
    }
    slot_ids.sort_unstable();

    slot_ids
        .into_iter()
        .filter_map(|slot_id| {
            let mut readings: Vec<(f64, NoblePhantasmMatch)> = samples
                .iter()
                .filter_map(|sample| {
                    sample
                        .iter()
                        .find(|slot| slot.slot == slot_id)
                        .and_then(|slot| slot.np_glow_score.map(|score| (score, slot.clone())))
                })
                .collect();
            if readings.len() < NP_GAUGE_MIN_VALID_SAMPLES {
                return None;
            }

            readings.sort_by(|left, right| left.0.total_cmp(&right.0));
            let median = if readings.len() % 2 == 1 {
                readings[readings.len() / 2].0
            } else {
                let upper = readings.len() / 2;
                (readings[upper - 1].0 + readings[upper].0) / 2.0
            };
            let mut representative = readings[readings.len() / 2].1.clone();
            let ready = median >= NP_GAUGE_GLOW_READY_THRESHOLD;
            representative.ready = ready;
            representative.ready_source = Some("glow".into());
            representative.np_glow_score = Some(median);
            representative.np_glow_ready = Some(ready);
            Some(representative)
        })
        .collect()
}

/// Aggregate one-second NP-gauge digit samples by majority vote of the
/// fixed hundreds slot. A visible hundreds digit means the gauge is at least
/// 100%, which is the only distinction the runner needs for NP readiness.
pub(crate) fn aggregate_np_gauge_digit_samples(
    samples: &[Vec<NoblePhantasmMatch>],
) -> Vec<NoblePhantasmMatch> {
    let mut slot_ids = Vec::new();
    for sample in samples {
        for slot in sample {
            if !slot_ids.contains(&slot.slot) {
                slot_ids.push(slot.slot);
            }
        }
    }
    slot_ids.sort_unstable();

    slot_ids
        .into_iter()
        .filter_map(|slot_id| {
            let readings: Vec<(bool, NoblePhantasmMatch)> = samples
                .iter()
                .filter_map(|sample| {
                    sample
                        .iter()
                        .find(|slot| slot.slot == slot_id)
                        .and_then(|slot| {
                            slot.gauge_hundreds_visible
                                .map(|visible| (visible, slot.clone()))
                        })
                })
                .collect();
            if readings.len() < NP_GAUGE_MIN_VALID_SAMPLES {
                return None;
            }

            let visible_count = readings.iter().filter(|(visible, _)| *visible).count();
            let ready = visible_count * 2 > readings.len();
            let mut representative = readings[readings.len() / 2].1.clone();
            representative.ready = ready;
            representative.ready_source = Some("gaugeDigits".into());
            representative.gauge_hundreds_visible = Some(ready);
            representative.gauge_digit_count = Some(if ready { 3 } else { 2 });
            Some(representative)
        })
        .collect()
}

pub(crate) fn np_gauge_sample_window_complete(elapsed: Duration, sample_count: usize) -> bool {
    elapsed >= NP_GAUGE_SAMPLE_WINDOW && sample_count >= NP_GAUGE_MIN_VALID_SAMPLES
}

pub(crate) fn np_condition_matches(
    condition: &crate::AdvancedNpSlotCondition,
    nps: &[NoblePhantasmMatch],
) -> bool {
    let Some(index) = parse_index(&condition.servant, "servant_") else {
        return false;
    };
    nps.iter()
        .find(|np| np.slot as usize == index)
        .map(|np| np.ready == condition.ready)
        .unwrap_or(false)
}
