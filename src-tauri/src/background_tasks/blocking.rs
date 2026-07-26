use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use tokio::{sync::Semaphore, task::JoinHandle};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct BlockingExecutorConfig {
    pub max_running: usize,
    pub queue_capacity: usize,
    pub queue_timeout: Duration,
    pub default_execution_timeout: Duration,
}

impl BlockingExecutorConfig {
    pub fn architecture_budget() -> Self {
        Self {
            max_running: 4,
            queue_capacity: 16,
            queue_timeout: Duration::from_millis(2_000),
            default_execution_timeout: Duration::from_millis(30_000),
        }
    }
}

#[derive(Clone)]
pub struct BlockingExecutor {
    semaphore: Arc<Semaphore>,
    queued: Arc<AtomicUsize>,
    running: Arc<AtomicUsize>,
    orphaned: Arc<AtomicUsize>,
    closed: Arc<AtomicBool>,
    next_id: Arc<AtomicU64>,
    config: BlockingExecutorConfig,
}

impl BlockingExecutor {
    pub fn new(config: BlockingExecutorConfig) -> Self {
        assert!(config.max_running > 0, "max_running must be positive");
        Self {
            semaphore: Arc::new(Semaphore::new(config.max_running)),
            queued: Arc::new(AtomicUsize::new(0)),
            running: Arc::new(AtomicUsize::new(0)),
            orphaned: Arc::new(AtomicUsize::new(0)),
            closed: Arc::new(AtomicBool::new(false)),
            next_id: Arc::new(AtomicU64::new(1)),
            config,
        }
    }

    pub fn submit<T, F>(
        &self,
        kind: impl Into<String>,
        operation_id: Option<String>,
        correlation_id: Option<String>,
        deadline: Option<Instant>,
        job: F,
    ) -> Result<BlockingJobHandle<T>, BlockingExecutorError>
    where
        T: Send + 'static,
        F: FnOnce(BlockingJobContext) -> Result<T, BlockingExecutorError> + Send + 'static,
    {
        if self.closed.load(Ordering::SeqCst) {
            return Err(BlockingExecutorError::Closed);
        }
        reserve_queue_slot(&self.queued, self.config.queue_capacity)?;

        let id = BlockingJobId(self.next_id.fetch_add(1, Ordering::SeqCst));
        let token = CancellationToken::new();
        let join_token = token.clone();
        let context = BlockingJobContext {
            id,
            kind: kind.into(),
            operation_id,
            correlation_id,
            cancellation_token: token.clone(),
        };
        let semaphore = Arc::clone(&self.semaphore);
        let queued = Arc::clone(&self.queued);
        let running = Arc::clone(&self.running);
        let orphaned = Arc::clone(&self.orphaned);
        let queue_timeout = bounded_timeout(self.config.queue_timeout, deadline);
        let execution_timeout = bounded_timeout(self.config.default_execution_timeout, deadline);

        let join = tokio::spawn(async move {
            let permit = tokio::select! {
                _ = join_token.cancelled() => {
                    queued.fetch_sub(1, Ordering::SeqCst);
                    return Err(BlockingExecutorError::CancelledBeforeStart);
                }
                permit = tokio::time::timeout(queue_timeout, semaphore.acquire_owned()) => {
                    queued.fetch_sub(1, Ordering::SeqCst);
                    match permit {
                        Ok(Ok(permit)) => permit,
                        Ok(Err(_)) => return Err(BlockingExecutorError::Closed),
                        Err(_) => return Err(BlockingExecutorError::QueueTimeout),
                    }
                }
            };

            let _permit = permit;
            running.fetch_add(1, Ordering::SeqCst);
            let blocking_token = join_token.clone();
            let blocking_join = tokio::task::spawn_blocking(move || {
                let result = job(context);
                if blocking_token.is_cancelled() {
                    return Err(BlockingExecutorError::CancelledLateResultDiscarded);
                }
                result
            });

            let result = match tokio::time::timeout(execution_timeout, blocking_join).await {
                Ok(Ok(result)) => result,
                Ok(Err(_)) => Err(BlockingExecutorError::Panicked),
                Err(_) => {
                    orphaned.fetch_add(1, Ordering::SeqCst);
                    Err(BlockingExecutorError::ExecutionTimeout)
                }
            };
            running.fetch_sub(1, Ordering::SeqCst);
            result
        });

        Ok(BlockingJobHandle { id, token, join })
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    pub async fn shutdown(
        &self,
        timeout: Duration,
    ) -> Result<BlockingJobMetrics, BlockingExecutorError> {
        self.close();
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);
        loop {
            let metrics = self.metrics();
            if metrics.queued == 0 && metrics.running == 0 {
                return Ok(metrics);
            }
            tokio::select! {
                _ = &mut deadline => return Err(BlockingExecutorError::ShutdownTimeout { metrics }),
                _ = tokio::task::yield_now() => {}
            }
        }
    }

    pub fn metrics(&self) -> BlockingJobMetrics {
        BlockingJobMetrics {
            queued: self.queued.load(Ordering::SeqCst),
            running: self.running.load(Ordering::SeqCst),
            orphaned: self.orphaned.load(Ordering::SeqCst),
        }
    }
}

fn reserve_queue_slot(
    queued: &AtomicUsize,
    queue_capacity: usize,
) -> Result<(), BlockingExecutorError> {
    let mut current = queued.load(Ordering::SeqCst);
    loop {
        if current >= queue_capacity {
            return Err(BlockingExecutorError::QueueFull);
        }
        match queued.compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return Ok(()),
            Err(next) => current = next,
        }
    }
}

fn bounded_timeout(default_timeout: Duration, deadline: Option<Instant>) -> Duration {
    let Some(deadline) = deadline else {
        return default_timeout;
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    default_timeout.min(remaining)
}

pub struct BlockingJobHandle<T> {
    id: BlockingJobId,
    token: CancellationToken,
    join: JoinHandle<Result<T, BlockingExecutorError>>,
}

impl<T> std::fmt::Debug for BlockingJobHandle<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BlockingJobHandle")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl<T> BlockingJobHandle<T> {
    pub fn id(&self) -> BlockingJobId {
        self.id
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }

    pub async fn result(self) -> Result<T, BlockingExecutorError> {
        self.join
            .await
            .map_err(|_| BlockingExecutorError::Panicked)?
    }
}

#[derive(Clone)]
pub struct BlockingJobContext {
    pub id: BlockingJobId,
    pub kind: String,
    pub operation_id: Option<String>,
    pub correlation_id: Option<String>,
    pub cancellation_token: CancellationToken,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlockingJobId(pub u64);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockingJobMetrics {
    pub queued: usize,
    pub running: usize,
    pub orphaned: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BlockingExecutorError {
    QueueFull,
    QueueTimeout,
    ExecutionTimeout,
    CancelledBeforeStart,
    CancelledLateResultDiscarded,
    Closed,
    Panicked,
    JobFailed { code: String },
    ShutdownTimeout { metrics: BlockingJobMetrics },
}

impl std::fmt::Display for BlockingExecutorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueueFull => formatter.write_str("blocking executor queue is full"),
            Self::QueueTimeout => formatter.write_str("blocking executor queue timed out"),
            Self::ExecutionTimeout => formatter.write_str("blocking executor job timed out"),
            Self::CancelledBeforeStart => {
                formatter.write_str("blocking job was cancelled before start")
            }
            Self::CancelledLateResultDiscarded => {
                formatter.write_str("blocking job completed after cancellation and was discarded")
            }
            Self::Closed => formatter.write_str("blocking executor is closed"),
            Self::Panicked => formatter.write_str("blocking job panicked"),
            Self::JobFailed { code } => write!(formatter, "blocking job failed: {code}"),
            Self::ShutdownTimeout { .. } => {
                formatter.write_str("blocking executor shutdown timed out")
            }
        }
    }
}

impl std::error::Error for BlockingExecutorError {}
