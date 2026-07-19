use sqlx::{Sqlite, SqliteConnection, Transaction};

pub(crate) struct ReadSession {
    transaction: Option<Transaction<'static, Sqlite>>,
}

impl ReadSession {
    pub(crate) fn new(transaction: Transaction<'static, Sqlite>) -> Self {
        Self {
            transaction: Some(transaction),
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
