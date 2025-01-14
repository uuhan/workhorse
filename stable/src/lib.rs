//! 一些公用的模块
//!
pub mod task;
pub mod prelude {
    #[rustfmt::skip]
    pub use super::task::{
        SpawnEssentialTaskHandle,
        SpawnTaskHandle,
        TaskExecutor,
        TaskManager,
        handle,
    };
}
