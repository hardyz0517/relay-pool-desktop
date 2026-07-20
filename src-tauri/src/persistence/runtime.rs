use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};

use rusqlite::Connection;
use tokio::time::Duration;

use super::{
    error::PersistenceError, migrations::apply_migrations, write_coordinator::WriteCoordinator,
    write_session::WriteSession,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PersistenceState {
    Starting,
    Ready,
    Draining,
    Closed,
    Unavailable,
}

#[derive(Debug, Clone)]
pub(crate) struct PersistenceRuntimeConfig {
    pub db_path: PathBuf,
    pub busy_timeout: Duration,
}

impl PersistenceRuntimeConfig {
    pub(crate) fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            busy_timeout: Duration::from_secs(5),
        }
    }
}

pub(crate) struct PersistenceRuntime {
    coordinator: WriteCoordinator,
    state: Arc<AtomicU8>,
}

impl PersistenceRuntime {
    pub(crate) async fn open(config: PersistenceRuntimeConfig) -> Result<Self, PersistenceError> {
        let connection = open_connection(&config.db_path, config.busy_timeout)?;
        apply_migrations(&connection)?;
        drop(connection);

        Ok(Self {
            coordinator: WriteCoordinator::new(config.db_path, config.busy_timeout),
            state: Arc::new(AtomicU8::new(state_to_u8(PersistenceState::Ready))),
        })
    }

    pub(crate) fn begin_write(&self) -> Result<WriteSession, PersistenceError> {
        match self.state() {
            PersistenceState::Ready => self.coordinator.begin(),
            PersistenceState::Starting => Err(PersistenceError::Unavailable(
                "persistence is still starting".to_string(),
            )),
            PersistenceState::Draining => Err(PersistenceError::Draining),
            PersistenceState::Closed => Err(PersistenceError::Unavailable(
                "persistence has been closed".to_string(),
            )),
            PersistenceState::Unavailable => Err(PersistenceError::Unavailable(
                "persistence is unavailable".to_string(),
            )),
        }
    }

    pub(crate) fn begin_draining(&self) {
        self.state
            .store(state_to_u8(PersistenceState::Draining), Ordering::Release);
    }

    pub(crate) fn close(&self) -> Result<(), PersistenceError> {
        self.state
            .store(state_to_u8(PersistenceState::Closed), Ordering::Release);
        Ok(())
    }

    pub(crate) fn state(&self) -> PersistenceState {
        match self.state.load(Ordering::Acquire) {
            0 => PersistenceState::Starting,
            1 => PersistenceState::Ready,
            2 => PersistenceState::Draining,
            3 => PersistenceState::Closed,
            _ => PersistenceState::Unavailable,
        }
    }

    pub(crate) fn db_path(&self) -> &Path {
        self.coordinator.db_path()
    }
}

fn open_connection(db_path: &Path, busy_timeout: Duration) -> Result<Connection, PersistenceError> {
    let connection = Connection::open(db_path)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.busy_timeout(busy_timeout)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    Ok(connection)
}

fn state_to_u8(state: PersistenceState) -> u8 {
    match state {
        PersistenceState::Starting => 0,
        PersistenceState::Ready => 1,
        PersistenceState::Draining => 2,
        PersistenceState::Closed => 3,
        PersistenceState::Unavailable => 4,
    }
}
