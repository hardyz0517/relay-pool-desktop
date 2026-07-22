use std::{collections::HashSet, sync::Arc};

use crate::{
    application::{clock::Clock, error::ApplicationError, ids::IdGenerator, pagination::PageLimit},
    models::change_events::{ChangeEvent, UpsertChangeEventInput},
    persistence::{
        runtime::PersistenceHandle,
        stores::change_store::{ChangeStore, NewChangeEvent},
    },
};

#[derive(Clone)]
pub(crate) struct ChangeService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    store: ChangeStore,
}

impl ChangeService {
    pub(crate) fn new(
        runtime: PersistenceHandle,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            runtime,
            clock,
            ids,
            store: ChangeStore,
        }
    }

    pub(crate) async fn list(
        &self,
        station_id: Option<&str>,
        limit: PageLimit,
    ) -> Result<Vec<ChangeEvent>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_page(&mut read, station_id, None, limit.get())
            .await
            .map(|page| page.items)
            .map_err(Into::into)
    }

    pub(crate) async fn clear(&self) -> Result<(), ApplicationError> {
        let store = self.store;
        self.runtime
            .write(|write| Box::pin(async move { store.clear(write).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn upsert(
        &self,
        input: UpsertChangeEventInput,
    ) -> Result<ChangeEvent, ApplicationError> {
        validate_input(&input)?;
        let store = self.store;
        let event = NewChangeEvent {
            id: self.ids.next_id(),
            severity: input.severity,
            event_type: input.event_type,
            title: input.title,
            message: input.message,
            object_type: input.object_type,
            object_id: input.object_id,
            station_id: input.station_id,
            station_key_id: input.station_key_id,
            pricing_rule_id: input.pricing_rule_id,
            request_log_id: input.request_log_id,
            old_value_json: input.old_value_json,
            new_value_json: input.new_value_json,
            impact_json: input.impact_json,
            dedupe_key: input.dedupe_key,
            source: input.source,
            now: self.now(),
        };
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    let result = store.upsert(write, &event).await?;
                    store.get_by_id(write.connection(), &result.id).await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn mark_read(&self, id: String) -> Result<ChangeEvent, ApplicationError> {
        self.set_status(id, "read").await
    }

    pub(crate) async fn dismiss(&self, id: String) -> Result<ChangeEvent, ApplicationError> {
        self.set_status(id, "dismissed").await
    }

    pub(crate) async fn resolve(&self, id: String) -> Result<ChangeEvent, ApplicationError> {
        self.set_status(id, "resolved").await
    }

    pub(crate) async fn mark_many_read(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<ChangeEvent>, ApplicationError> {
        let mut seen = HashSet::new();
        let ids = ids
            .into_iter()
            .filter(|id| !id.trim().is_empty() && seen.insert(id.clone()))
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let store = self.store;
        let now = self.now();
        self.runtime
            .write(|write| Box::pin(async move { store.mark_many_read(write, &ids, &now).await }))
            .await
            .map_err(Into::into)
    }

    async fn set_status(
        &self,
        id: String,
        status: &'static str,
    ) -> Result<ChangeEvent, ApplicationError> {
        if id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let store = self.store;
        let now = self.now();
        self.runtime
            .write(|write| {
                Box::pin(async move { store.set_status(write, &id, status, &now).await })
            })
            .await
            .map_err(Into::into)
    }

    fn now(&self) -> String {
        self.clock.now_utc().timestamp_millis().to_string()
    }
}

fn validate_input(input: &UpsertChangeEventInput) -> Result<(), ApplicationError> {
    if input.severity.trim().is_empty()
        || input.event_type.trim().is_empty()
        || input.title.trim().is_empty()
        || input.message.trim().is_empty()
        || input.object_type.trim().is_empty()
        || input.dedupe_key.trim().is_empty()
        || input.source.trim().is_empty()
    {
        return Err(ApplicationError::ConstraintViolation);
    }
    Ok(())
}
