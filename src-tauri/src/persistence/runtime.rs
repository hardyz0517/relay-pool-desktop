use std::{future::Future, path::Path, pin::Pin, sync::Arc, time::Duration};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Connection, Executor, SqliteConnection, SqlitePool,
};

#[cfg(test)]
use crate::persistence::backup::{create_verified_backup, VerifiedBackup};
#[cfg(test)]
use crate::persistence::write_coordinator::WriteCoordinatorSnapshot;
use crate::persistence::{
    error::PersistenceError,
    health_check::{record_runtime_open, runtime_health, RuntimeHealth},
    migrations::applied_schema_version,
    read_session::ReadSession,
    runtime_lifecycle::RuntimeLifecycle,
    schema_compatibility::{decide_open_mode, BinaryCompatibility, OpenMode, SchemaCompatibility},
    write_coordinator::WriteCoordinator,
    write_session::WriteSession,
};

pub(crate) use crate::persistence::write_coordinator::PersistenceVersion;

pub(crate) use crate::persistence::runtime_lifecycle::{RuntimeState, RuntimeTransitionError};

#[derive(Clone, Debug)]
pub(crate) struct PersistenceHandle {
    pool: SqlitePool,
    lifecycle: Arc<RuntimeLifecycle>,
    writes: Arc<WriteCoordinator>,
}

impl PersistenceHandle {
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "the request-log read model uses this version in application composition; some isolated persistence integration targets do not"
        )
    )]
    pub(crate) fn persistence_version(&self) -> PersistenceVersion {
        self.writes.persistence_version()
    }

    pub(crate) async fn begin_read(&self) -> Result<ReadSession, PersistenceError> {
        let runtime_permit = self
            .lifecycle
            .admit()
            .ok_or(PersistenceError::RuntimeUnavailable)?;
        Ok(ReadSession::new(self.pool.begin().await?, runtime_permit))
    }

    pub(crate) async fn begin_write(&self) -> Result<WriteSession, PersistenceError> {
        let runtime_permit = self
            .lifecycle
            .admit()
            .ok_or(PersistenceError::RuntimeUnavailable)?;
        let permit = self
            .writes
            .acquire()
            .await
            .map_err(|_| PersistenceError::RuntimeUnavailable)?;
        self.writes.record_session_started();
        let transaction = match self.pool.begin().await {
            Ok(transaction) => transaction,
            Err(error) => {
                self.writes.record_rollback();
                return Err(error.into());
            }
        };
        Ok(WriteSession::new(
            transaction,
            permit,
            self.writes.clone(),
            runtime_permit,
        ))
    }

    pub(crate) async fn write<T>(
        &self,
        operation: impl for<'session> FnOnce(
            &'session mut WriteSession,
        ) -> Pin<
            Box<dyn Future<Output = Result<T, PersistenceError>> + Send + 'session>,
        >,
    ) -> Result<T, PersistenceError> {
        let mut session = self.begin_write().await?;
        let output = operation(&mut session).await?;
        session.commit().await?;
        Ok(output)
    }

    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "backup behavior is exercised by the dedicated persistence session integration target"
    )]
    pub(crate) async fn backup_to(
        &self,
        final_path: &Path,
    ) -> Result<VerifiedBackup, PersistenceError> {
        let _runtime_permit = self
            .lifecycle
            .admit()
            .ok_or(PersistenceError::RuntimeUnavailable)?;
        create_verified_backup(&self.pool, final_path).await
    }

    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "write queue metrics are asserted by the dedicated runtime integration target"
    )]
    pub(crate) fn write_metrics(&self) -> WriteCoordinatorSnapshot {
        self.writes.snapshot()
    }
}

#[derive(Debug)]
pub(crate) struct PersistenceRuntime {
    handle: PersistenceHandle,
    compatibility: SchemaCompatibility,
    open_mode: OpenMode,
    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "compatibility details are asserted by the dedicated runtime integration target"
    )]
    decision_code: &'static str,
}

impl PersistenceRuntime {
    pub(crate) async fn initialize_new(path: &Path) -> Result<Self, PersistenceError> {
        crate::persistence::migrations::initialize_v2_database(path).await?;
        Self::open_current(path).await
    }

    pub(crate) async fn open_current(path: &Path) -> Result<Self, PersistenceError> {
        Self::open(
            path,
            crate::persistence::migrations::current_binary_compatibility(),
        )
        .await
    }

    pub(crate) async fn open(
        path: &Path,
        binary: BinaryCompatibility,
    ) -> Result<Self, PersistenceError> {
        if !path.is_file() {
            return Err(PersistenceError::MissingDatabase);
        }

        let options = connect_options(path, false)?;
        let mut connection = SqliteConnection::connect_with(&options).await?;
        configure_connection(&mut connection).await?;
        let compatibility =
            crate::persistence::schema_compatibility::load_schema_compatibility(&mut connection)
                .await?;
        let sqlx_version = applied_schema_version(&mut connection).await?;
        #[cfg(test)]
        let decision_code = crate::persistence::schema_compatibility::compatibility_decision_code(
            &binary,
            &compatibility,
            sqlx_version,
        )
        .as_code();
        let open_mode = decide_open_mode(&binary, &compatibility, sqlx_version)?;
        connection.close().await?;

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_secs(5))
            .idle_timeout(Duration::from_secs(300))
            .after_connect(|connection, _| Box::pin(configure_connection(connection)))
            .connect_with(options)
            .await?;
        record_runtime_open(&pool, open_mode).await?;
        let lifecycle = Arc::new(RuntimeLifecycle::new());
        lifecycle
            .transition(RuntimeState::Ready)
            .expect("ready state");
        Ok(Self {
            handle: PersistenceHandle {
                pool,
                lifecycle,
                writes: Arc::new(WriteCoordinator::new()),
            },
            compatibility,
            open_mode,
            #[cfg(test)]
            decision_code,
        })
    }

    pub(crate) fn handle(&self) -> PersistenceHandle {
        self.handle.clone()
    }

    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "runtime and performance integration targets exercise direct session admission"
    )]
    pub(crate) async fn begin_read(&self) -> Result<ReadSession, PersistenceError> {
        self.handle.begin_read().await
    }

    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "runtime and session integration targets exercise direct write admission"
    )]
    pub(crate) async fn begin_write(&self) -> Result<WriteSession, PersistenceError> {
        self.handle.begin_write().await
    }

    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "the runtime write facade is used by persistence integration targets"
    )]
    pub(crate) async fn write<T>(
        &self,
        operation: impl for<'session> FnOnce(
            &'session mut WriteSession,
        ) -> Pin<
            Box<dyn Future<Output = Result<T, PersistenceError>> + Send + 'session>,
        >,
    ) -> Result<T, PersistenceError> {
        self.handle.write(operation).await
    }

    pub(crate) async fn health(&self) -> Result<RuntimeHealth, PersistenceError> {
        let _runtime_permit = self
            .handle
            .lifecycle
            .admit()
            .ok_or(PersistenceError::RuntimeUnavailable)?;
        runtime_health(&self.handle.pool, self.open_mode, &self.compatibility).await
    }

    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "open mode is asserted by the dedicated runtime integration target"
    )]
    pub(crate) fn open_mode(&self) -> OpenMode {
        self.open_mode
    }

    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "schema metadata is asserted by the dedicated runtime integration target"
    )]
    pub(crate) fn compatibility(&self) -> &SchemaCompatibility {
        &self.compatibility
    }

    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "compatibility decisions are asserted by the dedicated runtime integration target"
    )]
    pub(crate) fn compatibility_decision_code(&self) -> &'static str {
        self.decision_code
    }

    fn current_state(&self) -> RuntimeState {
        self.handle.lifecycle.state()
    }

    #[cfg(test)]
    pub(crate) fn state(&self) -> RuntimeState {
        self.current_state()
    }

    pub(crate) async fn close(&self) -> Result<(), RuntimeTransitionError> {
        self.close_inner(|_| Ok(())).await
    }

    async fn close_inner(
        &self,
        fault: impl Fn(RuntimeCloseStep) -> Result<(), RuntimeTransitionError>,
    ) -> Result<(), RuntimeTransitionError> {
        match self.current_state() {
            RuntimeState::Closed => return Ok(()),
            RuntimeState::Ready => self.handle.lifecycle.transition(RuntimeState::Draining)?,
            RuntimeState::Draining => {}
            RuntimeState::Starting => {
                let _ = self.handle.lifecycle.transition(RuntimeState::Unavailable);
                self.handle.pool.close().await;
                return Err(RuntimeTransitionError::Invalid);
            }
            RuntimeState::Unavailable => {
                self.handle.pool.close().await;
                return Err(RuntimeTransitionError::CloseFailed);
            }
        }

        let before_close = fault(RuntimeCloseStep::BeforePoolClose);
        self.handle.lifecycle.wait_for_idle().await;
        self.handle.pool.close().await;
        let close_result = before_close.and_then(|_| fault(RuntimeCloseStep::AfterPoolClose));
        if let Err(error) = close_result {
            let _ = self.handle.lifecycle.transition(RuntimeState::Unavailable);
            return Err(error);
        }
        self.handle.lifecycle.transition(RuntimeState::Closed)
    }

    #[cfg(test)]
    async fn close_with_fault(
        &self,
        fail_at: RuntimeCloseStep,
    ) -> Result<(), RuntimeTransitionError> {
        self.close_inner(|step| {
            if step == fail_at {
                Err(RuntimeTransitionError::CloseFailed)
            } else {
                Ok(())
            }
        })
        .await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeCloseStep {
    BeforePoolClose,
    AfterPoolClose,
}

fn connect_options(
    path: &Path,
    create_if_missing: bool,
) -> Result<SqliteConnectOptions, sqlx::Error> {
    Ok(SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create_if_missing)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5)))
}

async fn configure_connection(connection: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    connection.execute("PRAGMA foreign_keys = ON").await?;
    connection.execute("PRAGMA synchronous = FULL").await?;
    connection.execute("PRAGMA busy_timeout = 5000").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use crate::persistence::error::PersistenceError;

    use super::{
        PersistenceRuntime, RuntimeCloseStep, RuntimeLifecycle, RuntimeState,
        RuntimeTransitionError,
    };

    #[test]
    fn runtime_lifecycle_is_monotonic() {
        let state = RuntimeLifecycle::new();
        assert_eq!(state.transition(RuntimeState::Ready), Ok(()));
        assert_eq!(
            state.transition(RuntimeState::Starting),
            Err(RuntimeTransitionError::Reverse)
        );
        assert_eq!(state.transition(RuntimeState::Draining), Ok(()));
        assert!(!state.accepts_new_work());
        assert_eq!(state.transition(RuntimeState::Closed), Ok(()));
    }

    #[test]
    fn runtime_lifecycle_rejects_skipped_and_terminal_transitions() {
        let state = RuntimeLifecycle::new();
        assert_eq!(
            state.transition(RuntimeState::Draining),
            Err(RuntimeTransitionError::Invalid)
        );
        assert_eq!(state.transition(RuntimeState::Ready), Ok(()));
        assert_eq!(
            state.transition(RuntimeState::Closed),
            Err(RuntimeTransitionError::Invalid)
        );
        assert_eq!(state.transition(RuntimeState::Unavailable), Ok(()));
        assert_eq!(
            state.transition(RuntimeState::Closed),
            Err(RuntimeTransitionError::Reverse)
        );
    }

    #[tokio::test]
    async fn runtime_close_drains_checked_out_connections_before_closed() {
        let root = tempfile::tempdir().expect("temp directory");
        let path = root.path().join("runtime.sqlite3");
        let runtime = Arc::new(
            PersistenceRuntime::initialize_new(&path)
                .await
                .expect("initialize runtime"),
        );
        let session = runtime.begin_read().await.expect("begin tracked read");
        let closing_runtime = Arc::clone(&runtime);
        let closing = tokio::spawn(async move { closing_runtime.close().await });

        for _ in 0..100 {
            if runtime.state() == RuntimeState::Draining {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(runtime.state(), RuntimeState::Draining);
        assert!(matches!(
            runtime.begin_read().await,
            Err(PersistenceError::RuntimeUnavailable)
        ));
        assert!(!closing.is_finished());

        drop(session);
        closing.await.expect("close task").expect("close runtime");
        assert_eq!(runtime.state(), RuntimeState::Closed);
        assert!(runtime.handle.pool.is_closed());
    }

    #[tokio::test]
    async fn runtime_close_fault_closes_pool_and_marks_runtime_unavailable() {
        for fail_at in [
            RuntimeCloseStep::BeforePoolClose,
            RuntimeCloseStep::AfterPoolClose,
        ] {
            let root = tempfile::tempdir().expect("temp directory");
            let path = root.path().join("runtime.sqlite3");
            let runtime = PersistenceRuntime::initialize_new(&path)
                .await
                .expect("initialize runtime");

            assert_eq!(
                runtime.close_with_fault(fail_at).await,
                Err(RuntimeTransitionError::CloseFailed)
            );
            assert_eq!(runtime.state(), RuntimeState::Unavailable);
            assert!(runtime.handle.pool.is_closed());
            assert!(matches!(
                runtime.begin_write().await,
                Err(PersistenceError::RuntimeUnavailable)
            ));
        }
    }
}
