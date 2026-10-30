//! Gesture generation for the pinned scrcpy control protocol.
//! Coordinates on the wire belong to the video, not the physical display.

use super::{TouchBackend, TouchTiming};
use crate::screen::{ScrcpyControl, ScrcpyTouchEvent};

const DRAG_PRESS_MS: u32 = 80;

pub struct ScrcpyTouchBackend {
    control: ScrcpyControl,
    screen_size: (u32, u32),
}

impl ScrcpyTouchBackend {
    pub fn new(control: ScrcpyControl, screen_size: (u32, u32)) -> Self {
        Self {
            control,
            screen_size,
        }
    }

    fn gesture(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        swipe_ms: u32,
        settle_ms: u32,
    ) -> Result<(), String> {
        let events = gesture_events(
            from,
            to,
            self.screen_size,
            self.control.size,
            swipe_ms,
            settle_ms,
        )?;
        self.control.send(&events)
    }
}

impl TouchBackend for ScrcpyTouchBackend {
    fn timing(&self) -> TouchTiming {
        TouchTiming::Streamed
    }

    fn tap(&mut self, x: u32, y: u32) -> Result<(), String> {
        self.gesture((x, y), (x, y), 0, 40)
    }

    fn swipe(&mut self, from: (u32, u32), to: (u32, u32), duration_ms: u32) -> Result<(), String> {
        self.gesture(from, to, duration_ms, 0)
    }

    fn swipe_with_settle(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        swipe_ms: u32,
        settle_ms: u32,
    ) -> Result<(), String> {
        self.gesture(from, to, swipe_ms, settle_ms)
    }

    fn name(&self) -> &'static str {
        "scrcpy"
    }
}

fn gesture_events(
    from: (u32, u32),
    to: (u32, u32),
    screen: (u32, u32),
    video: (u32, u32),
    swipe_ms: u32,
    settle_ms: u32,
) -> Result<Vec<ScrcpyTouchEvent>, String> {
    // Establish contact before dragging. Stationary taps/long presses keep
    // their requested duration, and ordinary swipes keep their existing shape.
    let press_ms = if swipe_ms > 0 && settle_ms > 0 && from != to {
        DRAG_PRESS_MS
    } else {
        0
    };
    let total_ms = swipe_ms
        .checked_add(settle_ms)
        .and_then(|duration| duration.checked_add(press_ms))
        .filter(|duration| *duration <= 60_000)
        .ok_or_else(|| "scrcpy gesture exceeds 60 seconds".to_string())?;
    if screen.0 == 0 || screen.1 == 0 || video.0 == 0 || video.1 == 0 {
        return Err("scrcpy gesture requires nonzero dimensions".into());
    }
    let project = |point: (u32, u32)| -> (u32, u32) {
        let scale = |value: u32, source: u32, target: u32| {
            ((u64::from(value.min(source - 1)) * u64::from(target) / u64::from(source)) as u32)
                .min(target - 1)
        };
        (
            scale(point.0, screen.0, video.0),
            scale(point.1, screen.1, video.1),
        )
    };
    let from = project(from);
    let to = project(to);
    let event = |action, point: (u32, u32), at_ms| ScrcpyTouchEvent {
        action,
        x: point.0,
        y: point.1,
        at_ms,
    };
    let mut events = vec![event(0, from, 0)];
    if press_ms > 0 {
        events.push(event(2, from, press_ms));
    }
    let motion_steps = if swipe_ms > 0 {
        swipe_ms.div_ceil(16).clamp(1, 600)
    } else {
        0
    };
    if swipe_ms > 0 {
        let steps = motion_steps;
        for step in 1..=steps {
            let progress = f64::from(step) / f64::from(steps);
            let interpolate = |start: u32, end: u32| {
                (f64::from(start) + (f64::from(end) - f64::from(start)) * progress).round() as u32
            };
            events.push(event(
                2,
                (interpolate(from.0, to.0), interpolate(from.1, to.1)),
                press_ms + swipe_ms * step / steps,
            ));
        }
    }
    if settle_ms > 0 {
        // Keep sending stationary samples across the hold, not just one MOVE
        // immediately before UP. This gives velocity tracking a stationary tail.
        let steps = settle_ms.div_ceil(16).clamp(1, (600 - motion_steps).max(1));
        for step in 1..=steps {
            events.push(event(2, to, press_ms + swipe_ms + settle_ms * step / steps));
        }
    }
    events.push(event(1, to, total_ms));
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swipe_scales_to_video_and_holds_before_releasing() {
        let events = gesture_events(
            (200, 800),
            (1000, 400),
            (2560, 1440),
            (1920, 1080),
            400,
            180,
        )
        .unwrap();
        let first = &events[0];
        assert_eq!(
            (first.action, first.x, first.y, first.at_ms),
            (0, 150, 600, 0)
        );
        let arrival = events
            .iter()
            .find(|event| event.at_ms == DRAG_PRESS_MS + 400)
            .unwrap();
        assert_eq!((arrival.action, arrival.x, arrival.y), (2, 750, 300));
        assert_eq!(events[events.len() - 2].at_ms, DRAG_PRESS_MS + 580);
        assert_eq!(events.last().unwrap().action, 1);
        assert!(events.windows(2).all(|pair| pair[0].at_ms <= pair[1].at_ms));
    }

    #[test]
    fn stationary_hold_and_tap_have_one_contact_cycle() {
        let events =
            gesture_events((1919, 1079), (1919, 1079), (1920, 1080), (960, 540), 0, 40).unwrap();
        assert_eq!(events.iter().filter(|event| event.action == 0).count(), 1);
        assert_eq!(events.iter().filter(|event| event.action == 1).count(), 1);
        assert!(events.iter().all(|event| (event.x, event.y) == (959, 539)));
        assert_eq!(events.last().unwrap().at_ms, 40);
        assert!(gesture_events((0, 0), (0, 0), (0, 1), (1, 1), 1, 0).is_err());
        assert!(gesture_events((0, 0), (0, 0), (1, 1), (1, 1), u32::MAX, 1).is_err());
    }

    #[test]
    fn settle_drag_establishes_contact_moves_linearly_and_samples_the_stationary_tail() {
        let events = gesture_events((0, 0), (1000, 0), (1200, 800), (1200, 800), 400, 250).unwrap();
        let arrival_ms = DRAG_PRESS_MS + 400;
        let moving: Vec<_> = events
            .iter()
            .filter(|event| event.at_ms >= DRAG_PRESS_MS && event.at_ms <= arrival_ms)
            .collect();
        assert_eq!((events[0].action, events[0].x, events[0].at_ms), (0, 0, 0));
        assert_eq!(
            (moving[0].action, moving[0].x, moving[0].at_ms),
            (2, 0, DRAG_PRESS_MS)
        );
        for event in moving {
            assert_eq!(event.x, (event.at_ms - DRAG_PRESS_MS) * 1000 / 400);
        }
        let tail: Vec<_> = events
            .iter()
            .filter(|event| event.at_ms >= arrival_ms)
            .collect();
        assert!(tail.iter().all(|event| (event.x, event.y) == (1000, 0)));
        assert!(tail
            .windows(2)
            .all(|pair| pair[1].at_ms - pair[0].at_ms <= 16));
        assert_eq!(tail.last().unwrap().at_ms, DRAG_PRESS_MS + 650);
    }

    #[test]
    fn plain_swipe_retains_linear_motion_and_long_holds_stay_within_protocol_limit() {
        let events = gesture_events((0, 0), (1000, 0), (1200, 800), (1200, 800), 400, 0).unwrap();
        for event in &events {
            assert_eq!(event.x, event.at_ms * 1000 / 400);
        }
        for (swipe_ms, settle_ms) in [(60_000, 0), (0, 60_000), (30_000 - DRAG_PRESS_MS, 30_000)] {
            let events = gesture_events(
                (0, 0),
                (1000, 0),
                (1200, 800),
                (1200, 800),
                swipe_ms,
                settle_ms,
            )
            .unwrap();
            assert!(events.len() <= 604);
            assert_eq!(events.last().unwrap().at_ms, 60_000);
        }
    }
}
