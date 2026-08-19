//! SQL-only durable document synchronization projection.
//!
//! This is a coalescing per-document-kind record, not a FIFO outbox. A future
//! shared coordinator may claim and materialize the latest desired target;
//! document compilers and file watchers stay outside this persistence boundary.

use sqlx::{Row, SqliteConnection};

use crate::{models::document_sync::DocumentSyncState, persistence::error::PersistenceError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredDocumentSync {
    pub(crate) document_kind: String,
    pub(crate) desired_revision: u64,
    pub(crate) desired_canonical_digest: Option<String>,
    pub(crate) materialized_revision: Option<u64>,
    pub(crate) materialized_canonical_digest: Option<String>,
    pub(crate) state: DocumentSyncState,
    pub(crate) last_observed_raw_digest: Option<String>,
    pub(crate) last_error_code: Option<String>,
    pub(crate) retry_count: u32,
    pub(crate) attempt_token: Option<String>,
    pub(crate) lease_owner: Option<String>,
    pub(crate) lease_expires_at_ms: Option<i64>,
    pub(crate) updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DocumentSyncStore;

impl DocumentSyncStore {
    pub(crate) async fn load(
        &self,
        connection: &mut SqliteConnection,
        document_kind: &str,
    ) -> Result<Option<StoredDocumentSync>, PersistenceError> {
        validate_document_kind(document_kind)?;
        let row = sqlx::query(
            "SELECT document_kind, desired_revision, desired_canonical_digest,
                    materialized_revision, materialized_canonical_digest, sync_state,
                    last_observed_raw_digest, last_error_code, retry_count,
                    attempt_token, lease_owner, lease_expires_at_ms, updated_at_ms
             FROM routing_document_sync
             WHERE document_kind = ?1",
        )
        .bind(document_kind)
        .fetch_optional(&mut *connection)
        .await?;
        row.map(read_row).transpose()
    }

    /// Upsert the newest desired target while retaining materialization evidence.
    pub(crate) async fn upsert_desired(
        &self,
        connection: &mut SqliteConnection,
        document_kind: &str,
        desired_revision: u64,
        desired_canonical_digest: Option<&str>,
        now_ms: i64,
    ) -> Result<StoredDocumentSync, PersistenceError> {
        validate_document_kind(document_kind)?;
        validate_revision(desired_revision)?;
        validate_digest(desired_canonical_digest)?;
        validate_timestamp(now_ms)?;
        let desired_revision = sqlite_revision(desired_revision)?;
        sqlx::query(
            "INSERT INTO routing_document_sync (
                document_kind, desired_revision, desired_canonical_digest,
                materialized_revision, materialized_canonical_digest, sync_state,
                last_observed_raw_digest, last_error_code, retry_count,
                attempt_token, lease_owner, lease_expires_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, NULL, NULL, 'pending_materialization',
                       NULL, NULL, 0, NULL, NULL, NULL, ?4)
             ON CONFLICT(document_kind) DO UPDATE SET
                desired_revision = excluded.desired_revision,
                desired_canonical_digest = excluded.desired_canonical_digest,
                sync_state = CASE
                    WHEN routing_document_sync.materialized_revision = excluded.desired_revision
                     AND (routing_document_sync.materialized_canonical_digest = excluded.desired_canonical_digest
                          OR (routing_document_sync.materialized_canonical_digest IS NULL
                              AND excluded.desired_canonical_digest IS NULL))
                    THEN 'synchronized'
                    ELSE 'pending_materialization'
                END,
                last_error_code = NULL,
                retry_count = CASE
                    WHEN routing_document_sync.desired_revision <> excluded.desired_revision THEN 0
                    ELSE routing_document_sync.retry_count
                END,
                attempt_token = NULL,
                lease_owner = NULL,
                lease_expires_at_ms = NULL,
                updated_at_ms = excluded.updated_at_ms
             WHERE excluded.desired_revision >= routing_document_sync.desired_revision",
        )
        .bind(document_kind)
        .bind(desired_revision)
        .bind(desired_canonical_digest)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        self.load(connection, document_kind).await?.ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "document sync row disappeared after upsert".into(),
            )
        })
    }

    /// Mark a target materialized only when it is still the current desired revision.
    pub(crate) async fn mark_materialized(
        &self,
        connection: &mut SqliteConnection,
        document_kind: &str,
        materialized_revision: u64,
        materialized_canonical_digest: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, PersistenceError> {
        validate_document_kind(document_kind)?;
        validate_revision(materialized_revision)?;
        validate_digest(materialized_canonical_digest)?;
        validate_timestamp(now_ms)?;
        let revision = sqlite_revision(materialized_revision)?;
        let changed = sqlx::query(
            "UPDATE routing_document_sync
             SET materialized_revision = ?1,
                 materialized_canonical_digest = ?2,
                 sync_state = CASE
                    WHEN desired_revision = ?1
                     AND (desired_canonical_digest = ?2
                          OR (desired_canonical_digest IS NULL AND ?2 IS NULL))
                    THEN 'synchronized'
                    ELSE 'pending_materialization'
                 END,
                 last_error_code = NULL,
                 attempt_token = NULL,
                 lease_owner = NULL,
                 lease_expires_at_ms = NULL,
                 updated_at_ms = ?3
             WHERE document_kind = ?4 AND desired_revision = ?1",
        )
        .bind(revision)
        .bind(materialized_canonical_digest)
        .bind(now_ms)
        .bind(document_kind)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    pub(crate) async fn mark_external_change(
        &self,
        connection: &mut SqliteConnection,
        document_kind: &str,
        raw_digest: Option<&str>,
        error_code: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, PersistenceError> {
        self.mark_observation(
            connection,
            document_kind,
            DocumentSyncState::ExternalChange,
            raw_digest,
            error_code,
            now_ms,
        )
        .await
    }

    pub(crate) async fn mark_error(
        &self,
        connection: &mut SqliteConnection,
        document_kind: &str,
        error_code: &str,
        now_ms: i64,
    ) -> Result<bool, PersistenceError> {
        self.mark_observation(
            connection,
            document_kind,
            DocumentSyncState::Error,
            None,
            Some(error_code),
            now_ms,
        )
        .await
    }

    async fn mark_observation(
        &self,
        connection: &mut SqliteConnection,
        document_kind: &str,
        state: DocumentSyncState,
        raw_digest: Option<&str>,
        error_code: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, PersistenceError> {
        validate_document_kind(document_kind)?;
        validate_digest(raw_digest)?;
        validate_error_code(error_code)?;
        validate_timestamp(now_ms)?;
        let changed = sqlx::query(
            "UPDATE routing_document_sync
             SET sync_state = ?1, last_observed_raw_digest = ?2,
                 last_error_code = ?3, updated_at_ms = ?4
             WHERE document_kind = ?5",
        )
        .bind(state.as_str())
        .bind(raw_digest)
        .bind(error_code)
        .bind(now_ms)
        .bind(document_kind)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }
}

fn read_row(row: sqlx::sqlite::SqliteRow) -> Result<StoredDocumentSync, PersistenceError> {
    let desired_revision = row.get::<i64, _>("desired_revision");
    let materialized_revision = row.get::<Option<i64>, _>("materialized_revision");
    let retry_count = row.get::<i64, _>("retry_count");
    let updated_at_ms = row.get::<i64, _>("updated_at_ms");
    if desired_revision <= 0
        || materialized_revision.is_some_and(|revision| revision <= 0)
        || retry_count < 0
        || retry_count > i64::from(u32::MAX)
        || updated_at_ms < 0
    {
        return Err(PersistenceError::InvariantViolation(
            "document sync metadata is invalid".into(),
        ));
    }
    let state_value: String = row.get("sync_state");
    let state = DocumentSyncState::parse(&state_value).ok_or_else(|| {
        PersistenceError::InvariantViolation("document sync state is invalid".into())
    })?;
    let desired_canonical_digest: Option<String> = row.get("desired_canonical_digest");
    let materialized_canonical_digest: Option<String> = row.get("materialized_canonical_digest");
    let last_observed_raw_digest: Option<String> = row.get("last_observed_raw_digest");
    validate_digest(desired_canonical_digest.as_deref())?;
    validate_digest(materialized_canonical_digest.as_deref())?;
    validate_digest(last_observed_raw_digest.as_deref())?;
    let last_error_code: Option<String> = row.get("last_error_code");
    validate_error_code(last_error_code.as_deref())?;
    Ok(StoredDocumentSync {
        document_kind: row.get("document_kind"),
        desired_revision: u64::try_from(desired_revision).map_err(|_| {
            PersistenceError::InvariantViolation("document sync revision is invalid".into())
        })?,
        desired_canonical_digest,
        materialized_revision: materialized_revision
            .map(|revision| {
                u64::try_from(revision).map_err(|_| {
                    PersistenceError::InvariantViolation("document sync revision is invalid".into())
                })
            })
            .transpose()?,
        materialized_canonical_digest,
        state,
        last_observed_raw_digest,
        last_error_code,
        retry_count: u32::try_from(retry_count).map_err(|_| {
            PersistenceError::InvariantViolation("document sync retry count is invalid".into())
        })?,
        attempt_token: row.get("attempt_token"),
        lease_owner: row.get("lease_owner"),
        lease_expires_at_ms: row.get("lease_expires_at_ms"),
        updated_at_ms,
    })
}

fn validate_document_kind(value: &str) -> Result<(), PersistenceError> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn validate_revision(value: u64) -> Result<(), PersistenceError> {
    if value == 0 {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn sqlite_revision(value: u64) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| {
        PersistenceError::InvariantViolation("document sync revision exceeds SQLite range".into())
    })
}

fn validate_digest(value: Option<&str>) -> Result<(), PersistenceError> {
    if value.is_some_and(|digest| {
        digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn validate_error_code(value: Option<&str>) -> Result<(), PersistenceError> {
    if value.is_some_and(|code| {
        code.is_empty() || code.len() > 96 || code.chars().any(char::is_control)
    }) {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn validate_timestamp(value: i64) -> Result<(), PersistenceError> {
    if value < 0 {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::{Connection, SqliteConnection};

    use super::DocumentSyncStore;
    use crate::{
        models::document_sync::{
            DocumentSyncState, MODEL_MAPPING_DOCUMENT_KIND, ROUTING_POLICY_DOCUMENT_KIND,
        },
        persistence::migrations::migrator,
    };

    #[tokio::test]
    async fn document_kind_rows_coalesce_and_reject_stale_materialization() {
        let mut connection = SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open sqlite");
        migrator().run(&mut connection).await.expect("migrate");
        let store = DocumentSyncStore;
        let initial = store
            .load(&mut connection, MODEL_MAPPING_DOCUMENT_KIND)
            .await
            .expect("load mapping sync")
            .expect("migration row");
        assert_eq!(initial.desired_revision, 1);
        assert_eq!(initial.state, DocumentSyncState::PendingMaterialization);
        assert!(initial.materialized_revision.is_none());
        let policy = store
            .load(&mut connection, ROUTING_POLICY_DOCUMENT_KIND)
            .await
            .expect("load routing policy sync")
            .expect("routing policy migration row");
        assert_eq!(policy.desired_revision, 1);
        assert_eq!(policy.state, DocumentSyncState::PendingMaterialization);

        let desired = store
            .upsert_desired(
                &mut connection,
                MODEL_MAPPING_DOCUMENT_KIND,
                2,
                Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                10,
            )
            .await
            .expect("upsert desired");
        assert_eq!(desired.desired_revision, 2);
        assert_eq!(desired.retry_count, 0);
        assert_eq!(desired.state, DocumentSyncState::PendingMaterialization);

        assert!(!store
            .mark_materialized(&mut connection, MODEL_MAPPING_DOCUMENT_KIND, 1, None, 11,)
            .await
            .expect("stale materialization"));
        assert!(store
            .mark_materialized(
                &mut connection,
                MODEL_MAPPING_DOCUMENT_KIND,
                2,
                Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                12,
            )
            .await
            .expect("current materialization"));
        let synchronized = store
            .load(&mut connection, MODEL_MAPPING_DOCUMENT_KIND)
            .await
            .expect("reload")
            .expect("row");
        assert_eq!(synchronized.state, DocumentSyncState::Synchronized);
        assert_eq!(synchronized.materialized_revision, Some(2));

        let stale_desired = store
            .upsert_desired(
                &mut connection,
                MODEL_MAPPING_DOCUMENT_KIND,
                1,
                Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
                13,
            )
            .await
            .expect("stale desired coalesces without regression");
        assert_eq!(stale_desired.desired_revision, 2);
        assert_eq!(
            stale_desired.desired_canonical_digest.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );

        assert!(store.load(&mut connection, "bad\nkind").await.is_err());
        assert!(store
            .upsert_desired(
                &mut connection,
                MODEL_MAPPING_DOCUMENT_KIND,
                3,
                Some("not-a-digest"),
                13,
            )
            .await
            .is_err());
    }
}
