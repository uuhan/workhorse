pub mod command;
pub mod db;
pub mod error;
pub mod ui;

pub mod prelude {
    pub use super::error::Error as HorseError;
    pub type HorseResult<T> = Result<T, HorseError>;
}
