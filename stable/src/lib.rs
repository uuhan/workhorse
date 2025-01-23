//! 一些公用的模块
//!
pub mod data;
pub mod task;
pub mod buffer;

pub mod prelude {
    #[rustfmt::skip]
    pub use super::task::{
        SpawnEssentialTaskHandle,
        SpawnTaskHandle,
        TaskExecutor,
        TaskManager,
        handle,
    };
    pub use super::data::*;
}
