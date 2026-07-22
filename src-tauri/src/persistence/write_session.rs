use std::sync::Arc;

use sqlx::{Sqlite, SqliteConnection, Transaction};
use tokio::sync::OwnedSemaphorePermit;

use crate::persistence::{
    error::PersistenceError, runtime_lifecycle::RuntimeWorkPermit,
    write_coordinator::WriteCoordinator,
};

pub(crate) struct WriteSession {
    transaction: Option<Transaction<'static, Sqlite>>,
    permit: Option<OwnedSemaphorePermit>,
    coordinator: Arc<WriteCoordinator>,
    _runtime_permit: RuntimeWorkPermit,
    completed: bool,
}

impl WriteSession {
    pub(crate) fn new(
        transaction: Transaction<'static, Sqlite>,
        permit: OwnedSemaphorePermit,
        coordinator: Arc<WriteCoordinator>,
        runtime_permit: RuntimeWorkPermit,
    ) -> Self {
        Self {
            transaction: Some(transaction),
            permit: Some(permit),
            coordinator,
            _runtime_permit: runtime_permit,
            completed: false,
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
        if let Err(error) = transaction.commit().await {
            self.coordinator.record_commit_outcome_unknown();
            self.completed = true;
            self.permit.take();
            return Err(error.into());
        }
        self.coordinator.record_commit();
        self.completed = true;
        self.permit.take();
        Ok(())
    }
}

impl Drop for WriteSession {
    fn drop(&mut self) {
        if !self.completed {
            self.coordinator.record_rollback();
            self.completed = true;
        }
    }
}
