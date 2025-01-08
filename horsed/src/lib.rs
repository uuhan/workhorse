#![feature(exit_status_error)]
pub mod command;
pub mod db;
pub mod error;
pub mod git;
pub mod key;
pub mod repo;
pub mod ui;

pub mod prelude {
    pub use super::error::Error as HorseError;
    pub(crate) use super::key::KEY;
    pub type HorseResult<T> = Result<T, HorseError>;
}
