use sqlx::{Sqlite, SqliteConnection, Transaction};

use crate::persistence::runtime_lifecycle::RuntimeWorkPermit;

pub(crate) struct ReadSession {
    transaction: Option<Transaction<'static, Sqlite>>,
    _runtime_permit: RuntimeWorkPermit,
}

impl ReadSession {
    pub(crate) fn new(
        transaction: Transaction<'static, Sqlite>,
        runtime_permit: RuntimeWorkPermit,
    ) -> Self {
        Self {
            transaction: Some(transaction),
            _runtime_permit: runtime_permit,
        }
    }

    pub(crate) fn connection(&mut self) -> &mut SqliteConnection {
        let transaction = self
            .transaction
            .as_mut()
            .expect("read session used after close");
        &mut *transaction
    }
}
