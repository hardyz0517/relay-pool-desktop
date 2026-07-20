use std::sync::Arc;

use futures_util::future::BoxFuture;

use crate::{
    persistence::{
        error::PersistenceError,
        runtime::PersistenceRuntime,
        stores::request_log_store::{
            AttemptPersistenceResult, RequestLogStore, RequestStartPersistenceResult,
            RequestTerminalPersistenceResult,
        },
    },
    services::proxy::lifecycle::{
        attempt::AttemptTerminalRecord,
        ports::{
            AttemptCommitAck, LifecycleWriteError, RequestCommitAck, RequestLifecycleStore,
            RequestStartAck,
        },
        request::{FinalRequestRecord, RequestStartRecord},
    },
};

#[derive(Clone)]
pub(crate) struct RequestLifecyclePersistenceService {
    runtime: Arc<PersistenceRuntime>,
    store: RequestLogStore,
}

impl RequestLifecyclePersistenceService {
    pub(crate) fn new(runtime: Arc<PersistenceRuntime>, store: RequestLogStore) -> Self {
        Self { runtime, store }
    }
}

impl RequestLifecycleStore for RequestLifecyclePersistenceService {
    fn start_request(
        &self,
        record: RequestStartRecord,
    ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
        let runtime = Arc::clone(&self.runtime);
        let store = self.store;
        Box::pin(async move {
            let outcome = tokio::task::spawn_blocking(move || {
                let mut session = runtime.begin_write().map_err(map_persistence_error)?;
                let result = store
                    .start_request(&mut session, &record)
                    .map_err(map_persistence_error)?;
                session.commit().map_err(map_persistence_error)?;
                Ok::<RequestStartPersistenceResult, LifecycleWriteError>(result)
            })
            .await
            .map_err(|error| LifecycleWriteError::Unavailable(error.to_string()))??;
            Ok(RequestStartAck {
                inserted: outcome.inserted,
            })
        })
    }

    fn finish_attempt(
        &self,
        record: AttemptTerminalRecord,
    ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
        let runtime = Arc::clone(&self.runtime);
        let store = self.store;
        Box::pin(async move {
            let outcome = tokio::task::spawn_blocking(move || {
                let mut session = runtime.begin_write().map_err(map_persistence_error)?;
                let result = store
                    .finish_attempt(&mut session, &record)
                    .map_err(map_persistence_error)?;
                session.commit().map_err(map_persistence_error)?;
                Ok::<AttemptPersistenceResult, LifecycleWriteError>(result)
            })
            .await
            .map_err(|error| LifecycleWriteError::Unavailable(error.to_string()))??;
            Ok(AttemptCommitAck {
                inserted: outcome.inserted,
                health_applied: outcome.health_applied,
            })
        })
    }

    fn finish_request(
        &self,
        record: FinalRequestRecord,
    ) -> BoxFuture<'static, Result<RequestCommitAck, LifecycleWriteError>> {
        let runtime = Arc::clone(&self.runtime);
        let store = self.store;
        Box::pin(async move {
            let outcome = tokio::task::spawn_blocking(move || {
                let mut session = runtime.begin_write().map_err(map_persistence_error)?;
                let result = store
                    .finish_request(&mut session, &record)
                    .map_err(map_persistence_error)?;
                session.commit().map_err(map_persistence_error)?;
                Ok::<RequestTerminalPersistenceResult, LifecycleWriteError>(result)
            })
            .await
            .map_err(|error| LifecycleWriteError::Unavailable(error.to_string()))??;
            Ok(RequestCommitAck {
                finalized: outcome.finalized,
            })
        })
    }
}

fn map_persistence_error(error: PersistenceError) -> LifecycleWriteError {
    match error {
        PersistenceError::CommitOutcomeUnknown(message) => {
            LifecycleWriteError::CommitOutcomeUnknown(message)
        }
        PersistenceError::Unavailable(message)
        | PersistenceError::ConstraintViolation(message)
        | PersistenceError::InvariantViolation(message)
        | PersistenceError::Database(message) => LifecycleWriteError::Unavailable(message),
        PersistenceError::Draining => {
            LifecycleWriteError::Unavailable("persistence is draining".to_string())
        }
        PersistenceError::Busy => LifecycleWriteError::Unavailable("database busy".to_string()),
        PersistenceError::Locked => LifecycleWriteError::Unavailable("database locked".to_string()),
        PersistenceError::StaleRevision => {
            LifecycleWriteError::Unavailable("stale endpoint revision".to_string())
        }
    }
}
