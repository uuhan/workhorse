#[macro_use]
mod mac;

pub mod command;
pub mod db;
pub mod error;
pub mod git;
pub mod ipc;
pub mod key;
pub mod logger;
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
