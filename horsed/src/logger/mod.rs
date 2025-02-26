#[cfg(feature = "opentelemetry")]
mod otel;
mod ring;

use once_cell::sync::Lazy;
#[cfg(feature = "opentelemetry")]
use opentelemetry::trace::TracerProvider;
#[cfg(feature = "opentelemetry")]
use otel::*;
use ring::RingWriter;
use tracing_appender::{
    non_blocking::{NonBlocking, WorkerGuard},
    rolling::Rotation,
};
use tracing_subscriber::fmt::MakeWriter;

pub static STDOUT_GUARD: Lazy<(NonBlocking, WorkerGuard)> =
    Lazy::new(|| tracing_appender::non_blocking(std::io::stdout()));

pub static FILE_GUARD: Lazy<(NonBlocking, WorkerGuard)> = Lazy::new(|| {
    let Ok(file_appender) = tracing_appender::rolling::Builder::new()
        .rotation(Rotation::DAILY)
        .max_log_files(15)
        .filename_prefix("horsed.log")
        .build(".")
    else {
        panic!("Failed to create file appender");
    };

    tracing_appender::non_blocking(file_appender)
});

pub static RING_LOG: Lazy<RingWriter> = Lazy::new(|| RingWriter::new(30));

#[cfg(feature = "opentelemetry")]
pub static OTEL_GUARD: Lazy<OtelGuard> = Lazy::new(init_otel);

pub fn init(show_log: bool) {
    use tracing_subscriber::{filter::EnvFilter, prelude::*};

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let non_blocking = if show_log {
        STDOUT_GUARD.0.clone()
    } else {
        FILE_GUARD.0.clone()
    };

    let tee = non_blocking.make_writer().and(RING_LOG.clone());

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(tee)
        .with_thread_ids(true)
        .with_target(true)
        .with_file(false)
        .with_line_number(true);

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer);

    #[cfg(tokio_unstable)]
    {
        use std::net::SocketAddr;
        let retention = std::time::Duration::from_secs(60);
        let server_addr: SocketAddr = "0.0.0.0:6669".parse().unwrap();
        let console_layer = console_subscriber::ConsoleLayer::builder()
            // set how long the console will retain data from completed tasks
            .retention(retention)
            // set the address the server is bound to
            .server_addr(server_addr)
            // ... other configurations ...
            .spawn();

        #[cfg(feature = "opentelemetry")]
        {
            let meter = OTEL_GUARD.meter_provider.clone();
            let tracer = OTEL_GUARD.tracer_provider.tracer(env!("CARGO_PKG_NAME"));
            registry
                .with(console_layer)
                .with(MetricsLayer::new(meter))
                .with(OpenTelemetryLayer::new(tracer))
                .init();
        }

        #[cfg(not(feature = "opentelemetry"))]
        registry.with(console_layer).init();
    }

    #[cfg(not(tokio_unstable))]
    {
        #[cfg(feature = "opentelemetry")]
        {
            let meter = OTEL_GUARD.meter_provider.clone();
            let tracer = OTEL_GUARD.tracer_provider.tracer(env!("CARGO_PKG_NAME"));
            registry
                .with(MetricsLayer::new(meter))
                .with(OpenTelemetryLayer::new(tracer))
                .init();
        }

        #[cfg(not(feature = "opentelemetry"))]
        registry.init();
    }
}
