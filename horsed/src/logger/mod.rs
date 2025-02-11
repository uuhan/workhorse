use once_cell::sync::Lazy;
#[cfg(feature = "opentelemetry")]
use opentelemetry::trace::TracerProvider;
use tracing_appender::{
    non_blocking::{NonBlocking, WorkerGuard},
    rolling::Rotation,
};
#[cfg(feature = "opentelemetry")]
mod otel;
#[cfg(feature = "opentelemetry")]
use otel::*;

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

    let layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_thread_ids(true)
        .with_target(true)
        .with_file(false)
        .with_line_number(true)
        .with_filter(env_filter);

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
            let tracer = OTEL_GUARD.tracer_provider.tracer("tracing-otel-subscriber");
            tracing_subscriber::registry()
                .with(console_layer)
                .with(layer)
                .with(MetricsLayer::new(meter))
                .with(OpenTelemetryLayer::new(tracer))
                .init();
        }

        #[cfg(not(feature = "opentelemetry"))]
        tracing_subscriber::registry()
            .with(console_layer)
            .with(layer)
            .init();
    }

    #[cfg(not(tokio_unstable))]
    {
        #[cfg(feature = "opentelemetry")]
        {
            let meter = OTEL_GUARD.meter_provider.clone();
            let tracer = OTEL_GUARD.tracer_provider.tracer("tracing-otel-subscriber");
            tracing_subscriber::registry()
                .with(layer)
                .with(MetricsLayer::new(meter))
                .with(OpenTelemetryLayer::new(tracer))
                .init();
        }

        #[cfg(not(feature = "opentelemetry"))]
        tracing_subscriber::registry().with(layer).init();
    }
}
