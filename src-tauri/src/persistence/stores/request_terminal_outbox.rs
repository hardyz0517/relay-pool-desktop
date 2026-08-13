use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

use super::request_log_write::RequestTerminalWrite;

const MAX_PAYLOAD_BYTES: usize = 32 * 1024;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct RequestTerminalOutboxStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OutboxDrainBatch {
    pub(crate) claimed: u64,
    pub(crate) has_more: bool,
}

impl RequestTerminalOutboxStore {
    pub(crate) async fn enqueue(
        &self,
        connection: &mut SqliteConnection,
        record: &RequestTerminalWrite,
        created_at_ms: i64,
    ) -> Result<(), PersistenceError> {
        let payload_json = serde_json::to_string(record).map_err(|error| {
            PersistenceError::InvariantViolation(format!(
                "terminal outbox serialization failed: {error}"
            ))
        })?;
        if payload_json.len() > MAX_PAYLOAD_BYTES {
            return Err(PersistenceError::ConstraintViolation);
        }
        // The first projection owns the audit timestamp. A repeat may observe
        // a later clock value without changing the terminal's semantics.
        let payload_sha256 = terminal_semantics_sha256(record)?;
        let existing =
            sqlx::query("SELECT payload_json FROM request_terminal_outbox WHERE request_id = ?1")
                .bind(&record.request_id)
                .fetch_optional(&mut *connection)
                .await?;
        if let Some(existing) = existing {
            let payload_json: String = existing.get(0);
            let existing_record: RequestTerminalWrite = serde_json::from_str(&payload_json)
                .map_err(|_| {
                    PersistenceError::InvariantViolation(
                        "request terminal outbox contains invalid canonical payload".to_string(),
                    )
                })?;
            if terminal_semantics_sha256(&existing_record)? != payload_sha256 {
                return Err(PersistenceError::InvariantViolation(
                    "request terminal outbox payload collision".to_string(),
                ));
            }
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO request_terminal_outbox (
                request_id, payload_json, payload_sha256, created_at_ms, attempts
             ) VALUES (?1, ?2, ?3, ?4, 0)",
        )
        .bind(&record.request_id)
        .bind(payload_json)
        .bind(payload_sha256)
        .bind(created_at_ms)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn claim_batch(
        &self,
        connection: &mut SqliteConnection,
        owner: &str,
        now_ms: i64,
        lease_ms: i64,
        batch_size: u32,
    ) -> Result<(Vec<RequestTerminalWrite>, OutboxDrainBatch), PersistenceError> {
        let limit = i64::from(batch_size.clamp(1, 256).saturating_add(1));
        let rows = sqlx::query(
            "SELECT request_id, payload_json, payload_sha256 FROM request_terminal_outbox
             WHERE lease_expires_at_ms IS NULL OR lease_expires_at_ms <= ?1
             ORDER BY request_id ASC LIMIT ?2",
        )
        .bind(now_ms)
        .bind(limit)
        .fetch_all(&mut *connection)
        .await?;
        let requested = usize::try_from(limit - 1).unwrap_or(usize::MAX);
        let has_more = rows.len() > requested;
        let mut records = Vec::with_capacity(rows.len().min(requested));
        for row in rows.into_iter().take(requested) {
            let request_id: String = row.get(0);
            let payload_json: String = row.get(1);
            let payload_sha256: String = row.get(2);
            let claimed = sqlx::query(
                "UPDATE request_terminal_outbox
                 SET lease_owner = ?1, lease_expires_at_ms = ?2, attempts = attempts + 1
                 WHERE request_id = ?3
                   AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms <= ?4)",
            )
            .bind(owner)
            .bind(now_ms.saturating_add(lease_ms.max(1)))
            .bind(&request_id)
            .bind(now_ms)
            .execute(&mut *connection)
            .await?
            .rows_affected();
            if claimed == 0 {
                continue;
            }
            let record: RequestTerminalWrite =
                serde_json::from_str(&payload_json).map_err(|_| {
                    PersistenceError::InvariantViolation(
                        "request terminal outbox contains invalid canonical payload".to_string(),
                    )
                })?;
            let raw_payload_sha256 = format!("{:x}", Sha256::digest(payload_json.as_bytes()));
            if terminal_semantics_sha256(&record)? != payload_sha256
                && raw_payload_sha256 != payload_sha256
            {
                return Err(PersistenceError::InvariantViolation(
                    "request terminal outbox payload digest mismatch".to_string(),
                ));
            }
            if record.request_id != request_id {
                return Err(PersistenceError::InvariantViolation(
                    "request terminal outbox request identity mismatch".to_string(),
                ));
            }
            records.push(record);
        }
        let claimed = records.len() as u64;
        Ok((records, OutboxDrainBatch { claimed, has_more }))
    }

    pub(crate) async fn delete_claimed(
        &self,
        connection: &mut SqliteConnection,
        request_id: &str,
        owner: &str,
    ) -> Result<(), PersistenceError> {
        let deleted = sqlx::query(
            "DELETE FROM request_terminal_outbox WHERE request_id = ?1 AND lease_owner = ?2",
        )
        .bind(request_id)
        .bind(owner)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if deleted != 1 {
            return Err(PersistenceError::InvariantViolation(
                "request terminal outbox lease was lost during projection".to_string(),
            ));
        }
        Ok(())
    }
}

fn terminal_semantics_sha256(record: &RequestTerminalWrite) -> Result<String, PersistenceError> {
    let mut canonical = record.clone();
    canonical.terminal_at_ms = 0;
    canonical.routing_outcome.terminal_at_ms = 0;
    let payload_json = serde_json::to_string(&canonical).map_err(|error| {
        PersistenceError::InvariantViolation(format!(
            "terminal outbox semantic serialization failed: {error}"
        ))
    })?;
    Ok(format!("{:x}", Sha256::digest(payload_json.as_bytes())))
}
