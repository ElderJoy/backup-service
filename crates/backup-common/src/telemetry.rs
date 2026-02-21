//! OpenTelemetry distributed tracing setup with Jaeger export via OTLP.
//!
//! When `OTEL_EXPORTER_OTLP_ENDPOINT` is set (e.g. `http://jaeger:4317`),
//! traces are exported to Jaeger. Otherwise, tracing falls back to stdout only.

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize the full observability stack:
/// - `tracing-subscriber` with env filter for structured logs
/// - OpenTelemetry trace export to Jaeger (when configured)
pub fn init_telemetry(service_name: &str) -> Option<opentelemetry_sdk::trace::TracerProvider> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=debug,sqlx=warn"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true);

    let otel_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();

    if let Some(endpoint) = otel_endpoint {
        match setup_otel_provider(service_name, &endpoint) {
            Ok(provider) => {
                let tracer = provider.tracer(service_name.to_string());
                let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt_layer)
                    .with(otel_layer)
                    .init();

                tracing::info!(
                    endpoint = %endpoint,
                    "OpenTelemetry tracing initialized (exporting to Jaeger)"
                );
                return Some(provider);
            }
            Err(e) => {
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt_layer)
                    .init();

                tracing::warn!(
                    error = %e,
                    "Failed to initialize OpenTelemetry, using stdout tracing only"
                );
            }
        }
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();

        tracing::info!("OpenTelemetry not configured (set OTEL_EXPORTER_OTLP_ENDPOINT to enable)");
    }

    None
}

fn setup_otel_provider(
    service_name: &str,
    endpoint: &str,
) -> Result<opentelemetry_sdk::trace::TracerProvider, Box<dyn std::error::Error>> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()?;

    let resource = opentelemetry_sdk::Resource::new(vec![
        KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
            service_name.to_string(),
        ),
        KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_VERSION,
            env!("CARGO_PKG_VERSION"),
        ),
    ]);

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(resource)
        .build();

    Ok(provider)
}

/// Shut down the OpenTelemetry provider, flushing pending spans.
pub fn shutdown_telemetry(provider: Option<opentelemetry_sdk::trace::TracerProvider>) {
    if let Some(provider) = provider {
        if let Err(e) = provider.shutdown() {
            tracing::error!(error = %e, "Failed to shut down OpenTelemetry provider");
        } else {
            tracing::info!("OpenTelemetry provider shut down, spans flushed");
        }
    }
}
