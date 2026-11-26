#[cfg(dev)]
use std::time::Duration;

#[cfg(dev)]
use opentelemetry::trace::TracerProvider;
#[cfg(dev)]
use opentelemetry_otlp::{Protocol, SpanExporter, WithExportConfig};
#[cfg(dev)]
use opentelemetry_sdk::{trace::SdkTracerProvider, Resource};
#[cfg(dev)]
use tracing_subscriber::{filter::filter_fn, prelude::*};

#[cfg(dev)]
pub(crate) const SUPPORT_TARGET: &str = "mash::support";
#[cfg(dev)]
pub(crate) const BATTLE_TARGET: &str = "mash::battle";
#[cfg(dev)]
const TRACES_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT";

fn enabled_endpoint(endpoint: Option<&str>) -> Option<&str> {
    if !cfg!(dev) {
        return None;
    }
    endpoint.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(dev)]
pub(crate) struct Telemetry(SdkTracerProvider);

#[cfg(dev)]
impl Drop for Telemetry {
    fn drop(&mut self) {
        // Give a normal app exit a chance to send completed traces.
        let _ = self.0.shutdown_with_timeout(Duration::from_secs(2));
    }
}

#[cfg(dev)]
pub(crate) fn init() -> Option<Telemetry> {
    let configured = std::env::var(TRACES_ENDPOINT).ok();
    let endpoint = enabled_endpoint(configured.as_deref())?;
    match install(endpoint) {
        Ok(telemetry) => Some(telemetry),
        Err(error) => {
            eprintln!("[telemetry] 性能追踪初始化失败: {error}");
            None
        }
    }
}

#[cfg(dev)]
fn install(endpoint: &str) -> Result<Telemetry, Box<dyn std::error::Error>> {
    let exporter = SpanExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(endpoint)
        .with_timeout(Duration::from_secs(2))
        .build()?;
    // Export on the SDK's worker thread so an unavailable dashboard cannot
    // delay the Runner or any sidecar requests.
    let provider = SdkTracerProvider::builder()
        .with_resource(Resource::builder().with_service_name("mash").build())
        .with_batch_exporter(exporter)
        .build();
    tracing::subscriber::set_global_default(trace_subscriber(&provider))?;
    Ok(Telemetry(provider))
}

#[cfg(dev)]
pub(crate) fn trace_subscriber(provider: &SdkTracerProvider) -> impl tracing::Subscriber {
    let layer = tracing_opentelemetry::layer()
        .with_tracer(provider.tracer("mash.runner"))
        .with_filter(filter_fn(|metadata| {
            metadata.is_span() && matches!(metadata.target(), SUPPORT_TARGET | BATTLE_TARGET)
        }));
    tracing_subscriber::registry().with(layer)
}

#[cfg(dev)]
pub(crate) fn sidecar_span(command: &str) -> tracing::Span {
    let current = tracing::Span::current();
    match current.metadata().map(|metadata| metadata.target()) {
        Some(SUPPORT_TARGET) => tracing::info_span!(
            target: SUPPORT_TARGET, "support.sidecar", command, response_ok = tracing::field::Empty
        ),
        Some(BATTLE_TARGET) => tracing::info_span!(
            target: BATTLE_TARGET, "battle.sidecar", command, response_ok = tracing::field::Empty
        ),
        _ => tracing::Span::none(),
    }
}

#[cfg(all(dev, test))]
pub(crate) mod test_support {
    use std::sync::{Arc, Mutex};

    use opentelemetry::Context;
    use opentelemetry_sdk::{
        error::OTelSdkResult,
        trace::{Span, SpanData, SpanProcessor},
    };

    #[derive(Clone, Debug, Default)]
    pub(crate) struct CompletedSpans(pub(crate) Arc<Mutex<Vec<SpanData>>>);

    impl SpanProcessor for CompletedSpans {
        fn on_start(&self, _: &mut Span, _: &Context) {}

        fn on_end(&self, span: SpanData) {
            self.0.lock().unwrap().push(span);
        }

        fn force_flush(&self) -> OTelSdkResult {
            Ok(())
        }

        fn shutdown_with_timeout(&self, _: std::time::Duration) -> OTelSdkResult {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::enabled_endpoint;

    #[test]
    fn only_tauri_dev_with_an_explicit_endpoint_enables_export() {
        let endpoint = "http://localhost:4318/v1/traces";
        assert_eq!(enabled_endpoint(None), None);
        assert_eq!(enabled_endpoint(Some("  ")), None);
        assert_eq!(
            enabled_endpoint(Some(endpoint)),
            cfg!(dev).then_some(endpoint)
        );
    }
}
