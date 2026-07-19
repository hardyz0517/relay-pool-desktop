use std::sync::Arc;

use sqlx::{Sqlite, SqliteConnection, Transaction};
use tokio::sync::OwnedSemaphorePermit;

use crate::persistence::{error::PersistenceError, write_coordinator::WriteCoordinator};

pub(crate) struct WriteSession {
    transaction: Option<Transaction<'static, Sqlite>>,
    permit: Option<OwnedSemaphorePermit>,
    coordinator: Arc<WriteCoordinator>,
    committed: bool,
}

impl WriteSession {
    pub(crate) fn new(
        transaction: Transaction<'static, Sqlite>,
        permit: OwnedSemaphorePermit,
        coordinator: Arc<WriteCoordinator>,
    ) -> Self {
        Self {
            transaction: Some(transaction),
            permit: Some(permit),
            coordinator,
            committed: false,
        }
    }

    pub(crate) fn connection(&mut self) -> &mut SqliteConnection {
        let transaction = self
            .transaction
            .as_mut()
            .expect("write session used after close");
        &mut *transaction
    }

    pub(crate) async fn commit(mut self) -> Result<(), PersistenceError> {
        let transaction = self
            .transaction
            .take()
            .ok_or(PersistenceError::SessionClosed)?;
        transaction.commit().await?;
        self.committed = true;
        self.coordinator.record_commit();
        self.permit.take();
        Ok(())
    }
}

impl Drop for WriteSession {
    fn drop(&mut self) {
        if !self.committed && self.transaction.is_some() {
            self.coordinator.record_rollback();
        }
    }
}
