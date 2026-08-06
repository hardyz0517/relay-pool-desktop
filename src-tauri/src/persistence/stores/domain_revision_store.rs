use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DomainRevision {
    pub(crate) scope: String,
    pub(crate) revision: u64,
    pub(crate) updated_at_ms: i64,
    pub(crate) provenance: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DomainRevisionStore;

impl DomainRevisionStore {
    pub(crate) async fn load(
        &self,
        connection: &mut SqliteConnection,
        scope: &str,
    ) -> Result<DomainRevision, PersistenceError> {
        validate_scope(scope)?;
        let row = sqlx::query(
            "SELECT scope, revision, updated_at_ms, provenance FROM domain_revisions WHERE scope = ?1",
        )
        .bind(scope)
        .fetch_optional(connection)
        .await?;
        row.map(read_revision)
            .transpose()?
            .ok_or_else(|| PersistenceError::RevisionUnavailable(scope.to_string()))
    }

    /// Advances a pre-existing revision inside the caller's write transaction.
    /// The compare-and-swap prevents same-millisecond writes from collapsing.
    pub(crate) async fn advance(
        &self,
        connection: &mut SqliteConnection,
        scope: &str,
        expected_revision: u64,
        updated_at_ms: i64,
    ) -> Result<DomainRevision, PersistenceError> {
        validate_scope(scope)?;
        if expected_revision == 0 || updated_at_ms < 0 {
            return Err(PersistenceError::InvariantViolation(
                "domain revision advance has an invalid expected revision or timestamp".into(),
            ));
        }
        let expected = i64::try_from(expected_revision).map_err(|_| {
            PersistenceError::InvariantViolation("domain revision exceeds SQLite range".into())
        })?;
        let next = expected.checked_add(1).ok_or_else(|| {
            PersistenceError::InvariantViolation("domain revision overflow".into())
        })?;
        let changed = sqlx::query(
            "UPDATE domain_revisions
             SET revision = ?1, updated_at_ms = ?2, provenance = 'transactional_write'
             WHERE scope = ?3 AND revision = ?4",
        )
        .bind(next)
        .bind(updated_at_ms)
        .bind(scope)
        .bind(expected)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if changed == 0 {
            return match self.load(connection, scope).await {
                Err(PersistenceError::RevisionUnavailable(_)) => {
                    Err(PersistenceError::RevisionUnavailable(scope.to_string()))
                }
                Err(error) => Err(error),
                Ok(_) => Err(PersistenceError::RevisionConflict(scope.to_string())),
            };
        }
        self.load(connection, scope).await
    }
}

fn read_revision(row: sqlx::sqlite::SqliteRow) -> Result<DomainRevision, PersistenceError> {
    let revision = row.get::<i64, _>("revision");
    let updated_at_ms = row.get::<i64, _>("updated_at_ms");
    if revision <= 0 || updated_at_ms < 0 {
        return Err(PersistenceError::InvariantViolation(
            "stored domain revision is invalid".into(),
        ));
    }
    Ok(DomainRevision {
        scope: row.get("scope"),
        revision: u64::try_from(revision).map_err(|_| {
            PersistenceError::InvariantViolation("stored domain revision is negative".into())
        })?,
        updated_at_ms,
        provenance: row.get("provenance"),
    })
}

fn validate_scope(scope: &str) -> Result<(), PersistenceError> {
    if scope.is_empty() || scope.len() > 192 || scope.chars().any(char::is_control) {
        return Err(PersistenceError::InvariantViolation(
            "domain revision scope is invalid".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::{Connection, Executor, SqliteConnection};

    use super::DomainRevisionStore;
    use crate::persistence::{error::PersistenceError, migrations::migrator};

    #[tokio::test]
    async fn revisions_are_monotonic_even_when_writes_share_a_millisecond() {
        let mut connection = SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open memory database");
        migrator()
            .run(&mut connection)
            .await
            .expect("migrate foundation");
        connection
            .execute(
                "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
                 VALUES ('station:fixture', 7, 42, 'baseline_snapshot')",
            )
            .await
            .expect("insert fixture revision");

        let store = DomainRevisionStore;
        let first = store
            .advance(&mut connection, "station:fixture", 7, 42)
            .await
            .expect("first same-millisecond write");
        let second = store
            .advance(&mut connection, "station:fixture", 8, 42)
            .await
            .expect("second same-millisecond write");

        assert_eq!(first.revision, 8);
        assert_eq!(second.revision, 9);
        assert_eq!(second.updated_at_ms, 42);
        assert!(matches!(
            store.advance(&mut connection, "station:fixture", 7, 42).await,
            Err(PersistenceError::RevisionConflict(scope)) if scope == "station:fixture"
        ));
        assert!(matches!(
            store.load(&mut connection, "station:missing").await,
            Err(PersistenceError::RevisionUnavailable(scope)) if scope == "station:missing"
        ));
    }
}
