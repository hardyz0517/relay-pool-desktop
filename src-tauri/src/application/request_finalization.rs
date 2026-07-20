use std::sync::Arc;

use futures_util::future::BoxFuture;

use crate::{
    application::clock::{Clock, SystemClock},
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
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
pub(crate) struct RequestFinalizationService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
}

impl RequestFinalizationService {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            clock: Arc::new(SystemClock),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_clock(runtime: PersistenceHandle, clock: Arc<dyn Clock>) -> Self {
        Self { runtime, clock }
    }
}

impl RequestLifecycleStore for RequestFinalizationService {
    fn start_request(
        &self,
        record: RequestStartRecord,
    ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
        let runtime = self.runtime.clone();
        let created_at_ms = self.clock.now_utc().timestamp_millis();
        Box::pin(async move {
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome: RequestStartPersistenceResult = RequestLogStore
                .start_request(&mut session, &record, created_at_ms)
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
        let runtime = self.runtime.clone();
        Box::pin(async move {
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome: AttemptPersistenceResult = RequestLogStore
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
        let runtime = self.runtime.clone();
        let terminal_at_ms = self.clock.now_utc().timestamp_millis();
        Box::pin(async move {
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome: RequestTerminalPersistenceResult = RequestLogStore
                .finish_request(&mut session, &record, terminal_at_ms)
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
