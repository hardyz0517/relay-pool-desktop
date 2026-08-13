use std::{
    collections::HashSet,
    num::NonZeroUsize,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub(crate) struct StationCollectionCoordinator {
    inner: Arc<StationCollectionCoordinatorInner>,
}

struct StationCollectionCoordinatorInner {
    state: Mutex<StationCollectionCoordinatorState>,
    notify: Notify,
}

struct StationCollectionCoordinatorState {
    max_concurrency: NonZeroUsize,
    active_station_ids: HashSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StationCollectionAdmissionError {
    AlreadyRunning,
    AtCapacity,
    Cancelled,
    InvalidStationId,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StationCollectionCoordinatorSnapshot {
    pub max_concurrency: usize,
    pub active: usize,
}

pub(crate) struct StationCollectionLease {
    inner: Arc<StationCollectionCoordinatorInner>,
    station_id: String,
}

enum TryInsert {
    Acquired,
    AlreadyRunning,
    AtCapacity,
}

impl StationCollectionCoordinator {
    pub(crate) fn new(max_concurrency: NonZeroUsize) -> Self {
        Self {
            inner: Arc::new(StationCollectionCoordinatorInner {
                state: Mutex::new(StationCollectionCoordinatorState {
                    max_concurrency,
                    active_station_ids: HashSet::new(),
                }),
                notify: Notify::new(),
            }),
        }
    }

    pub(crate) fn set_max_concurrency(&self, max_concurrency: NonZeroUsize) {
        let changed = {
            let mut state = lock_state(&self.inner);
            if state.max_concurrency == max_concurrency {
                false
            } else {
                state.max_concurrency = max_concurrency;
                true
            }
        };
        if changed {
            self.inner.notify.notify_waiters();
        }
    }

    pub(crate) fn max_concurrency(&self) -> NonZeroUsize {
        lock_state(&self.inner).max_concurrency
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> StationCollectionCoordinatorSnapshot {
        let state = lock_state(&self.inner);
        StationCollectionCoordinatorSnapshot {
            max_concurrency: state.max_concurrency.get(),
            active: state.active_station_ids.len(),
        }
    }

    pub(crate) fn try_acquire(
        &self,
        station_id: &str,
    ) -> Result<StationCollectionLease, StationCollectionAdmissionError> {
        match try_insert_station(&self.inner, station_id)? {
            TryInsert::Acquired => Ok(StationCollectionLease::new(
                Arc::clone(&self.inner),
                station_id.to_owned(),
            )),
            TryInsert::AlreadyRunning => Err(StationCollectionAdmissionError::AlreadyRunning),
            TryInsert::AtCapacity => Err(StationCollectionAdmissionError::AtCapacity),
        }
    }

    pub(crate) async fn acquire(
        &self,
        station_id: &str,
        cancellation: &CancellationToken,
    ) -> Result<StationCollectionLease, StationCollectionAdmissionError> {
        loop {
            if cancellation.is_cancelled() {
                return Err(StationCollectionAdmissionError::Cancelled);
            }

            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            match try_insert_station(&self.inner, station_id)? {
                TryInsert::Acquired => {
                    let lease =
                        StationCollectionLease::new(Arc::clone(&self.inner), station_id.to_owned());
                    if cancellation.is_cancelled() {
                        drop(lease);
                        return Err(StationCollectionAdmissionError::Cancelled);
                    }
                    return Ok(lease);
                }
                TryInsert::AlreadyRunning => {
                    return Err(StationCollectionAdmissionError::AlreadyRunning);
                }
                TryInsert::AtCapacity => {}
            }

            tokio::select! {
                _ = cancellation.cancelled() => {
                    return Err(StationCollectionAdmissionError::Cancelled);
                }
                _ = &mut notified => {}
            }
        }
    }
}

impl StationCollectionLease {
    fn new(inner: Arc<StationCollectionCoordinatorInner>, station_id: String) -> Self {
        Self { inner, station_id }
    }
}

impl Drop for StationCollectionLease {
    fn drop(&mut self) {
        let removed = lock_state(&self.inner)
            .active_station_ids
            .remove(&self.station_id);
        debug_assert!(
            removed,
            "station collection lease must own an active station ID"
        );
        if removed {
            self.inner.notify.notify_waiters();
        }
    }
}

fn try_insert_station(
    inner: &StationCollectionCoordinatorInner,
    station_id: &str,
) -> Result<TryInsert, StationCollectionAdmissionError> {
    if station_id.trim().is_empty() {
        return Err(StationCollectionAdmissionError::InvalidStationId);
    }

    let mut state = lock_state(inner);
    if state.active_station_ids.contains(station_id) {
        return Ok(TryInsert::AlreadyRunning);
    }
    if state.active_station_ids.len() >= state.max_concurrency.get() {
        return Ok(TryInsert::AtCapacity);
    }
    let inserted = state.active_station_ids.insert(station_id.to_owned());
    debug_assert!(inserted, "station ID was checked before insertion");
    Ok(TryInsert::Acquired)
}

fn lock_state(
    inner: &StationCollectionCoordinatorInner,
) -> MutexGuard<'_, StationCollectionCoordinatorState> {
    inner.state.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::{future::pending, num::NonZeroUsize, panic::AssertUnwindSafe, sync::Arc};

    use tokio::{sync::oneshot, time::timeout};
    use tokio_util::sync::CancellationToken;

    use super::*;

    fn coordinator(limit: usize) -> StationCollectionCoordinator {
        StationCollectionCoordinator::new(NonZeroUsize::new(limit).expect("non-zero limit"))
    }

    #[test]
    fn clones_share_station_exclusion_but_allow_different_stations() {
        let coordinator = coordinator(2);
        let clone = coordinator.clone();
        let first = coordinator
            .try_acquire("station-a")
            .expect("first station starts");
        let second = clone
            .try_acquire("station-b")
            .expect("different station starts");

        assert!(matches!(
            clone.try_acquire("station-a"),
            Err(StationCollectionAdmissionError::AlreadyRunning),
        ));
        assert_eq!(coordinator.snapshot().active, 2);
        drop((first, second));
        assert_eq!(coordinator.snapshot().active, 0);
    }

    #[test]
    fn applies_capacity_changes_and_releases_leases() {
        let coordinator = coordinator(2);
        let first = coordinator.try_acquire("a").expect("a starts");
        let second = coordinator.try_acquire("b").expect("b starts");
        assert!(matches!(
            coordinator.try_acquire("c"),
            Err(StationCollectionAdmissionError::AtCapacity),
        ));

        coordinator.set_max_concurrency(NonZeroUsize::new(1).expect("non-zero"));
        drop(first);
        assert!(matches!(
            coordinator.try_acquire("c"),
            Err(StationCollectionAdmissionError::AtCapacity),
        ));
        drop(second);
        assert!(coordinator.try_acquire("c").is_ok());
    }

    #[test]
    fn validates_ids_and_prioritizes_same_station_conflict() {
        let coordinator = coordinator(1);
        let opaque = coordinator
            .try_acquire(" station-a ")
            .expect("opaque ID starts");
        assert!(matches!(
            coordinator.try_acquire(" station-a "),
            Err(StationCollectionAdmissionError::AlreadyRunning),
        ));
        assert!(matches!(
            coordinator.try_acquire("station-a"),
            Err(StationCollectionAdmissionError::AtCapacity),
        ));
        assert!(matches!(
            coordinator.try_acquire("   "),
            Err(StationCollectionAdmissionError::InvalidStationId),
        ));
        drop(opaque);
    }

    #[tokio::test]
    async fn waits_for_capacity_and_observes_cancellation() {
        let coordinator = coordinator(1);
        let first = coordinator.try_acquire("a").expect("a starts");
        let cancellation = CancellationToken::new();
        let (started_tx, started_rx) = oneshot::channel();
        let waiting_coordinator = coordinator.clone();
        let waiting_cancellation = cancellation.clone();
        let mut waiting = tokio::spawn(async move {
            let _ = started_tx.send(());
            waiting_coordinator
                .acquire("b", &waiting_cancellation)
                .await
        });
        started_rx.await.expect("waiter starts");
        assert!(timeout(std::time::Duration::from_millis(20), &mut waiting)
            .await
            .is_err());
        drop(first);
        let second = timeout(std::time::Duration::from_secs(1), waiting)
            .await
            .expect("release wakes waiter")
            .expect("waiter joins")
            .expect("b acquires");
        drop(second);
    }

    #[tokio::test]
    async fn cancellation_and_capacity_change_wake_waiters() {
        let coordinator = coordinator(1);
        let first = coordinator.try_acquire("a").expect("a starts");
        let cancellation = CancellationToken::new();
        let waiting_coordinator = coordinator.clone();
        let waiting_cancellation = cancellation.clone();
        let waiting = tokio::spawn(async move {
            waiting_coordinator
                .acquire("b", &waiting_cancellation)
                .await
        });
        tokio::task::yield_now().await;
        cancellation.cancel();
        assert!(matches!(
            waiting.await.expect("waiter joins"),
            Err(StationCollectionAdmissionError::Cancelled),
        ));
        assert_eq!(coordinator.snapshot().active, 1);

        let waiting_cancellation = CancellationToken::new();
        let waiting_coordinator = coordinator.clone();
        let waiting = tokio::spawn(async move {
            waiting_coordinator
                .acquire("b", &waiting_cancellation)
                .await
        });
        tokio::task::yield_now().await;
        coordinator.set_max_concurrency(NonZeroUsize::new(2).expect("non-zero"));
        let second = timeout(std::time::Duration::from_secs(1), waiting)
            .await
            .expect("limit change wakes waiter")
            .expect("waiter joins")
            .expect("b acquires");
        drop((first, second));
    }

    #[tokio::test]
    async fn future_abort_and_panic_unwind_release_leases() {
        let coordinator = coordinator(1);
        let acquired = Arc::new(tokio::sync::Notify::new());
        let task_coordinator = coordinator.clone();
        let task_acquired = Arc::clone(&acquired);
        let task = tokio::spawn(async move {
            let _lease = task_coordinator.try_acquire("a").expect("a starts in task");
            task_acquired.notify_one();
            pending::<()>().await;
        });
        acquired.notified().await;
        task.abort();
        assert!(task.await.expect_err("task aborts").is_cancelled());
        assert!(coordinator.try_acquire("a").is_ok());

        let panic_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _lease = coordinator.try_acquire("b").expect("b starts");
            panic!("test panic");
        }));
        assert!(panic_result.is_err());
        assert_eq!(coordinator.snapshot().active, 0);
        assert!(coordinator.try_acquire("b").is_ok());
    }
}
