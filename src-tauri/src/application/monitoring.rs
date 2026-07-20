use std::sync::Arc;

use crate::{
    application::{clock::Clock, error::ApplicationError, ids::IdGenerator, pagination::PageLimit},
    models::channel_monitors::{
        ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun, ChannelMonitorRunCursor,
        ChannelMonitorRunPage, CreateChannelMonitorInput, CreateChannelMonitorRunInput,
        CreateChannelMonitorTemplateInput, UpdateChannelMonitorInput,
        UpdateChannelMonitorTemplateInput,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::monitoring_store::{
            MonitorPatch, MonitorTemplatePatch, MonitoringStore, NewMonitorRow, NewMonitorRunRow,
            NewMonitorTemplateRow,
        },
    },
};

#[derive(Clone)]
pub(crate) struct MonitoringService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    store: MonitoringStore,
}

impl MonitoringService {
    pub(crate) fn new(
        runtime: PersistenceHandle,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            runtime,
            clock,
            ids,
            store: MonitoringStore,
        }
    }

    pub(crate) async fn list_templates(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ChannelMonitorRequestTemplate>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_templates(&mut read, limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn get_template(
        &self,
        id: &str,
    ) -> Result<ChannelMonitorRequestTemplate, ApplicationError> {
        if id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.store
            .get_template(&mut read, id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn create_template(
        &self,
        input: CreateChannelMonitorTemplateInput,
    ) -> Result<ChannelMonitorRequestTemplate, ApplicationError> {
        let store = self.store;
        let row = NewMonitorTemplateRow {
            id: self.ids.next_id(),
            now: self.now_ms_string(),
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.insert_template(write, row).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_template(
        &self,
        input: UpdateChannelMonitorTemplateInput,
    ) -> Result<ChannelMonitorRequestTemplate, ApplicationError> {
        let store = self.store;
        let patch = MonitorTemplatePatch {
            now: self.now_ms_string(),
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.update_template(write, patch).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn delete_template(&self, id: String) -> Result<(), ApplicationError> {
        if id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let store = self.store;
        self.runtime
            .write(|write| Box::pin(async move { store.delete_template(write, &id).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_monitors(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ChannelMonitor>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_monitors(&mut read, limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn get_monitor(&self, id: &str) -> Result<ChannelMonitor, ApplicationError> {
        if id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.store
            .get_monitor(&mut read, id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn create_monitor(
        &self,
        input: CreateChannelMonitorInput,
    ) -> Result<ChannelMonitor, ApplicationError> {
        let store = self.store;
        let next_run_at = input.enabled.then(|| self.now_ms_string());
        let row = NewMonitorRow {
            id: self.ids.next_id(),
            now: self.now_ms_string(),
            next_run_at,
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.insert_monitor(write, row).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_monitor(
        &self,
        input: UpdateChannelMonitorInput,
    ) -> Result<ChannelMonitor, ApplicationError> {
        let store = self.store;
        let patch = MonitorPatch {
            now: self.now_ms_string(),
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.update_monitor(write, patch).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn delete_monitor(&self, id: String) -> Result<(), ApplicationError> {
        if id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let store = self.store;
        self.runtime
            .write(|write| Box::pin(async move { store.delete_monitor(write, &id).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn due_monitors(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ChannelMonitor>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .due_monitors(&mut read, self.now_ms(), limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn record_run(
        &self,
        input: CreateChannelMonitorRunInput,
    ) -> Result<ChannelMonitorRun, ApplicationError> {
        let store = self.store;
        let now_ms = self.now_ms();
        let id = self.ids.next_id();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    let (interval_seconds, jitter_seconds) =
                        store.monitor_schedule(write, &input.monitor_id).await?;
                    let next_run_at =
                        next_run_at(&input.monitor_id, now_ms, interval_seconds, jitter_seconds)
                            .to_string();
                    store
                        .insert_run_and_advance_monitor(
                            write,
                            NewMonitorRunRow {
                                id,
                                now: now_ms.to_string(),
                                next_run_at,
                                input,
                            },
                        )
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_run_page(
        &self,
        monitor_id: &str,
        cursor: Option<&ChannelMonitorRunCursor>,
        limit: PageLimit,
    ) -> Result<ChannelMonitorRunPage, ApplicationError> {
        if monitor_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_run_page(&mut read, monitor_id, cursor, limit.get())
            .await
            .map_err(Into::into)
    }

    fn now_ms(&self) -> i64 {
        self.clock.now_utc().timestamp_millis()
    }

    fn now_ms_string(&self) -> String {
        self.now_ms().to_string()
    }
}

fn next_run_at(monitor_id: &str, now_ms: i64, interval_seconds: i64, jitter_seconds: i64) -> i64 {
    let jitter_range_ms = u64::try_from(jitter_seconds.max(0))
        .unwrap_or_default()
        .saturating_mul(1_000)
        .saturating_add(1);
    let jitter_ms = if jitter_seconds <= 0 {
        0
    } else {
        stable_schedule_hash(monitor_id, now_ms) % jitter_range_ms
    };
    now_ms
        .saturating_add(interval_seconds.max(1).saturating_mul(1_000))
        .saturating_add(i64::try_from(jitter_ms).unwrap_or(i64::MAX))
}

fn stable_schedule_hash(monitor_id: &str, now_ms: i64) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    monitor_id
        .as_bytes()
        .iter()
        .chain(now_ms.to_le_bytes().iter())
        .fold(FNV_OFFSET, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_is_deterministic_and_bounded() {
        let first = next_run_at("monitor-a", 1_000, 30, 5);
        let second = next_run_at("monitor-a", 1_000, 30, 5);
        assert_eq!(first, second);
        assert!((31_000..=36_000).contains(&first));
    }
}
