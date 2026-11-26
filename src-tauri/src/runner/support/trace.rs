use std::{thread, time::Duration};

use crate::screen::Screen;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runner) enum SearchOutcome {
    Selected,
    LeftScreen,
    Cancelled,
    Failed,
    Interrupted,
}

impl SearchOutcome {
    pub(in crate::runner) fn on_screen_exit(screen: Screen, selection_sent: bool) -> Self {
        if screen == Screen::TeamConfirm && selection_sent {
            Self::Selected
        } else {
            Self::LeftScreen
        }
    }

    pub(in crate::runner) fn on_runner_exit(failed: bool, cancelled: bool) -> Self {
        if failed {
            Self::Failed
        } else if cancelled {
            Self::Cancelled
        } else {
            Self::Interrupted
        }
    }

    #[cfg(dev)]
    fn as_str(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::LeftScreen => "left_screen",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Default)]
pub(in crate::runner) struct SupportSearchTrace {
    #[cfg(dev)]
    span: Option<tracing::Span>,
}

impl SupportSearchTrace {
    pub(in crate::runner) fn begin(&mut self) {
        #[cfg(dev)]
        if self.span.is_none() {
            let span = tracing::info_span!(
                target: "mash::support",
                parent: None,
                "support.search",
                outcome = tracing::field::Empty,
                run_id = tracing::field::Empty,
                quest_index = tracing::field::Empty,
                otel.status_code = tracing::field::Empty,
            );
            if !span.is_disabled() {
                self.span = Some(span);
            }
        }
    }

    #[cfg(dev)]
    pub(in crate::runner) fn record_quest(&self, run_id: uuid::Uuid, quest_index: u32) {
        if let Some(span) = &self.span {
            span.record("run_id", tracing::field::display(run_id));
            span.record("quest_index", quest_index);
        }
    }

    pub(in crate::runner) fn in_scope<T>(&self, operation: impl FnOnce() -> T) -> T {
        #[cfg(dev)]
        if let Some(span) = &self.span {
            return span.in_scope(operation);
        }
        operation()
    }

    pub(in crate::runner) fn is_active(&self) -> bool {
        #[cfg(dev)]
        return self.span.is_some();
        #[cfg(not(dev))]
        return false;
    }

    pub(in crate::runner) fn wait_for_poll(&self, duration: Duration) {
        #[cfg(dev)]
        if self.is_active() {
            self.in_scope(|| support_phase!("support.poll_wait", thread::sleep(duration)));
            return;
        }
        thread::sleep(duration);
    }

    pub(in crate::runner) fn finish(&mut self, outcome: SearchOutcome) {
        #[cfg(dev)]
        if let Some(span) = self.span.take() {
            span.record("outcome", outcome.as_str());
            if outcome == SearchOutcome::Failed {
                span.record("otel.status_code", "ERROR");
            }
        }
        #[cfg(not(dev))]
        let _ = outcome;
    }
}

#[cfg(dev)]
impl Drop for SupportSearchTrace {
    fn drop(&mut self) {
        self.finish(SearchOutcome::on_runner_exit(thread::panicking(), false));
    }
}

#[cfg(all(dev, test))]
mod tests {
    use opentelemetry::trace::{SpanId, Status};
    use opentelemetry_sdk::trace::{SdkTracerProvider, SpanData};

    use super::*;
    use crate::{
        runner::engine::support_search_screen_exited,
        telemetry::{self, test_support::CompletedSpans},
    };

    #[test]
    fn search_keeps_polling_and_sidecar_spans_under_one_root_through_unknown() {
        let completed = CompletedSpans::default();
        let provider = SdkTracerProvider::builder()
            .with_span_processor(completed.clone())
            .build();
        tracing::subscriber::with_default(telemetry::trace_subscriber(&provider), || {
            let mut search = SupportSearchTrace::default();
            search.wait_for_poll(Duration::ZERO);
            assert!(telemetry::sidecar_span("detect").is_disabled());

            search.begin();
            search.wait_for_poll(Duration::ZERO);
            search.in_scope(|| {
                support_phase!(
                    "support.detect_screen",
                    telemetry::sidecar_span("detect").in_scope(|| {})
                );
            });
            assert!(!support_search_screen_exited(
                Screen::SupportSelect,
                Screen::Unknown
            ));
            search.begin();
            search.in_scope(|| support_phase!("support.select", ()));
            assert!(support_search_screen_exited(
                Screen::SupportSelect,
                Screen::TeamConfirm
            ));
            search.finish(SearchOutcome::on_screen_exit(Screen::TeamConfirm, true));
            search.wait_for_poll(Duration::ZERO);
        });

        let spans = completed.0.lock().unwrap();
        assert_eq!(spans.len(), 5);
        let root = &spans[4];
        assert_eq!(root.name, "support.search");
        assert_eq!(root.parent_span_id, SpanId::INVALID);
        assert_outcome(root, "selected");
        assert_eq!(spans[0].name, "support.poll_wait");
        assert_eq!(spans[1].name, "support.sidecar");
        assert_eq!(spans[2].name, "support.detect_screen");
        assert_eq!(spans[3].name, "support.select");
        assert_eq!(spans[1].parent_span_id, spans[2].span_context.span_id());
        for child in [&spans[0], &spans[2], &spans[3]] {
            assert_eq!(child.parent_span_id, root.span_context.span_id());
        }
        for child in &spans[..4] {
            assert_eq!(child.span_context.trace_id(), root.span_context.trace_id());
        }
    }

    #[test]
    fn searches_close_with_the_observed_outcome_and_next_search_starts_a_new_trace() {
        let completed = CompletedSpans::default();
        let provider = SdkTracerProvider::builder()
            .with_span_processor(completed.clone())
            .build();
        let cases = [
            (
                SearchOutcome::on_screen_exit(Screen::TeamConfirm, false),
                "left_screen",
            ),
            (
                SearchOutcome::on_screen_exit(Screen::Battle, true),
                "left_screen",
            ),
            (SearchOutcome::on_runner_exit(false, true), "cancelled"),
            (SearchOutcome::on_runner_exit(true, false), "failed"),
            (SearchOutcome::on_runner_exit(false, false), "interrupted"),
        ];
        tracing::subscriber::with_default(telemetry::trace_subscriber(&provider), || {
            let mut search = SupportSearchTrace::default();
            for (outcome, _) in cases {
                search.begin();
                search.finish(outcome);
            }
        });

        let spans = completed.0.lock().unwrap();
        assert_eq!(spans.len(), cases.len());
        for (index, (span, (_, expected))) in spans.iter().zip(cases).enumerate() {
            assert_outcome(span, expected);
            assert_eq!(
                matches!(span.status, Status::Error { .. }),
                expected == "failed"
            );
            assert!(spans[..index]
                .iter()
                .all(|previous| previous.span_context.trace_id() != span.span_context.trace_id()));
        }
    }

    fn assert_outcome(span: &SpanData, expected: &'static str) {
        assert!(span
            .attributes
            .iter()
            .any(|attribute| attribute.key.as_str() == "outcome"
                && attribute.value == expected.into()));
    }
}
