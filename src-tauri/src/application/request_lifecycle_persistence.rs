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
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome: RequestStartPersistenceResult = store
                .start_request(&mut session, &record)
                .await
                .map_err(map_persistence_error)?;
            session.commit().await.map_err(map_persistence_error)?;
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
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome: AttemptPersistenceResult = store
                .finish_attempt(&mut session, &record)
                .await
                .map_err(map_persistence_error)?;
            session.commit().await.map_err(map_persistence_error)?;
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
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome: RequestTerminalPersistenceResult = store
                .finish_request(&mut session, &record)
                .await
                .map_err(map_persistence_error)?;
            session.commit().await.map_err(map_persistence_error)?;
            Ok(RequestCommitAck {
                finalized: outcome.finalized,
            })
        })
    }
}

fn map_persistence_error(error: PersistenceError) -> LifecycleWriteError {
    match error {
        PersistenceError::CommitOutcomeUnknown => LifecycleWriteError::CommitOutcomeUnknown(
            "request lifecycle commit outcome is unknown".to_string(),
        ),
        other => LifecycleWriteError::Unavailable(other.to_string()),
    }
}
