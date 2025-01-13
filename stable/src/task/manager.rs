use super::{
    executor::{TaskExecutor, TaskType},
    signal::{signal, Exit, Signal},
    tracing_unbounded, JoinFuture, TaskCondition, TracingUnboundedReceiver, TracingUnboundedSender,
};
use anyhow::Result;
use core::panic;
use futures::{
    future::{join_all, pending, select, Either},
    sink::SinkExt,
    Future, FutureExt, StreamExt,
};
use std::pin::Pin;
use std::sync::Arc;

pub struct TaskHandler {
    condition: Arc<TaskCondition>,
    pool_size: Option<usize>,
    exiting: bool,
}

#[derive(Clone)]
pub struct SpawnTaskHandle {
    on_exit: Exit,
    executor: TaskExecutor,
    task_notifier: TracingUnboundedSender<JoinFuture<()>>,
    condition: Arc<TaskCondition>,
    pool_size: Option<usize>,
}

#[derive(Clone)]
pub struct SpawnEssentialTaskHandle {
    pub(self) essential_failed_tx: TracingUnboundedSender<()>,
    pub(self) inner: SpawnTaskHandle,
}

impl SpawnTaskHandle {
    pub fn spawn<T>(&self, task: T) -> TaskHandler
    where
        T: Future<Output = Result<()>> + Send + 'static,
    {
        self.spawn_inner::<T>(task, TaskType::Async)
    }

    pub fn spawn_blocking<T>(&self, task: T) -> TaskHandler
    where
        T: Future<Output = Result<()>> + Send + 'static,
    {
        self.spawn_inner::<T>(task, TaskType::Block)
    }
    fn spawn_inner<T>(&self, task: T, tt: TaskType) -> TaskHandler
    where
        T: Future<Output = Result<()>> + Send + 'static,
    {
        // 任务管理器关闭时，不允许新任务被创建
        if self.task_notifier.is_closed() {
            tracing::warn!("Attempt to spawn a new task has been prevented in closed phase.");

            return TaskHandler {
                condition: self.condition.clone(),
                pool_size: self.pool_size.clone(),
                exiting: true,
            };
        }

        // 任务数加1
        self.condition.inc();
        let cd = self.condition.clone();

        let mut on_exit = self.on_exit.clone();

        let task = async move {
            let task = { panic::AssertUnwindSafe(task).catch_unwind() };

            futures::pin_mut!(task);

            match select(&mut on_exit, &mut task).await {
                // 接收到退出信号, 任务退出
                Either::Left(_) => {}
                Either::Right((Err(pc), _)) => {
                    if let Some(message) = pc.downcast_ref::<&str>() {
                        tracing::error!("Panic occurred: {}", message);
                    } else if let Some(message) = pc.downcast_ref::<String>() {
                        tracing::error!("Panic occurred: {}", message);
                    } else {
                        tracing::error!("Panic occurred: {:#?}", pc);
                    }

                    // std::panic::resume_unwind(pc);
                }
                Either::Right((Ok(_), _)) => {
                    // 任务正常退出
                }
                Either::Right((Err(err), _)) => {
                    tracing::error!("任务异常: {:?}", err);
                }
            }

            cd.dec();
        };

        let join_handle = self.executor.spawn(task.boxed(), tt);
        let mut task_notifier = self.task_notifier.clone();

        let _ = self.executor.spawn(
            Box::pin(async move {
                if let Err(err) = task_notifier.send(join_handle).await {
                    tracing::error!("Failed to notify task completion: {:?}", err);
                }
            }),
            TaskType::Async,
        );

        TaskHandler {
            condition: self.condition.clone(),
            pool_size: self.pool_size.clone(),
            exiting: false,
        }
    }
}

impl SpawnEssentialTaskHandle {
    pub fn spawn<T>(&self, task: T) -> TaskHandler
    where
        T: Future<Output = Result<()>> + Send + 'static,
    {
        self.spawn_inner(task, TaskType::Async)
    }

    pub fn spawn_blocking<T>(&self, task: T) -> TaskHandler
    where
        T: Future<Output = Result<()>> + Send + 'static,
    {
        self.spawn_inner(task, TaskType::Block)
    }

    fn spawn_inner<T>(&self, task: T, tt: TaskType) -> TaskHandler
    where
        T: Future<Output = Result<()>> + Send + 'static,
    {
        let essential_failed = self.essential_failed_tx.clone();
        self.inner.spawn_inner(
            async move {
                let essential_failed = essential_failed.clone();
                std::panic::AssertUnwindSafe(task)
                    .catch_unwind()
                    .map(move |res| match res {
                        Err(pc) => {
                            if let Some(message) = pc.downcast_ref::<&str>() {
                                tracing::error!("Panic occurred: {}", message);
                            } else if let Some(message) = pc.downcast_ref::<String>() {
                                tracing::error!("Panic occurred: {}", message);
                            } else {
                                tracing::error!("Panic occurred: {:#?}", pc);
                            }
                            let _ = essential_failed.close_channel();
                        }
                        Ok(_) => {
                            tracing::debug!("Essential task exited. Exiting...");
                            let _ = essential_failed.close_channel();
                        }
                    })
                    .await;
                Ok(())
            },
            tt,
        )
    }
}

/// 异步服务管理器
pub struct TaskManager {
    /// 服务退出接收
    on_exit: Exit,
    /// 通知服务退出
    signal: Option<Signal>,

    /// 异步执行器(tokio)
    executor: TaskExecutor,

    /// 必要任务失败通知
    essential_failed_tx: TracingUnboundedSender<()>,
    /// 必要任务失败通知接收
    essential_failed_rx: TracingUnboundedReceiver<()>,

    /// 发起后台任务
    task_notifier: TracingUnboundedSender<JoinFuture<()>>,
    /// 运行后台任务
    completion_future: JoinFuture<()>,

    /// 子服务管理器
    child_tasks: Vec<TaskManager>,
}

impl TaskManager {
    pub fn new(executor: TaskExecutor) -> Self {
        let (signal, on_exit) = signal();
        let (essential_failed_tx, essential_failed_rx) = tracing_unbounded();
        let (task_notifier, bg_tasks) = tracing_unbounded();
        let completion_future = executor.spawn(
            Box::pin(bg_tasks.for_each_concurrent(None, |task| task)),
            TaskType::Async,
        );

        Self {
            on_exit,
            signal: Some(signal),
            executor,
            essential_failed_tx,
            essential_failed_rx,
            task_notifier,
            completion_future,
            child_tasks: Vec::new(),
        }
    }

    /// 停止当前服务
    pub fn terminate(&mut self) {
        if let Some(signal) = self.signal.take() {
            let _ = signal.fire();
            // 停止发起新的任务
            self.task_notifier.close_channel();
            for child in self.child_tasks.iter_mut() {
                child.terminate();
            }
        }
    }

    // pub fn collect(mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    //     self.task_notifier.close_channel();
    //
    //     Box::pin(async move {
    //         let mut t1 = self.essential_failed_rx.next().fuse();
    //         let mut t2 = self.on_exit.clone().fuse();
    //         let mut t3 = join_all(self.child_tasks.iter_mut().map(|x| x.collect())).fuse();
    //
    //         futures::select! {
    //             _ = t1 => {
    //                 tracing::error!("Essential task failed.");
    //             }
    //             // 接收到退出信号, 等待任务退出
    //             _ = t2 => {}
    //             // 子服务退出，但是这里会永远 pending
    //             _ = t3 => {}
    //         }
    //     })
    // }

    pub fn future<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let mut t1 = self.essential_failed_rx.next().fuse();
            let mut t2 = self.on_exit.clone().fuse();
            let mut t3 = join_all(
                self.child_tasks
                    .iter_mut()
                    .map(|x| x.future())
                    .chain(std::iter::once(pending().boxed())),
            )
            .fuse();

            futures::select! {
                _ = t1 => {
                    tracing::debug!("Essential task failed.");
                }
                // 接收到退出信号, 等待任务退出
                _ = t2 => {}
                // 子服务退出，但是这里会永远 pending
                _ = t3 => {}
            }
            Ok(())
        })
    }

    pub fn clean_shutdown(mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.terminate();
        let children_shutdowns = self.child_tasks.into_iter().map(|x| x.clean_shutdown());
        let completion_future = self.completion_future;

        Box::pin(async move {
            join_all(children_shutdowns).await;
            completion_future.await;
            Ok(())
        })
    }

    pub fn add_child(&mut self, child: TaskManager) {
        self.child_tasks.push(child);
    }

    pub fn spawn_handle(&self) -> SpawnTaskHandle {
        SpawnTaskHandle {
            on_exit: self.on_exit.clone(),
            executor: self.executor.clone(),
            task_notifier: self.task_notifier.clone(),
            condition: Arc::new(TaskCondition::new()),
            pool_size: None,
        }
    }

    pub fn spawn_essential_handle(&self) -> SpawnEssentialTaskHandle {
        SpawnEssentialTaskHandle {
            essential_failed_tx: self.essential_failed_tx.clone(),
            inner: self.spawn_handle(),
        }
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new(Default::default())
    }
}
