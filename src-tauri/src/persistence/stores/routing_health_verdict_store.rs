use std::collections::BTreeSet;

use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

const MAX_BATCH_SUBJECTS: usize = 4_096;
const NATIVE_MODEL_CAPABILITY_IDENTITY_VERSION: i64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityObservationApplyResult {
    Applied,
    Existing,
}

#[derive(Debug, Clone)]
pub(crate) struct UnsupportedModelObservation {
    pub(crate) observation_id: String,
    pub(crate) logical_request_id: String,
    pub(crate) attempt_ordinal: u8,
    pub(crate) station_key_id: String,
    pub(crate) resolved_model: String,
    pub(crate) credential_revision: i64,
    pub(crate) endpoint_revision: i64,
    /// Historical alias revision is provenance, not native capability identity.
    pub(crate) model_alias_revision: i64,
    pub(crate) endpoint_kind: String,
    pub(crate) protocol_kind: String,
    pub(crate) model_mapping_revision: Option<i64>,
    pub(crate) model_resolution_fence: Option<String>,
    pub(crate) evidence_code: String,
    pub(crate) classifier_profile_version: String,
}

/// Durable owner for learned model-on-key capability evidence.
///
/// The historical module name remains stable until the schema compatibility
/// window closes, but this API deliberately contains no health breaker,
/// error-rate reducer, or probe lifecycle responsibilities.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingHealthVerdictStore;

impl RoutingHealthVerdictStore {
    /// Loads unsupported-model verdicts for the native model and lifecycle
    /// tuples supplied by the planner. The final tuple slot is retained for
    /// source compatibility; mapping revision is provenance rather than
    /// native capability identity.
    pub(crate) async fn load_unsupported_model_batch(
        &self,
        connection: &mut SqliteConnection,
        subjects: &[(String, String, i64, i64, i64)],
    ) -> Result<BTreeSet<(String, String, i64, i64, i64)>, PersistenceError> {
        if subjects.len() > MAX_BATCH_SUBJECTS {
            return Err(PersistenceError::ConstraintViolation);
        }
        if subjects.is_empty() {
            return Ok(BTreeSet::new());
        }
        let json = serde_json::to_string(subjects)
            .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
        let rows = sqlx::query(
            "WITH requested(station_key_id,resolved_model,credential_revision,endpoint_revision,model_alias_revision) AS (SELECT json_extract(value,'$[0]'),json_extract(value,'$[1]'),json_extract(value,'$[2]'),json_extract(value,'$[3]'),json_extract(value,'$[4]') FROM json_each(?1)) SELECT v.station_key_id,v.resolved_model,v.credential_revision,v.endpoint_revision,v.model_alias_revision FROM requested r JOIN routing_capability_model_verdicts v ON v.station_key_id=r.station_key_id AND v.resolved_model=r.resolved_model AND v.credential_revision=r.credential_revision AND v.endpoint_revision=r.endpoint_revision AND v.endpoint_kind='unknown' AND v.protocol_kind='unknown' AND v.identity_version >= 2 WHERE v.verdict='unsupported'",
        )
        .bind(json)
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.get("station_key_id"),
                    row.get("resolved_model"),
                    row.get("credential_revision"),
                    row.get("endpoint_revision"),
                    row.get("model_alias_revision"),
                )
            })
            .collect())
    }

    pub(crate) async fn apply_unsupported_model(
        &self,
        connection: &mut SqliteConnection,
        observation: &UnsupportedModelObservation,
        now_ms: i64,
    ) -> Result<CapabilityObservationApplyResult, PersistenceError> {
        for (value, max) in [
            (observation.observation_id.as_str(), 160),
            (observation.logical_request_id.as_str(), 160),
            (observation.station_key_id.as_str(), 160),
            (observation.resolved_model.as_str(), 256),
            (observation.endpoint_kind.as_str(), 64),
            (observation.protocol_kind.as_str(), 64),
            (observation.evidence_code.as_str(), 96),
            (observation.classifier_profile_version.as_str(), 96),
        ] {
            validate_text(value, max)?;
        }
        if let Some(fence) = observation.model_resolution_fence.as_deref() {
            validate_text(fence, 128)?;
        }
        if now_ms < 0
            || observation.credential_revision <= 0
            || observation.endpoint_revision <= 0
            || observation.model_alias_revision <= 0
            || observation
                .model_mapping_revision
                .is_some_and(|revision| revision <= 0)
        {
            return Err(PersistenceError::ConstraintViolation);
        }

        let payload_hash = hex_digest(
            serde_json::to_vec(&(
                &observation.logical_request_id,
                observation.attempt_ordinal,
                &observation.station_key_id,
                &observation.resolved_model,
                observation.credential_revision,
                observation.endpoint_revision,
                observation.model_alias_revision,
                &observation.endpoint_kind,
                &observation.protocol_kind,
                observation.model_mapping_revision,
                &observation.model_resolution_fence,
                &observation.evidence_code,
                &observation.classifier_profile_version,
            ))
            .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?,
        );
        if let Some(existing) = sqlx::query("SELECT payload_hash FROM routing_capability_model_observations WHERE observation_id = ?1 OR (logical_request_id = ?2 AND attempt_ordinal = ?3 AND station_key_id = ?4 AND resolved_model = ?5) LIMIT 1")
            .bind(&observation.observation_id)
            .bind(&observation.logical_request_id)
            .bind(i64::from(observation.attempt_ordinal))
            .bind(&observation.station_key_id)
            .bind(&observation.resolved_model)
            .fetch_optional(&mut *connection)
            .await?
        {
            return if existing.get::<String, _>("payload_hash") == payload_hash {
                Ok(CapabilityObservationApplyResult::Existing)
            } else {
                Err(PersistenceError::InvariantViolation(
                    "capability observation identity collision".into(),
                ))
            };
        }

        sqlx::query("INSERT INTO routing_capability_model_observations (observation_id,payload_hash,logical_request_id,attempt_ordinal,station_key_id,resolved_model,credential_revision,endpoint_revision,model_alias_revision,endpoint_kind,protocol_kind,identity_version,model_mapping_revision,model_resolution_fence,verdict,evidence_code,classifier_profile_version,created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,'unsupported',?15,?16,?17)")
            .bind(&observation.observation_id)
            .bind(&payload_hash)
            .bind(&observation.logical_request_id)
            .bind(i64::from(observation.attempt_ordinal))
            .bind(&observation.station_key_id)
            .bind(&observation.resolved_model)
            .bind(observation.credential_revision)
            .bind(observation.endpoint_revision)
            .bind(observation.model_alias_revision)
            .bind(&observation.endpoint_kind)
            .bind(&observation.protocol_kind)
            .bind(NATIVE_MODEL_CAPABILITY_IDENTITY_VERSION)
            .bind(observation.model_mapping_revision)
            .bind(&observation.model_resolution_fence)
            .bind(&observation.evidence_code)
            .bind(&observation.classifier_profile_version)
            .bind(now_ms)
            .execute(&mut *connection)
            .await?;
        let sequence: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
            .fetch_one(&mut *connection)
            .await?;
        sqlx::query("INSERT INTO routing_capability_model_verdicts (station_key_id,resolved_model,credential_revision,endpoint_revision,model_alias_revision,endpoint_kind,protocol_kind,identity_version,model_mapping_revision,model_resolution_fence,verdict,source_observation_id,source_ingestion_sequence,projector_version,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'unsupported',?11,?12,?13,?14) ON CONFLICT(station_key_id,resolved_model,endpoint_kind,protocol_kind,credential_revision,endpoint_revision) WHERE identity_version >= 2 DO UPDATE SET model_alias_revision=excluded.model_alias_revision,model_mapping_revision=excluded.model_mapping_revision,model_resolution_fence=excluded.model_resolution_fence,source_observation_id=excluded.source_observation_id,source_ingestion_sequence=excluded.source_ingestion_sequence,projector_version=excluded.projector_version,updated_at_ms=excluded.updated_at_ms WHERE excluded.source_ingestion_sequence > routing_capability_model_verdicts.source_ingestion_sequence")
            .bind(&observation.station_key_id)
            .bind(&observation.resolved_model)
            .bind(observation.credential_revision)
            .bind(observation.endpoint_revision)
            .bind(observation.model_alias_revision)
            .bind(&observation.endpoint_kind)
            .bind(&observation.protocol_kind)
            .bind(NATIVE_MODEL_CAPABILITY_IDENTITY_VERSION)
            .bind(observation.model_mapping_revision)
            .bind(&observation.model_resolution_fence)
            .bind(&observation.observation_id)
            .bind(sequence)
            .bind("capability_evidence_v2")
            .bind(now_ms)
            .execute(&mut *connection)
            .await?;
        Ok(CapabilityObservationApplyResult::Applied)
    }
}

fn validate_text(value: &str, max: usize) -> Result<(), PersistenceError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn hex_digest(value: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_ref());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use sqlx::{Connection, Executor, SqliteConnection};

    use super::*;
    use crate::persistence::migrations::migrator;

    async fn connection() -> SqliteConnection {
        let mut connection = SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open memory database");
        migrator()
            .run(&mut connection)
            .await
            .expect("migrate current schema");
        connection
    }

    fn unsupported_model() -> UnsupportedModelObservation {
        UnsupportedModelObservation {
            observation_id: "model-unsupported".to_string(),
            logical_request_id: "request-model".to_string(),
            attempt_ordinal: 1,
            station_key_id: "key".to_string(),
            resolved_model: "gpt-test".to_string(),
            credential_revision: 1,
            endpoint_revision: 1,
            model_alias_revision: 1,
            endpoint_kind: "unknown".to_string(),
            protocol_kind: "unknown".to_string(),
            model_mapping_revision: Some(1),
            model_resolution_fence: Some("mapping-fence-v1".to_string()),
            evidence_code: "model_unavailable".to_string(),
            classifier_profile_version: "rules-v1".to_string(),
        }
    }

    #[tokio::test]
    async fn unsupported_model_is_idempotent_and_never_writes_legacy_health() {
        let mut connection = connection().await;
        let observation = unsupported_model();
        assert_eq!(
            RoutingHealthVerdictStore
                .apply_unsupported_model(&mut connection, &observation, 100)
                .await
                .unwrap(),
            CapabilityObservationApplyResult::Applied
        );
        assert_eq!(
            RoutingHealthVerdictStore
                .apply_unsupported_model(&mut connection, &observation, 101)
                .await
                .unwrap(),
            CapabilityObservationApplyResult::Existing
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM routing_capability_model_verdicts")
                .fetch_one(&mut connection)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM routing_health_verdicts WHERE scope_kind = 'model_on_key'"
            )
            .fetch_one(&mut connection)
            .await
            .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn lifecycle_revisions_invalidate_but_mapping_provenance_does_not() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        store
            .apply_unsupported_model(&mut connection, &unsupported_model(), 100)
            .await
            .unwrap();

        let exact = [("key".to_string(), "gpt-test".to_string(), 1, 1, 99)];
        assert_eq!(
            store
                .load_unsupported_model_batch(&mut connection, &exact)
                .await
                .unwrap()
                .len(),
            1
        );
        for (credential_revision, endpoint_revision) in [(2, 1), (1, 2)] {
            let changed = [(
                "key".to_string(),
                "gpt-test".to_string(),
                credential_revision,
                endpoint_revision,
                1,
            )];
            assert!(store
                .load_unsupported_model_batch(&mut connection, &changed)
                .await
                .unwrap()
                .is_empty());
        }

        let mut newer_mapping = unsupported_model();
        newer_mapping.observation_id = "model-unsupported-newer".to_string();
        newer_mapping.logical_request_id = "request-model-newer".to_string();
        newer_mapping.model_alias_revision = 99;
        newer_mapping.model_mapping_revision = Some(99);
        newer_mapping.model_resolution_fence = Some("mapping-fence-v99".to_string());
        store
            .apply_unsupported_model(&mut connection, &newer_mapping, 102)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM routing_capability_model_verdicts WHERE station_key_id = 'key' AND resolved_model = 'gpt-test'")
                .fetch_one(&mut connection)
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn learned_capability_survives_database_reopen() {
        use sqlx::sqlite::SqliteConnectOptions;

        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("capability.sqlite3");
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let mut connection = SqliteConnection::connect_with(&options)
            .await
            .expect("open database");
        migrator().run(&mut connection).await.expect("migrate");
        RoutingHealthVerdictStore
            .apply_unsupported_model(&mut connection, &unsupported_model(), 100)
            .await
            .expect("learn capability");
        connection.close().await.expect("close database");

        let mut reopened = SqliteConnection::connect_with(&options)
            .await
            .expect("reopen database");
        let exact = [("key".to_string(), "gpt-test".to_string(), 1, 1, 1)];
        assert_eq!(
            RoutingHealthVerdictStore
                .load_unsupported_model_batch(&mut reopened, &exact)
                .await
                .expect("reload capability")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn batch_input_is_bounded() {
        let mut connection = connection().await;
        let subjects = (0..=MAX_BATCH_SUBJECTS)
            .map(|index| (format!("key-{index}"), "model".to_string(), 1, 1, 1))
            .collect::<Vec<_>>();
        assert!(matches!(
            RoutingHealthVerdictStore
                .load_unsupported_model_batch(&mut connection, &subjects)
                .await,
            Err(PersistenceError::ConstraintViolation)
        ));
    }
}
