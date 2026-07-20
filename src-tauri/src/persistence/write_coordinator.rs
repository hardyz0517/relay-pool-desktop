use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use super::{error::PersistenceError, write_session::WriteSession};

pub(crate) struct WriteCoordinator {
    db_path: PathBuf,
    busy_timeout: Duration,
}

impl WriteCoordinator {
    pub(crate) fn new(db_path: PathBuf, busy_timeout: Duration) -> Self {
        Self {
            db_path,
            busy_timeout,
        }
    }

    pub(crate) fn begin(&self) -> Result<WriteSession, PersistenceError> {
        WriteSession::open(&self.db_path, self.busy_timeout)
    }

    pub(crate) fn db_path(&self) -> &Path {
        &self.db_path
    }
}
