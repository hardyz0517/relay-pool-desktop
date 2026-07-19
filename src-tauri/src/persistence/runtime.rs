use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Connection, Executor, SqliteConnection, SqlitePool,
};

use crate::persistence::{
    error::PersistenceError,
    health_check::{record_runtime_open, runtime_health, RuntimeHealth},
    migrations::applied_schema_version,
    schema_compatibility::{
        compatibility_decision_code, decide_open_mode, BinaryCompatibility, OpenMode,
        SchemaCompatibility,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RuntimeState {
    Starting,
    Ready,
    Draining,
    Closed,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeTransitionError {
    Reverse,
}

#[derive(Debug)]
pub(crate) struct RuntimeLifecycle {
    state: Mutex<RuntimeState>,
}

impl RuntimeLifecycle {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(RuntimeState::Starting),
        }
    }

    pub(crate) fn transition(&self, next: RuntimeState) -> Result<(), RuntimeTransitionError> {
        let mut state = self.state.lock().expect("runtime lifecycle poisoned");
        if rank(next) < rank(*state) {
            return Err(RuntimeTransitionError::Reverse);
        }
        *state = next;
        Ok(())
    }

    pub(crate) fn accepts_new_work(&self) -> bool {
        matches!(
            *self.state.lock().expect("runtime lifecycle poisoned"),
            RuntimeState::Ready
        )
    }
}

fn rank(state: RuntimeState) -> u8 {
    match state {
        RuntimeState::Starting => 0,
        RuntimeState::Ready => 1,
        RuntimeState::Draining => 2,
        RuntimeState::Closed => 3,
        RuntimeState::Unavailable => 4,
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PersistenceHandle {
    pool: SqlitePool,
    lifecycle: Arc<RuntimeLifecycle>,
}

#[derive(Debug)]
pub(crate) struct PersistenceRuntime {
    handle: PersistenceHandle,
    compatibility: SchemaCompatibility,
    open_mode: OpenMode,
    decision_code: &'static str,
}

impl PersistenceRuntime {
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
        let decision = compatibility_decision_code(&binary, &compatibility, sqlx_version);
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
            handle: PersistenceHandle { pool, lifecycle },
            compatibility,
            open_mode,
            decision_code: decision.as_code(),
        })
    }

    pub(crate) fn open_mode(&self) -> OpenMode {
        self.open_mode
    }

    pub(crate) fn compatibility(&self) -> &SchemaCompatibility {
        &self.compatibility
    }

    pub(crate) fn compatibility_decision_code(&self) -> &'static str {
        self.decision_code
    }

    pub(crate) async fn health(&self) -> Result<RuntimeHealth, PersistenceError> {
        let _accepting = self.handle.lifecycle.accepts_new_work();
        runtime_health(&self.handle.pool, self.open_mode, &self.compatibility).await
    }
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
    use super::{RuntimeLifecycle, RuntimeState, RuntimeTransitionError};

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
}
