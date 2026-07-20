use crate::{
    application::{error::ApplicationError, pagination::PageLimit},
    persistence::{
        runtime::PersistenceHandle,
        stores::change_store::{ChangeCursor, ChangeEventPage, ChangeStore},
    },
};

#[derive(Clone)]
pub(crate) struct ChangeCenterQuery {
    runtime: PersistenceHandle,
    store: ChangeStore,
}

impl ChangeCenterQuery {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            store: ChangeStore,
        }
    }

    pub(crate) async fn load_page(
        &self,
        station_id: Option<&str>,
        cursor: Option<&ChangeCursor>,
        limit: PageLimit,
    ) -> Result<ChangeEventPage, ApplicationError> {
        if station_id.is_some_and(|value| value.trim().is_empty()) {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_page(&mut read, station_id, cursor, limit.get())
            .await
            .map_err(Into::into)
    }
}
