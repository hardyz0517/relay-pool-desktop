use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[cfg(test)]
#[allow(
    dead_code,
    reason = "the runtime integration target asserts bounded queue metrics from this snapshot"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WriteCoordinatorSnapshot {
    pub(crate) current_queue_depth: u64,
    pub(crate) peak_queue_depth: u64,
    pub(crate) acquired_writes: u64,
    pub(crate) total_queue_wait_micros: u64,
    pub(crate) committed_writes: u64,
    pub(crate) rolled_back_writes: u64,
}

#[derive(Debug)]
pub(crate) struct WriteCoordinator {
    semaphore: Arc<Semaphore>,
    metrics: Arc<WriteCoordinatorMetrics>,
}

#[derive(Debug, Default)]
pub(crate) struct WriteCoordinatorMetrics {
    current_queue_depth: AtomicU64,
    peak_queue_depth: AtomicU64,
    acquired_writes: AtomicU64,
    total_queue_wait_micros: AtomicU64,
    committed_writes: AtomicU64,
    rolled_back_writes: AtomicU64,
    data_revision: AtomicU64,
    active_write_sessions: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "the request-log read model uses this version in application composition; some isolated persistence integration targets do not"
    )
)]
pub(crate) struct PersistenceVersion {
    data_revision: u64,
    active_write_sessions: u64,
}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "version constructors and accessors are used by request-log cache tests in the library target"
    )
)]
impl PersistenceVersion {
    #[cfg(test)]
    pub(crate) fn for_test(data_revision: u64, active_write_sessions: u64) -> Self {
        Self {
            data_revision,
            active_write_sessions,
        }
    }

    pub(crate) fn is_quiescent(self) -> bool {
        self.active_write_sessions == 0
    }

    pub(crate) fn data_revision(self) -> u64 {
        self.data_revision
    }
}

impl WriteCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(1)),
            metrics: Arc::new(WriteCoordinatorMetrics::default()),
        }
    }

    pub(crate) async fn acquire(&self) -> Result<OwnedSemaphorePermit, tokio::sync::AcquireError> {
        let queue_depth = self
            .metrics
            .current_queue_depth
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        self.metrics
            .peak_queue_depth
            .fetch_max(queue_depth, Ordering::Relaxed);
        let queued = QueuedWrite::new(Arc::clone(&self.metrics));
        let started = Instant::now();
        let permit = self.semaphore.clone().acquire_owned().await?;
        queued.acquired();
        self.metrics.acquired_writes.fetch_add(1, Ordering::Relaxed);
        self.metrics.total_queue_wait_micros.fetch_add(
            started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        Ok(permit)
    }

    pub(crate) fn record_session_started(&self) {
        self.metrics
            .active_write_sessions
            .fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn persistence_version(&self) -> PersistenceVersion {
        let active_write_sessions = self.metrics.active_write_sessions.load(Ordering::SeqCst);
        let data_revision = self.metrics.data_revision.load(Ordering::SeqCst);
        PersistenceVersion {
            data_revision,
            active_write_sessions,
        }
    }

    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "the runtime integration target asserts bounded queue metrics"
    )]
    pub(crate) fn snapshot(&self) -> WriteCoordinatorSnapshot {
        WriteCoordinatorSnapshot {
            current_queue_depth: self.metrics.current_queue_depth.load(Ordering::Relaxed),
            peak_queue_depth: self.metrics.peak_queue_depth.load(Ordering::Relaxed),
            acquired_writes: self.metrics.acquired_writes.load(Ordering::Relaxed),
            total_queue_wait_micros: self.metrics.total_queue_wait_micros.load(Ordering::Relaxed),
            committed_writes: self.metrics.committed_writes.load(Ordering::Relaxed),
            rolled_back_writes: self.metrics.rolled_back_writes.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn record_commit(&self) {
        self.metrics
            .committed_writes
            .fetch_add(1, Ordering::Relaxed);
        self.finish_write(true);
    }

    pub(crate) fn record_commit_outcome_unknown(&self) {
        self.finish_write(true);
    }

    pub(crate) fn record_rollback(&self) {
        self.metrics
            .rolled_back_writes
            .fetch_add(1, Ordering::Relaxed);
        self.finish_write(false);
    }

    fn finish_write(&self, invalidate_reads: bool) {
        if invalidate_reads {
            self.metrics.data_revision.fetch_add(1, Ordering::SeqCst);
        }
        let previous = self
            .metrics
            .active_write_sessions
            .fetch_sub(1, Ordering::SeqCst);
        debug_assert!(previous > 0, "write session accounting underflow");
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

struct QueuedWrite {
    metrics: Arc<WriteCoordinatorMetrics>,
    active: bool,
}

impl QueuedWrite {
    fn new(metrics: Arc<WriteCoordinatorMetrics>) -> Self {
        Self {
            metrics,
            active: true,
        }
    }

    fn acquired(mut self) {
        self.release();
    }

    fn release(&mut self) {
        if self.active {
            self.metrics
                .current_queue_depth
                .fetch_sub(1, Ordering::Relaxed);
            self.active = false;
        }
    }
}

impl Drop for QueuedWrite {
    fn drop(&mut self) {
        self.release();
    }
}
