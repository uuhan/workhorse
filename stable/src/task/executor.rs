use super::*;
use std::{future::Future, sync::Arc};

#[derive(PartialEq)]
pub enum TaskType {
    Async,
    Block,
}

#[rustfmt::skip]
#[derive(Clone)]
pub struct TaskExecutor(
    Arc<
        dyn Fn(SomeFuture<()>, TaskType)
            -> JoinFuture<()> + Send + Sync
    >
);

impl std::fmt::Debug for TaskExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("TaskExecutor")
            .field("executor", &"Fn(...)")
            .finish()
    }
}

impl<F, FUT> std::convert::From<F> for TaskExecutor
where
    F: Fn(SomeFuture<()>, TaskType) -> FUT + Send + Sync + 'static,
    FUT: Future<Output = ()> + Send + 'static,
{
    fn from(func: F) -> Self {
        Self(Arc::new(move |fut, tt| Box::pin(func(fut, tt))))
    }
}

impl TaskExecutor {
    /// 启动一个指定类型的异步任务
    pub fn spawn(&self, fut: SomeFuture<()>, tt: TaskType) -> JoinFuture<()> {
        self.0(fut, tt)
    }
}
