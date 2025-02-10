#[macro_use]
mod mac;
use tracing_appender::non_blocking::WorkerGuard;

pub mod command;
pub mod db;
pub mod error;
pub mod git;
pub mod ipc;
pub mod key;
pub mod options;
pub mod ssh;
pub mod ui;

pub mod prelude {
    pub(crate) use super::db::DB;
    use std::process::ExitStatus;

    pub use super::error::Error as HorseError;
    pub use super::key::{key_exists, key_init};
    pub type HorseResult<T> = Result<T, HorseError>;

    pub trait ExitOk {
        fn exit_ok(self) -> anyhow::Result<()>;
    }

    impl ExitOk for ExitStatus {
        fn exit_ok(self) -> anyhow::Result<()> {
            if self.success() {
                Ok(())
            } else {
                Err(anyhow::anyhow!("process exited with non-zero status code"))
            }
        }
    }
}

#[must_use]
pub fn init_log(show_log: bool) -> WorkerGuard {
    use tracing_subscriber::{filter::EnvFilter, prelude::*};

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let (non_blocking, _guard) = if show_log {
        tracing_appender::non_blocking(std::io::stdout())
    } else {
        let file_appender = tracing_appender::rolling::never(".", "horsed.log");
        tracing_appender::non_blocking(file_appender)
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

        tracing_subscriber::registry()
            .with(console_layer)
            .with(layer)
            .init();
    }

    #[cfg(not(tokio_unstable))]
    tracing_subscriber::registry().with(layer).init();

    _guard
}
