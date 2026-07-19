use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Debug)]
pub(crate) struct WriteCoordinator {
    semaphore: Arc<Semaphore>,
    metrics: Arc<WriteCoordinatorMetrics>,
}

#[derive(Debug, Default)]
pub(crate) struct WriteCoordinatorMetrics {
    queued_writes: AtomicU64,
    acquired_writes: AtomicU64,
    total_queue_wait_micros: AtomicU64,
    committed_writes: AtomicU64,
    rolled_back_writes: AtomicU64,
}

impl WriteCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(1)),
            metrics: Arc::new(WriteCoordinatorMetrics::default()),
        }
    }

    pub(crate) async fn acquire(&self) -> Result<OwnedSemaphorePermit, tokio::sync::AcquireError> {
        self.metrics.queued_writes.fetch_add(1, Ordering::Relaxed);
        let started = Instant::now();
        let permit = self.semaphore.clone().acquire_owned().await?;
        self.metrics.acquired_writes.fetch_add(1, Ordering::Relaxed);
        self.metrics.total_queue_wait_micros.fetch_add(
            started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        Ok(permit)
    }

    pub(crate) fn record_commit(&self) {
        self.metrics
            .committed_writes
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_rollback(&self) {
        self.metrics
            .rolled_back_writes
            .fetch_add(1, Ordering::Relaxed);
    }
}

impl Clone for WriteCoordinator {
    fn clone(&self) -> Self {
        Self {
            semaphore: self.semaphore.clone(),
            metrics: self.metrics.clone(),
        }
    }
}
