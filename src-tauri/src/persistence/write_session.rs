use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;
use tokio::sync::OwnedSemaphorePermit;

use super::error::PersistenceError;

pub(crate) struct WriteSession {
    connection: Connection,
    _permit: Option<OwnedSemaphorePermit>,
}

pub(crate) struct WriteSessionCommit;

impl WriteSession {
    pub(crate) fn open(db_path: &Path, busy_timeout: Duration) -> Result<Self, PersistenceError> {
        let connection = Connection::open(db_path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(busy_timeout)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(Self {
            connection,
            _permit: None,
        })
    }

    pub(crate) fn connection(&mut self) -> &mut Connection {
        &mut self.connection
    }

    pub(crate) fn commit(self) -> Result<WriteSessionCommit, PersistenceError> {
        Ok(WriteSessionCommit)
    }
}
