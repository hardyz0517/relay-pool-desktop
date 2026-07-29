use std::sync::Arc;

use crate::{
    application::{changes::ChangeService, error::ApplicationError, pagination::PageLimit},
    models::change_events::{ChangeEvent, UpsertChangeEventInput},
};

#[derive(Clone)]
pub(crate) struct ChangeEventsCommandFacade {
    changes: Arc<ChangeService>,
}

impl ChangeEventsCommandFacade {
    pub(crate) fn new(changes: Arc<ChangeService>) -> Self {
        Self { changes }
    }

    pub(crate) async fn list_change_events(
        &self,
        station_id: Option<&str>,
        limit: PageLimit,
    ) -> Result<Vec<ChangeEvent>, ApplicationError> {
        self.changes.list(station_id, limit).await
    }

    pub(crate) async fn clear_change_events(&self) -> Result<(), ApplicationError> {
        self.changes.clear().await
    }

    pub(crate) async fn upsert_change_event(
        &self,
        input: UpsertChangeEventInput,
    ) -> Result<ChangeEvent, ApplicationError> {
        self.changes.upsert(input).await
    }

    pub(crate) async fn mark_change_event_read(
        &self,
        id: String,
    ) -> Result<ChangeEvent, ApplicationError> {
        self.changes.mark_read(id).await
    }

    pub(crate) async fn mark_change_events_read(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<ChangeEvent>, ApplicationError> {
        self.changes.mark_many_read(ids).await
    }

    pub(crate) async fn dismiss_change_event(
        &self,
        id: String,
    ) -> Result<ChangeEvent, ApplicationError> {
        self.changes.dismiss(id).await
    }

    pub(crate) async fn resolve_change_event(
        &self,
        id: String,
    ) -> Result<ChangeEvent, ApplicationError> {
        self.changes.resolve(id).await
    }
}
