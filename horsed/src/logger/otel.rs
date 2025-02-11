//! Tracing Opentelemetry Exporter
//!
//! ## OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT";
//!
//! Target to which the exporter is going to send spans, defaults to https://localhost:4317/v1/traces.
//! Learn about the relationship between this constant and default/metrics/logs at
//! <https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/protocol/exporter.md#endpoint-urls-for-otlphttp>
//!
//! ## OTEL_EXPORTER_OTLP_TRACES_TIMEOUT: &str = "OTEL_EXPORTER_OTLP_TRACES_TIMEOUT";
//!
//! Max waiting time for the backend to process each spans batch, defaults to 10s.
//!
//! ## OTEL_EXPORTER_OTLP_TRACES_COMPRESSION: &str = "OTEL_EXPORTER_OTLP_TRACES_COMPRESSION";
//!
//! Compression algorithm to use, defaults to none.
//!
//! ## OTEL_EXPORTER_OTLP_TRACES_HEADERS: &str = "OTEL_EXPORTER_OTLP_TRACES_HEADERS";
//!
//! Key-value pairs to be used as headers associated with gRPC or HTTP requests
//! for sending spans.
//! Example: `k1=v1,k2=v2`
//! Note: this is only supported for HTTP.

use opentelemetry::{global, KeyValue};
use opentelemetry_sdk::{
    metrics::{MeterProviderBuilder, PeriodicReader, SdkMeterProvider, Temporality},
    runtime::Tokio,
    trace::{RandomIdGenerator, Sampler, TracerProvider},
    Resource,
};
use opentelemetry_semantic_conventions::{
    attribute::{DEPLOYMENT_ENVIRONMENT_NAME, SERVICE_NAME, SERVICE_VERSION},
    SCHEMA_URL,
};
use std::time::Duration;
pub use tracing_opentelemetry::{MetricsLayer, OpenTelemetryLayer};

// Create a Resource that captures information about the entity for which telemetry is recorded.
fn resource() -> Resource {
    Resource::from_schema_url(
        [
            KeyValue::new(SERVICE_NAME, env!("CARGO_PKG_NAME")),
            KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
            KeyValue::new(DEPLOYMENT_ENVIRONMENT_NAME, "develop"),
        ],
        SCHEMA_URL,
    )
}

// Construct MeterProvider for MetricsLayer
fn init_meter_provider() -> SdkMeterProvider {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_temporality(Temporality::default())
        .build()
        .unwrap();

    let reader = PeriodicReader::builder(exporter, Tokio)
        .with_interval(Duration::from_secs(30))
        .build();

    // For debugging in development
    let stdout_reader =
        PeriodicReader::builder(opentelemetry_stdout::MetricExporter::default(), Tokio).build();

    let meter_provider = MeterProviderBuilder::default()
        .with_resource(resource())
        .with_reader(reader)
        .with_reader(stdout_reader)
        .build();

    global::set_meter_provider(meter_provider.clone());

    meter_provider
}

// Construct TracerProvider for OpenTelemetryLayer
fn init_tracer_provider() -> TracerProvider {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()
        .unwrap();

    TracerProvider::builder()
        // Customize sampling strategy
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            1.0,
        ))))
        // If export trace to AWS X-Ray, you can use XrayIdGenerator
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource())
        .with_batch_exporter(exporter, Tokio)
        .build()
}

pub fn init_otel() -> OtelGuard {
    OtelGuard {
        tracer_provider: init_tracer_provider(),
        meter_provider: init_meter_provider(),
    }
}

#[must_use]
pub struct OtelGuard {
    pub tracer_provider: TracerProvider,
    pub meter_provider: SdkMeterProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(err) = self.tracer_provider.shutdown() {
            eprintln!("{err:?}");
        }
        if let Err(err) = self.meter_provider.shutdown() {
            eprintln!("{err:?}");
        }
    }
}

#[tracing::instrument]
async fn foo() {
    tracing::info!(
        monotonic_counter.foo = 1_u64,
        key_1 = "bar",
        key_2 = 10,
        "handle foo",
    );

    tracing::info!(histogram.baz = 10, "histogram example");
}
