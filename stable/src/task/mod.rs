//! 异步任务管理器
use futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use parking_lot::{Condvar, Mutex};
use std::future::Future;
use std::pin::Pin;
use tokio::runtime::Runtime;

pub mod executor;
pub mod manager;
pub mod signal;

pub use executor::TaskExecutor;
pub use manager::{SpawnTaskHandle, TaskManager};

pub(self) type TracingUnboundedSender<T> = UnboundedSender<T>;
pub(self) type TracingUnboundedReceiver<T> = UnboundedReceiver<T>;

pub(self) type JoinFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
pub(self) type SomeFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// 任务运行限制，比如限制同时运行的任务数量
pub(self) struct TaskCondition(Mutex<usize>, Condvar);

impl TaskCondition {
    pub fn new() -> Self {
        TaskCondition(Mutex::new(0), Condvar::new())
    }

    /// 运行数加1
    pub fn inc(&self) {
        let mut count = self.0.lock();
        *count = *count + 1;
        self.1.notify_all();
    }

    /// 运行数减1
    pub fn dec(&self) {
        let mut count = self.0.lock();
        *count = *count - 1;
        self.1.notify_all();
    }

    /// 检查运行条件, 不满足则同步等待
    pub fn check(&self, upper: usize) {
        let mut count = self.0.lock();
        while *count >= upper {
            self.1.wait(&mut count)
        }
    }
}

pub fn tracing_unbounded<T>() -> (TracingUnboundedSender<T>, TracingUnboundedReceiver<T>) {
    mpsc::unbounded()
}

pub fn build_multi_thread() -> Runtime {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    tokio::runtime::Builder::new_multi_thread()
        .on_thread_start(|| {
            let idx = COUNT.fetch_add(1, Ordering::SeqCst);
            tracing::trace!("#{} tokio thread started", idx + 1);
        })
        .on_thread_stop(|| {
            let idx = COUNT.fetch_sub(1, Ordering::SeqCst);
            tracing::trace!("#{} tokio thread stopped", idx);
        })
        .enable_all()
        .build()
        .unwrap()
}
