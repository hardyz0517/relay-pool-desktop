use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use tokio::sync::watch;

use crate::models::collector::CollectorRunResult;

type ScheduledCollectionResult = Result<CollectorRunResult, String>;

#[derive(Clone, Default)]
pub(crate) struct StationCollectionFeedback {
    inner: Arc<Mutex<StationCollectionFeedbackState>>,
}

#[derive(Default)]
struct StationCollectionFeedbackState {
    active: HashMap<String, Arc<ScheduledCollectionFeedbackEntry>>,
}

struct ScheduledCollectionFeedbackEntry {
    sender: watch::Sender<Option<ScheduledCollectionResult>>,
}

pub(crate) struct ScheduledCollectionFeedback {
    inner: Arc<Mutex<StationCollectionFeedbackState>>,
    station_id: String,
    entry: Arc<ScheduledCollectionFeedbackEntry>,
    completed: bool,
}

impl StationCollectionFeedback {
    /// Register a scheduled collection before it waits for execution capacity.
    /// This lets a manual request for the same station join the scheduled work
    /// rather than overtaking it while the scheduler is queued.
    pub(crate) fn begin_scheduled(&self, station_id: &str) -> Option<ScheduledCollectionFeedback> {
        if station_id.trim().is_empty() {
            return None;
        }

        let mut state = lock_state(&self.inner);
        if state.active.contains_key(station_id) {
            return None;
        }
        let (sender, _) = watch::channel(None);
        let entry = Arc::new(ScheduledCollectionFeedbackEntry { sender });
        state
            .active
            .insert(station_id.to_owned(), Arc::clone(&entry));
        Some(ScheduledCollectionFeedback {
            inner: Arc::clone(&self.inner),
            station_id: station_id.to_owned(),
            entry,
            completed: false,
        })
    }

    /// Wait for an in-flight scheduled collection, if one has registered for
    /// this station. A completed value remains available to every current
    /// manual caller after the scheduler releases the station lease.
    pub(crate) async fn wait_for_scheduled_result(
        &self,
        station_id: &str,
    ) -> Option<ScheduledCollectionResult> {
        let mut receiver = {
            let state = lock_state(&self.inner);
            state
                .active
                .get(station_id)
                .map(|entry| entry.sender.subscribe())
        }?;

        loop {
            if let Some(result) = receiver.borrow_and_update().clone() {
                return Some(result);
            }
            if receiver.changed().await.is_err() {
                if let Some(result) = receiver.borrow_and_update().clone() {
                    return Some(result);
                }
                return Some(Err(
                    "Scheduled station collection ended before publishing a result".to_string(),
                ));
            }
        }
    }
}

impl ScheduledCollectionFeedback {
    pub(crate) fn complete(mut self, result: ScheduledCollectionResult) {
        self.completed = true;
        self.entry.sender.send_replace(Some(result));
    }
}

impl Drop for ScheduledCollectionFeedback {
    fn drop(&mut self) {
        if !self.completed {
            self.entry.sender.send_replace(Some(Err(
                "Scheduled station collection was cancelled before completion".to_string(),
            )));
        }

        let mut state = lock_state(&self.inner);
        if state
            .active
            .get(&self.station_id)
            .is_some_and(|entry| Arc::ptr_eq(entry, &self.entry))
        {
            state.active.remove(&self.station_id);
        }
    }
}

fn lock_state(
    inner: &Mutex<StationCollectionFeedbackState>,
) -> MutexGuard<'_, StationCollectionFeedbackState> {
    inner.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::collector::CollectorSnapshot;

    fn result() -> CollectorRunResult {
        CollectorRunResult {
            snapshot: CollectorSnapshot {
                id: "snapshot-1".to_string(),
                station_id: "station-1".to_string(),
                endpoint_revision: 1,
                source: "fixture".to_string(),
                status: "success".to_string(),
                fetched_at: "1700000000000".to_string(),
                summary_json: serde_json::json!({}),
                normalized_json: serde_json::json!({}),
                raw_json_redacted: None,
                error_message: None,
                created_at: "1700000000000".to_string(),
            },
            events: Vec::new(),
        }
    }

    #[tokio::test]
    async fn registered_scheduled_collection_shares_its_completion_with_manual_waiters() {
        let feedback = StationCollectionFeedback::default();
        let scheduled = feedback
            .begin_scheduled("station-1")
            .expect("scheduled collection registers");
        let waiter = feedback.wait_for_scheduled_result("station-1");
        tokio::pin!(waiter);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut waiter)
                .await
                .is_err()
        );

        scheduled.complete(Ok(result()));
        let shared = waiter
            .await
            .expect("scheduled result is available")
            .expect("scheduled collection succeeds");
        assert_eq!(shared.snapshot.id, "snapshot-1");
        assert!(feedback
            .wait_for_scheduled_result("station-1")
            .await
            .is_none());
    }

    #[tokio::test]
    async fn dropped_scheduled_collection_wakes_manual_waiters_with_a_failure() {
        let feedback = StationCollectionFeedback::default();
        let scheduled = feedback
            .begin_scheduled("station-1")
            .expect("scheduled collection registers");
        let waiter = feedback.wait_for_scheduled_result("station-1");
        tokio::pin!(waiter);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut waiter)
                .await
                .is_err()
        );
        drop(scheduled);

        let error = waiter
            .await
            .expect("scheduled completion is available")
            .expect_err("dropped scheduled collection fails");
        assert!(error.contains("cancelled"));
    }
}
