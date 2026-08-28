use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};

use crate::{
    application::error_rate_protection::{
        transition_code, ErrorRateHistoryEventV1, ErrorRateProtectionConfigV1,
    },
    application::health_protection::{
        durable_health_scope_commitment, DurableHealthScopeCommitmentInput,
        HealthProtectionObservation, HealthProtectionObservationOutcome,
        HealthProtectionPersistenceKind, HealthProtectionProbe, HealthProtectionProfileV1,
        HealthProtectionReducer, HealthProtectionScope, HealthProtectionScopeKind,
        HealthProtectionSnapshotV1,
    },
    persistence::error::PersistenceError,
    persistence::stores::routing_error_rate_history_store::RoutingErrorRateHistoryStore,
};

pub(crate) const SCOPED_HEALTH_PROJECTOR_VERSION: &str = "scoped-health-projector-v1";
const MAX_BATCH_SUBJECTS: usize = 4_096;
const HEALTH_PROTECTION_STATE_MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthScopeKind {
    StationKeyCredential,
    StationAccount,
    StationGroup,
    StationEndpoint,
    ModelOnKey,
}

impl HealthScopeKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::StationKeyCredential => "station_key_credential",
            Self::StationAccount => "station_account",
            Self::StationGroup => "station_group",
            Self::StationEndpoint => "station_endpoint",
            Self::ModelOnKey => "model_on_key",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DurableHealthVerdict {
    Degraded,
    Cooldown,
    Blocked,
}

impl DurableHealthVerdict {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Degraded => "degraded",
            Self::Cooldown => "cooldown",
            Self::Blocked => "blocked",
        }
    }

    fn parse(value: &str) -> Result<Self, PersistenceError> {
        match value {
            "degraded" => Ok(Self::Degraded),
            "cooldown" => Ok(Self::Cooldown),
            "blocked" => Ok(Self::Blocked),
            _ => Err(PersistenceError::InvariantViolation(
                "unknown scoped routing health verdict".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FailureDimension {
    Credential,
    AccountLifecycle,
    GroupSubscription,
    Balance,
    Quota,
    RateLimit,
    EndpointAvailability,
}

impl FailureDimension {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Credential => "credential",
            Self::AccountLifecycle => "account_lifecycle",
            Self::GroupSubscription => "group_subscription",
            Self::Balance => "balance",
            Self::Quota => "quota",
            Self::RateLimit => "rate_limit",
            Self::EndpointAvailability => "endpoint_availability",
        }
    }
}

/// Closed typed scope. `scope` is a commitment produced here solely as a
/// stable key; readers match the typed columns and never reverse-parse it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ScopedHealthSubject {
    scope: String,
    scope_kind: HealthScopeKind,
    station_id: String,
    station_key_id: Option<String>,
    group_binding_id: Option<String>,
    resolved_model_commitment: Option<String>,
    credential_revision: Option<i64>,
    account_revision: Option<i64>,
    group_revision: Option<i64>,
    endpoint_revision: Option<i64>,
    model_alias_revision: Option<i64>,
}

impl ScopedHealthSubject {
    pub(crate) fn credential(
        station_id: impl Into<String>,
        station_key_id: impl Into<String>,
        credential_revision: i64,
    ) -> Result<Self, PersistenceError> {
        Self::new(
            HealthScopeKind::StationKeyCredential,
            station_id.into(),
            Some(station_key_id.into()),
            None,
            None,
            Some(credential_revision),
            None,
            None,
            None,
            None,
        )
    }

    pub(crate) fn account(
        station_id: impl Into<String>,
        account_revision: i64,
    ) -> Result<Self, PersistenceError> {
        Self::new(
            HealthScopeKind::StationAccount,
            station_id.into(),
            None,
            None,
            None,
            None,
            Some(account_revision),
            None,
            None,
            None,
        )
    }

    pub(crate) fn group(
        station_id: impl Into<String>,
        group_binding_id: impl Into<String>,
        group_revision: i64,
    ) -> Result<Self, PersistenceError> {
        Self::new(
            HealthScopeKind::StationGroup,
            station_id.into(),
            None,
            Some(group_binding_id.into()),
            None,
            None,
            None,
            Some(group_revision),
            None,
            None,
        )
    }

    pub(crate) fn endpoint(
        station_id: impl Into<String>,
        endpoint_revision: i64,
    ) -> Result<Self, PersistenceError> {
        Self::new(
            HealthScopeKind::StationEndpoint,
            station_id.into(),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(endpoint_revision),
            None,
        )
    }

    pub(crate) fn model_on_key(
        station_id: impl Into<String>,
        station_key_id: impl Into<String>,
        resolved_upstream_model: &str,
        deployment_identity: &str,
        credential_revision: i64,
        endpoint_revision: i64,
        model_alias_revision: i64,
    ) -> Result<Self, PersistenceError> {
        validate_text(resolved_upstream_model, 256)?;
        validate_text(deployment_identity, 256)?;
        let commitment = hex_digest(
            [
                b"model-deployment:v1\0".as_slice(),
                resolved_upstream_model.as_bytes(),
                b"\0",
                deployment_identity.as_bytes(),
            ]
            .concat(),
        );
        Self::new(
            HealthScopeKind::ModelOnKey,
            station_id.into(),
            Some(station_key_id.into()),
            None,
            Some(commitment),
            Some(credential_revision),
            None,
            None,
            Some(endpoint_revision),
            Some(model_alias_revision),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        scope_kind: HealthScopeKind,
        station_id: String,
        station_key_id: Option<String>,
        group_binding_id: Option<String>,
        resolved_model_commitment: Option<String>,
        credential_revision: Option<i64>,
        account_revision: Option<i64>,
        group_revision: Option<i64>,
        endpoint_revision: Option<i64>,
        model_alias_revision: Option<i64>,
    ) -> Result<Self, PersistenceError> {
        validate_text(&station_id, 160)?;
        for text in [station_key_id.as_deref(), group_binding_id.as_deref()]
            .into_iter()
            .flatten()
        {
            validate_text(text, 160)?;
        }
        if [
            credential_revision,
            account_revision,
            group_revision,
            endpoint_revision,
            model_alias_revision,
        ]
        .into_iter()
        .flatten()
        .any(|revision| revision <= 0)
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        let shape_valid = match scope_kind {
            HealthScopeKind::StationKeyCredential => {
                station_key_id.is_some()
                    && credential_revision.is_some()
                    && account_revision.is_none()
                    && group_binding_id.is_none()
                    && endpoint_revision.is_none()
                    && resolved_model_commitment.is_none()
            }
            HealthScopeKind::StationAccount => {
                station_key_id.is_none()
                    && account_revision.is_some()
                    && group_binding_id.is_none()
                    && endpoint_revision.is_none()
            }
            HealthScopeKind::StationGroup => {
                station_key_id.is_none()
                    && group_binding_id.is_some()
                    && group_revision.is_some()
                    && account_revision.is_none()
            }
            HealthScopeKind::StationEndpoint => {
                station_key_id.is_none()
                    && endpoint_revision.is_some()
                    && credential_revision.is_none()
            }
            HealthScopeKind::ModelOnKey => {
                station_key_id.is_some()
                    && resolved_model_commitment.is_some()
                    && credential_revision.is_some()
                    && endpoint_revision.is_some()
                    && model_alias_revision.is_some()
            }
        };
        if !shape_valid {
            return Err(PersistenceError::ConstraintViolation);
        }
        let scope = durable_health_scope_commitment(DurableHealthScopeCommitmentInput {
            scope_kind: scope_kind.as_str(),
            station_id: &station_id,
            station_key_id: station_key_id.as_deref(),
            group_binding_id: group_binding_id.as_deref(),
            resolved_model_commitment: resolved_model_commitment.as_deref(),
            credential_revision,
            account_revision,
            group_revision,
            endpoint_revision,
            model_alias_revision,
        })
        .ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "failed to encode scoped health subject".to_string(),
            )
        })?;
        Ok(Self {
            scope,
            scope_kind,
            station_id,
            station_key_id,
            group_binding_id,
            resolved_model_commitment,
            credential_revision,
            account_revision,
            group_revision,
            endpoint_revision,
            model_alias_revision,
        })
    }

    pub(crate) fn scope(&self) -> &str {
        &self.scope
    }

    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "the typed scope shape contract is asserted by this module's focused tests"
    )]
    pub(crate) const fn scope_kind(&self) -> HealthScopeKind {
        self.scope_kind
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ScopedHealthObservation {
    pub(crate) observation_id: String,
    pub(crate) producer_id: String,
    pub(crate) producer_sequence: u64,
    pub(crate) logical_request_id: String,
    pub(crate) attempt_ordinal: u8,
    pub(crate) terminal_kind: String,
    pub(crate) subject: ScopedHealthSubject,
    pub(crate) dimension: FailureDimension,
    /// None is explicit same-scope recovery. Absence of a row is the only
    /// durable representation of healthy/admit.
    pub(crate) verdict: Option<DurableHealthVerdict>,
    pub(crate) cooldown_until_ms: Option<i64>,
    pub(crate) evidence_code: String,
    pub(crate) classifier_profile_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScopedHealthVerdictRow {
    pub(crate) subject_scope: String,
    pub(crate) scope_kind: HealthScopeKind,
    pub(crate) dimension: FailureDimension,
    pub(crate) verdict: DurableHealthVerdict,
    pub(crate) cooldown_until_ms: Option<i64>,
    pub(crate) evidence_code: String,
    pub(crate) updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopedObservationApplyResult {
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
    /// Historical alias revision retained for provenance. It is deliberately
    /// not part of the native capability identity written by this path.
    pub(crate) model_alias_revision: i64,
    /// Endpoint/protocol labels are carried independently from model mapping
    /// provenance. Current request lifecycle records do not yet expose these
    /// labels, so the production bridge uses the explicit `unknown` value and
    /// fails closed for future non-unknown identities until the planner can
    /// provide matching labels.
    pub(crate) endpoint_kind: String,
    pub(crate) protocol_kind: String,
    pub(crate) model_mapping_revision: Option<i64>,
    pub(crate) model_resolution_fence: Option<String>,
    pub(crate) evidence_code: String,
    pub(crate) classifier_profile_version: String,
}

const NATIVE_MODEL_CAPABILITY_IDENTITY_VERSION: i64 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RebuildProof {
    pub(crate) generation_id: String,
    pub(crate) row_count: u64,
    pub(crate) content_hash: String,
    pub(crate) watermark: Option<(i64, i64, String)>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingHealthVerdictStore;

impl RoutingHealthVerdictStore {
    async fn health_protection_profile(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<HealthProtectionProfileV1, PersistenceError> {
        let config_json: Option<String> =
            sqlx::query_scalar("SELECT config_json FROM routing_policy WHERE singleton_key = 1")
                .fetch_optional(&mut *connection)
                .await?;
        let Some(config_json) = config_json else {
            return HealthProtectionProfileV1::from_policy_config(
                &crate::models::routing_policy::ProtectionProfileConfigV2::default(),
            )
            .map_err(|_| {
                PersistenceError::InvariantViolation(
                    "invalid default health protection policy profile".into(),
                )
            });
        };
        let value: serde_json::Value = serde_json::from_str(&config_json).map_err(|_| {
            PersistenceError::InvariantViolation("routing policy JSON is invalid".into())
        })?;
        let policy =
            crate::models::routing_policy::RoutingPolicyConfigV2::from_stored_value(&value)
                .map_err(|_| {
                    PersistenceError::InvariantViolation("routing policy is invalid".into())
                })?;
        HealthProtectionProfileV1::from_policy_config(&policy.protection_profile).map_err(|_| {
            PersistenceError::InvariantViolation("invalid health protection policy profile".into())
        })
    }

    /// Ensures the active projection was built by this store's current
    /// projector. A mismatch is repaired through a shadow generation, so
    /// planners never observe an empty table during a rebuild.
    pub(crate) async fn ensure_current_projection(
        &self,
        connection: &mut SqliteConnection,
        now_ms: i64,
    ) -> Result<bool, PersistenceError> {
        let active_state: (String, Option<i64>, Option<i64>, Option<String>) = sqlx::query_as(
            "SELECT projector_version, watermark_ingested_at_ms, watermark_ingestion_sequence, watermark_observation_id FROM routing_health_projector_state WHERE singleton_key = 1",
        )
        .fetch_one(&mut *connection)
        .await?;
        let observed_watermark: Option<(i64, i64, String)> = sqlx::query_as(
            "SELECT ingested_at_ms, ingestion_sequence, observation_id FROM routing_health_observations ORDER BY ingestion_sequence DESC LIMIT 1",
        )
        .fetch_optional(&mut *connection)
        .await?;
        let active_watermark = match (active_state.1, active_state.2, active_state.3) {
            (Some(ingested_at_ms), Some(ingestion_sequence), Some(observation_id)) => {
                Some((ingested_at_ms, ingestion_sequence, observation_id))
            }
            (None, None, None) => None,
            _ => {
                return Err(PersistenceError::InvariantViolation(
                    "scoped health projector checkpoint is incomplete".into(),
                ));
            }
        };
        // A portable restore preserves immutable observations but resets runtime
        // projections. Version equality alone must not make that empty state
        // appear current after restart.
        if active_state.0 == SCOPED_HEALTH_PROJECTOR_VERSION
            && active_watermark == observed_watermark
        {
            return Ok(false);
        }

        let latest_sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(ingestion_sequence), 0) FROM routing_health_observations",
        )
        .fetch_one(&mut *connection)
        .await?;
        let generation_id = format!(
            "scoped-health-rebuild-v1-{}-{}",
            now_ms.max(0),
            latest_sequence.max(0)
        );
        let proof = self
            .rebuild_shadow(connection, &generation_id, now_ms.max(0))
            .await?;
        self.activate_generation(connection, &proof, now_ms.max(0))
            .await?;
        Ok(true)
    }

    /// Loads unsupported-model capability verdicts for the native model and
    /// execution revision tuples supplied by the planner. The final tuple
    /// slot is retained as a source-compatibility field for callers that still
    /// carry `model_alias_revision`; it is not used for native identity.
    /// One JSON bind keeps statement count and SQLite variable use bounded.
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
    ) -> Result<ScopedObservationApplyResult, PersistenceError> {
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
            .bind(&observation.observation_id).bind(&observation.logical_request_id)
            .bind(i64::from(observation.attempt_ordinal)).bind(&observation.station_key_id)
            .bind(&observation.resolved_model).fetch_optional(&mut *connection).await? {
            return if existing.get::<String, _>("payload_hash") == payload_hash {
                Ok(ScopedObservationApplyResult::Existing)
            } else { Err(PersistenceError::InvariantViolation("capability observation identity collision".into())) };
        }
        sqlx::query("INSERT INTO routing_capability_model_observations (observation_id,payload_hash,logical_request_id,attempt_ordinal,station_key_id,resolved_model,credential_revision,endpoint_revision,model_alias_revision,endpoint_kind,protocol_kind,identity_version,model_mapping_revision,model_resolution_fence,verdict,evidence_code,classifier_profile_version,created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,'unsupported',?15,?16,?17)")
            .bind(&observation.observation_id).bind(&payload_hash).bind(&observation.logical_request_id)
            .bind(i64::from(observation.attempt_ordinal)).bind(&observation.station_key_id)
            .bind(&observation.resolved_model).bind(observation.credential_revision)
            .bind(observation.endpoint_revision).bind(observation.model_alias_revision)
            .bind(&observation.endpoint_kind).bind(&observation.protocol_kind)
            .bind(NATIVE_MODEL_CAPABILITY_IDENTITY_VERSION)
            .bind(observation.model_mapping_revision)
            .bind(&observation.model_resolution_fence)
            .bind(&observation.evidence_code).bind(&observation.classifier_profile_version)
            .bind(now_ms).execute(&mut *connection).await?;
        let sequence: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
            .fetch_one(&mut *connection)
            .await?;
        sqlx::query("INSERT INTO routing_capability_model_verdicts (station_key_id,resolved_model,credential_revision,endpoint_revision,model_alias_revision,endpoint_kind,protocol_kind,identity_version,model_mapping_revision,model_resolution_fence,verdict,source_observation_id,source_ingestion_sequence,projector_version,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'unsupported',?11,?12,?13,?14) ON CONFLICT(station_key_id,resolved_model,endpoint_kind,protocol_kind,credential_revision,endpoint_revision) WHERE identity_version >= 2 DO UPDATE SET model_alias_revision=excluded.model_alias_revision,model_mapping_revision=excluded.model_mapping_revision,model_resolution_fence=excluded.model_resolution_fence,source_observation_id=excluded.source_observation_id,source_ingestion_sequence=excluded.source_ingestion_sequence,projector_version=excluded.projector_version,updated_at_ms=excluded.updated_at_ms WHERE excluded.source_ingestion_sequence > routing_capability_model_verdicts.source_ingestion_sequence")
            .bind(&observation.station_key_id).bind(&observation.resolved_model)
            .bind(observation.credential_revision).bind(observation.endpoint_revision)
            .bind(observation.model_alias_revision).bind(&observation.endpoint_kind)
            .bind(&observation.protocol_kind)
            .bind(NATIVE_MODEL_CAPABILITY_IDENTITY_VERSION)
            .bind(observation.model_mapping_revision)
            .bind(&observation.model_resolution_fence)
            .bind(&observation.observation_id)
            .bind(sequence).bind("capability_evidence_v2").bind(now_ms)
            .execute(&mut *connection).await?;
        Ok(ScopedObservationApplyResult::Applied)
    }

    /// Appends immutable evidence, applies/removes its scoped verdict and moves
    /// the cursor in the caller-owned transaction. No other lifecycle owner is
    /// allowed to independently write the projection.
    pub(crate) async fn apply_observation(
        &self,
        connection: &mut SqliteConnection,
        observation: &ScopedHealthObservation,
        now_ms: i64,
    ) -> Result<ScopedObservationApplyResult, PersistenceError> {
        validate_observation(observation, now_ms)?;
        let payload_hash = observation_payload_hash(observation)?;
        if let Some(row) = sqlx::query(
            "SELECT producer_id, producer_sequence, payload_hash FROM routing_health_observations WHERE observation_id = ?1 OR (producer_id = ?2 AND producer_sequence = ?3) OR (logical_request_id = ?4 AND attempt_ordinal = ?5 AND terminal_kind = ?6 AND scope = ?7 AND failure_dimension = ?8) LIMIT 1",
        )
        .bind(&observation.observation_id)
        .bind(&observation.producer_id)
        .bind(i64::try_from(observation.producer_sequence).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(&observation.logical_request_id)
        .bind(i64::from(observation.attempt_ordinal))
        .bind(&observation.terminal_kind)
        .bind(observation.subject.scope())
        .bind(observation.dimension.as_str())
        .fetch_optional(&mut *connection)
        .await?
        {
            if row.get::<String, _>("producer_id") == observation.producer_id
                && row.get::<i64, _>("producer_sequence") == observation.producer_sequence as i64
                && row.get::<String, _>("payload_hash") == payload_hash
            {
                return Ok(ScopedObservationApplyResult::Existing);
            }
            return Err(PersistenceError::InvariantViolation(
                "scoped health observation identity collision".into(),
            ));
        }
        let ingested_at_ms = now_ms;
        bind_observation_insert(
            sqlx::query(
                "INSERT INTO routing_health_observations (observation_id, producer_id, producer_sequence, payload_hash, logical_request_id, attempt_ordinal, terminal_kind, ingested_at_ms, scope, scope_kind, failure_dimension, station_id, station_key_id, group_binding_id, resolved_model_commitment, credential_revision, account_revision, group_revision, endpoint_revision, model_alias_revision, verdict, cooldown_until_ms, evidence_code, projector_profile_version, created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)",
            ),
            observation,
            &payload_hash,
            ingested_at_ms,
            now_ms,
        )
        .execute(&mut *connection)
        .await?;
        let ingestion_sequence: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
            .fetch_one(&mut *connection)
            .await?;
        let active_generation: String = sqlx::query_scalar(
            "SELECT active_generation_id FROM routing_health_projector_state WHERE singleton_key = 1",
        )
        .fetch_one(&mut *connection)
        .await?;
        if observation.verdict.is_some() {
            upsert_verdict(
                connection,
                &active_generation,
                observation,
                ingestion_sequence,
                ingested_at_ms,
                now_ms,
            )
            .await?;
        } else {
            sqlx::query(
                "DELETE FROM routing_health_verdicts WHERE generation_id = ?1 AND scope = ?2 AND failure_dimension = ?3 AND source_ingestion_sequence < ?4",
            )
            .bind(&active_generation)
            .bind(observation.subject.scope())
            .bind(observation.dimension.as_str())
            .bind(ingestion_sequence)
            .execute(&mut *connection)
            .await?;
        }
        sqlx::query(
            "UPDATE routing_health_projector_state SET watermark_ingested_at_ms = ?1, watermark_ingestion_sequence = ?2, watermark_observation_id = ?3, updated_at_ms = ?4 WHERE singleton_key = 1",
        )
        .bind(ingested_at_ms)
        .bind(ingestion_sequence)
        .bind(&observation.observation_id)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        // The reducer metadata is written in this same caller-owned
        // transaction. The immutable observation and scoped verdict remain
        // the only durable health evidence/projection owners.
        self.apply_health_protection_state(connection, observation, ingested_at_ms)
            .await?;
        Ok(ScopedObservationApplyResult::Applied)
    }

    /// Applies a low-cardinality error-rate observation to the single durable
    /// reducer and writes its diagnostic event in the caller-owned transaction.
    /// The existing scoped verdict tables remain the owner for explicit
    /// credential/account/endpoint verdicts; this path only owns the reducer
    /// window and its bounded history.
    pub(crate) async fn apply_error_rate_observation(
        &self,
        connection: &mut SqliteConnection,
        observation: HealthProtectionObservation,
        mut history_event: ErrorRateHistoryEventV1,
        history_observation_id: &str,
        config: &ErrorRateProtectionConfigV1,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        if !config.enabled {
            return Ok(());
        }
        let mut reducer = match self.load_health_protection_reducer(connection).await? {
            Some(reducer) => reducer,
            None => {
                let mut reducer = HealthProtectionReducer::new(
                    self.health_protection_profile(connection).await?,
                    HealthProtectionPersistenceKind::Durable,
                )
                .map_err(|_| {
                    PersistenceError::InvariantViolation(
                        "invalid default health protection profile".into(),
                    )
                })?;
                self.rebuild_health_protection_state(connection, &mut reducer)
                    .await?;
                reducer
            }
        };
        let transition = match reducer.observe(observation) {
            Ok(transition) => Some(transition_code(transition)),
            // A late event or an observation arriving while a scope is Open
            // is still valuable history, but must not be allowed to mutate or
            // fail the durable protection state.
            Err(
                crate::application::health_protection::HealthProtectionError::ProbeNotAllowed
                | crate::application::health_protection::HealthProtectionError::StaleObservation,
            ) => None,
            Err(error) => {
                return Err(PersistenceError::InvariantViolation(format!(
                    "error-rate reducer rejected observation: {error:?}"
                )))
            }
        };
        history_event.transition = transition;
        if transition.is_some() {
            self.save_health_protection_reducer(connection, &reducer, now_ms)
                .await?;
        }
        RoutingErrorRateHistoryStore
            .append(
                connection,
                history_event,
                history_observation_id,
                config,
                now_ms,
            )
            .await
            .map(|_| ())
    }

    /// Load the durable reducer snapshot for read models and startup checks.
    /// A malformed or future snapshot fails closed instead of silently
    /// discarding protection state.
    pub(crate) async fn load_health_protection_reducer(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<Option<HealthProtectionReducer>, PersistenceError> {
        let row = sqlx::query(
            "SELECT profile_version, profile_json, snapshot_version, snapshot_json, content_hash FROM routing_health_protection_state WHERE singleton_key = 1",
        )
        .fetch_optional(&mut *connection)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let profile_version: String = row.get("profile_version");
        let profile_json: String = row.get("profile_json");
        let snapshot_version: String = row.get("snapshot_version");
        let snapshot_json: String = row.get("snapshot_json");
        let content_hash: String = row.get("content_hash");
        if profile_json.len() > HEALTH_PROTECTION_STATE_MAX_BYTES
            || snapshot_json.len() > HEALTH_PROTECTION_STATE_MAX_BYTES
            || content_hash
                != hex_digest([profile_json.as_bytes(), snapshot_json.as_bytes()].concat())
        {
            return Err(PersistenceError::InvariantViolation(
                "routing health protection snapshot integrity check failed".into(),
            ));
        }
        let profile: HealthProtectionProfileV1 =
            serde_json::from_str(&profile_json).map_err(|_| {
                PersistenceError::InvariantViolation("invalid health protection profile".into())
            })?;
        let snapshot: HealthProtectionSnapshotV1 =
            serde_json::from_str(&snapshot_json).map_err(|_| {
                PersistenceError::InvariantViolation("invalid health protection snapshot".into())
            })?;
        if profile_version != profile.version || snapshot_version != snapshot.version {
            return Err(PersistenceError::InvariantViolation(
                "routing health protection snapshot version mismatch".into(),
            ));
        }
        if snapshot.persistence_kind != HealthProtectionPersistenceKind::Durable {
            return Err(PersistenceError::InvariantViolation(
                "runtime health protection snapshot cannot be restored as durable state".into(),
            ));
        }
        let mut reducer = HealthProtectionReducer::restore(profile, snapshot).map_err(|_| {
            PersistenceError::InvariantViolation("invalid health protection snapshot state".into())
        })?;
        reducer
            .reconfigure(self.health_protection_profile(connection).await?)
            .map_err(|_| {
                PersistenceError::InvariantViolation(
                    "invalid health protection policy profile".into(),
                )
            })?;
        Ok(Some(reducer))
    }

    pub(crate) async fn load_health_protection_statuses(
        &self,
        connection: &mut SqliteConnection,
        now_ms: i64,
    ) -> Result<Vec<crate::application::health_protection::HealthProtectionStatus>, PersistenceError>
    {
        let Some(mut reducer) = self.load_health_protection_reducer(connection).await? else {
            return Ok(Vec::new());
        };
        reducer.statuses(now_ms.max(0)).map_err(|_| {
            PersistenceError::InvariantViolation(
                "health protection status projection failed".into(),
            )
        })
    }

    /// Atomically reserves the single Half-Open probe for a durable scope.
    /// The returned fence is caller-owned and must be attached to the next
    /// real, replay-safe observation; this method never performs network I/O.
    pub(crate) async fn begin_health_protection_probe(
        &self,
        connection: &mut SqliteConnection,
        scope: &HealthProtectionScope,
        now_ms: i64,
    ) -> Result<Option<HealthProtectionProbe>, PersistenceError> {
        let Some(mut reducer) = self.load_health_protection_reducer(connection).await? else {
            return Ok(None);
        };
        let probe = match reducer.begin_probe(scope, now_ms.max(0)) {
            Ok(probe) => probe,
            Err(crate::application::health_protection::HealthProtectionError::ProbeNotAllowed)
            | Err(
                crate::application::health_protection::HealthProtectionError::ProbeAlreadyInFlight,
            )
            | Err(crate::application::health_protection::HealthProtectionError::ScopeNotFound) => {
                return Ok(None)
            }
            Err(error) => {
                return Err(PersistenceError::InvariantViolation(format!(
                    "health protection probe reservation rejected: {error:?}"
                )))
            }
        };
        self.save_health_protection_reducer(connection, &reducer, now_ms.max(0))
            .await?;
        Ok(Some(probe))
    }

    /// Releases a probe that never crossed the outbound-attempt boundary.
    /// The reducer bumps its revision so a late terminal record cannot apply
    /// to a subsequently reserved probe.
    pub(crate) async fn cancel_health_protection_probe(
        &self,
        connection: &mut SqliteConnection,
        probe: &HealthProtectionProbe,
        now_ms: i64,
    ) -> Result<bool, PersistenceError> {
        let Some(mut reducer) = self.load_health_protection_reducer(connection).await? else {
            return Ok(false);
        };
        let cancelled = reducer
            .cancel_probe(probe.clone(), now_ms.max(0))
            .map_err(|error| {
                PersistenceError::InvariantViolation(format!(
                    "health protection probe cancellation rejected: {error:?}"
                ))
            })?;
        if cancelled {
            self.save_health_protection_reducer(connection, &reducer, now_ms.max(0))
                .await?;
        }
        Ok(cancelled)
    }

    /// Applies the result of a previously fenced probe in the same caller
    /// transaction. Stale or duplicate results cannot mutate a newer cycle.
    #[cfg(test)]
    pub(crate) async fn apply_health_protection_probe(
        &self,
        connection: &mut SqliteConnection,
        observation: HealthProtectionObservation,
        now_ms: i64,
    ) -> Result<crate::application::health_protection::HealthProtectionTransition, PersistenceError>
    {
        if !observation.probe {
            return Err(PersistenceError::ConstraintViolation);
        }
        let Some(mut reducer) = self.load_health_protection_reducer(connection).await? else {
            return Err(PersistenceError::InvariantViolation(
                "probe result has no durable health protection state".into(),
            ));
        };
        let transition = reducer.observe(observation).map_err(|error| {
            PersistenceError::InvariantViolation(format!(
                "health protection probe result rejected: {error:?}"
            ))
        })?;
        self.save_health_protection_reducer(connection, &reducer, now_ms.max(0))
            .await?;
        Ok(transition)
    }

    pub(crate) async fn ensure_health_protection_state(
        &self,
        connection: &mut SqliteConnection,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        if self
            .load_health_protection_reducer(connection)
            .await?
            .is_some()
        {
            return Ok(());
        }
        let mut reducer = HealthProtectionReducer::new(
            self.health_protection_profile(connection).await?,
            HealthProtectionPersistenceKind::Durable,
        )
        .map_err(|_| {
            PersistenceError::InvariantViolation("invalid default health protection profile".into())
        })?;
        self.rebuild_health_protection_state(connection, &mut reducer)
            .await?;
        self.save_health_protection_reducer(connection, &reducer, now_ms)
            .await
    }

    async fn apply_health_protection_state(
        &self,
        connection: &mut SqliteConnection,
        observation: &ScopedHealthObservation,
        observed_at_ms: i64,
    ) -> Result<(), PersistenceError> {
        let mut reducer = match self.load_health_protection_reducer(connection).await? {
            Some(reducer) => reducer,
            None => {
                let mut reducer = HealthProtectionReducer::new(
                    self.health_protection_profile(connection).await?,
                    HealthProtectionPersistenceKind::Durable,
                )
                .map_err(|_| {
                    PersistenceError::InvariantViolation(
                        "invalid default health protection profile".into(),
                    )
                })?;
                self.rebuild_health_protection_state(connection, &mut reducer)
                    .await?;
                reducer
            }
        };
        let scope =
            health_protection_scope(observation.subject.scope_kind, observation.subject.scope())?;
        match observation.verdict {
            None => {
                reducer
                    .apply_durable_recovery(
                        scope,
                        observation.observation_id.clone(),
                        observed_at_ms,
                    )
                    .map_err(|_| {
                        PersistenceError::InvariantViolation(
                            "health protection recovery rejected".into(),
                        )
                    })?;
            }
            Some(verdict) => {
                let opens = matches!(
                    verdict,
                    DurableHealthVerdict::Cooldown | DurableHealthVerdict::Blocked
                );
                let outcome = HealthProtectionObservationOutcome::Failure(
                    crate::application::health_protection::failure_code_from_label(
                        &observation.evidence_code,
                    ),
                );
                reducer
                    .apply_durable_verdict(
                        HealthProtectionObservation {
                            id: observation.observation_id.clone(),
                            scope,
                            observed_at_ms,
                            outcome,
                            probe: false,
                            probe_state_revision: None,
                            retry_after_ms: observation
                                .cooldown_until_ms
                                .map(|until| until.saturating_sub(observed_at_ms).max(0)),
                        },
                        opens,
                    )
                    .map_err(|_| {
                        PersistenceError::InvariantViolation(
                            "health protection verdict rejected".into(),
                        )
                    })?;
            }
        }
        self.save_health_protection_reducer(connection, &reducer, observed_at_ms)
            .await
    }

    async fn rebuild_health_protection_state(
        &self,
        connection: &mut SqliteConnection,
        reducer: &mut HealthProtectionReducer,
    ) -> Result<(), PersistenceError> {
        let rows = sqlx::query(
            "SELECT observation_id, scope, scope_kind, ingested_at_ms, verdict, cooldown_until_ms, evidence_code FROM routing_health_observations ORDER BY ingestion_sequence ASC",
        )
        .fetch_all(&mut *connection)
        .await?;
        if rows.len() > 65_536 {
            return Err(PersistenceError::ConstraintViolation);
        }
        for row in rows {
            let scope_kind = parse_scope_kind(&row.get::<String, _>("scope_kind"))?;
            let scope = health_protection_scope(scope_kind, &row.get::<String, _>("scope"))?;
            let observed_at_ms: i64 = row.get("ingested_at_ms");
            let observation_id: String = row.get("observation_id");
            let verdict = row.get::<Option<String>, _>("verdict");
            if let Some(verdict) = verdict {
                let parsed = DurableHealthVerdict::parse(&verdict)?;
                let opens = matches!(
                    parsed,
                    DurableHealthVerdict::Cooldown | DurableHealthVerdict::Blocked
                );
                reducer
                    .apply_durable_verdict(
                        HealthProtectionObservation {
                            id: observation_id,
                            scope,
                            observed_at_ms,
                            outcome: HealthProtectionObservationOutcome::Failure(
                                crate::application::health_protection::failure_code_from_label(
                                    &row.get::<String, _>("evidence_code"),
                                ),
                            ),
                            probe: false,
                            probe_state_revision: None,
                            retry_after_ms: row
                                .get::<Option<i64>, _>("cooldown_until_ms")
                                .map(|until| until.saturating_sub(observed_at_ms).max(0)),
                        },
                        opens,
                    )
                    .map_err(|_| {
                        PersistenceError::InvariantViolation(
                            "health protection rebuild rejected".into(),
                        )
                    })?;
            } else {
                reducer
                    .apply_durable_recovery(scope, observation_id, observed_at_ms)
                    .map_err(|_| {
                        PersistenceError::InvariantViolation(
                            "health protection rebuild recovery rejected".into(),
                        )
                    })?;
            }
        }
        Ok(())
    }

    async fn save_health_protection_reducer(
        &self,
        connection: &mut SqliteConnection,
        reducer: &HealthProtectionReducer,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        if reducer.persistence_kind() != HealthProtectionPersistenceKind::Durable {
            return Err(PersistenceError::InvariantViolation(
                "only durable health protection state may be persisted".into(),
            ));
        }
        let profile_json = serde_json::to_string(reducer.profile()).map_err(|_| {
            PersistenceError::InvariantViolation("health protection profile encode failed".into())
        })?;
        let snapshot = reducer.snapshot(now_ms.max(0));
        let snapshot_json = serde_json::to_string(&snapshot).map_err(|_| {
            PersistenceError::InvariantViolation("health protection snapshot encode failed".into())
        })?;
        if profile_json.len() > HEALTH_PROTECTION_STATE_MAX_BYTES
            || snapshot_json.len() > HEALTH_PROTECTION_STATE_MAX_BYTES
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        let content_hash = hex_digest([profile_json.as_bytes(), snapshot_json.as_bytes()].concat());
        sqlx::query(
            "INSERT INTO routing_health_protection_state (singleton_key, profile_version, profile_json, snapshot_version, snapshot_json, content_hash, generated_at_ms, updated_at_ms) VALUES (1,?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(singleton_key) DO UPDATE SET profile_version=excluded.profile_version, profile_json=excluded.profile_json, snapshot_version=excluded.snapshot_version, snapshot_json=excluded.snapshot_json, content_hash=excluded.content_hash, generated_at_ms=excluded.generated_at_ms, updated_at_ms=excluded.updated_at_ms",
        )
        .bind(&reducer.profile().version)
        .bind(profile_json)
        .bind(&snapshot.version)
        .bind(snapshot_json)
        .bind(content_hash)
        .bind(snapshot.generated_at_ms)
        .bind(now_ms.max(0))
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    /// One SQL statement for any bounded candidate set. JSON input consumes a
    /// single SQLite bind and therefore cannot cross the variable limit.
    pub(crate) async fn load_active_batch(
        &self,
        connection: &mut SqliteConnection,
        subjects: &[ScopedHealthSubject],
    ) -> Result<BTreeMap<(String, FailureDimension), ScopedHealthVerdictRow>, PersistenceError>
    {
        if subjects.len() > MAX_BATCH_SUBJECTS {
            return Err(PersistenceError::ConstraintViolation);
        }
        if subjects.is_empty() {
            return Ok(BTreeMap::new());
        }
        let unique = subjects
            .iter()
            .map(|subject| subject.scope().to_string())
            .collect::<BTreeSet<_>>();
        let json = serde_json::to_string(&unique)
            .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
        let rows = sqlx::query(
            "WITH requested(scope) AS (SELECT value FROM json_each(?1)) SELECT v.scope, v.scope_kind, v.failure_dimension, v.verdict, v.cooldown_until_ms, v.evidence_code, v.updated_at_ms FROM requested r JOIN routing_health_projector_state state ON state.singleton_key = 1 JOIN routing_health_verdicts v ON v.generation_id = state.active_generation_id AND v.scope = r.scope",
        )
        .bind(json)
        .fetch_all(&mut *connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                let scope = row.get::<String, _>("scope");
                let kind = parse_scope_kind(&row.get::<String, _>("scope_kind"))?;
                let dimension =
                    parse_failure_dimension(&row.get::<String, _>("failure_dimension"))?;
                Ok((
                    (scope.clone(), dimension),
                    ScopedHealthVerdictRow {
                        subject_scope: scope,
                        scope_kind: kind,
                        dimension,
                        verdict: DurableHealthVerdict::parse(&row.get::<String, _>("verdict"))?,
                        cooldown_until_ms: row.get("cooldown_until_ms"),
                        evidence_code: row.get("evidence_code"),
                        updated_at_ms: row.get("updated_at_ms"),
                    },
                ))
            })
            .collect()
    }

    /// Load the bounded active projection for the protection read model. The
    /// projector is already bounded by its ingestion policy; this method only
    /// exposes the active generation and never scans historical generations.
    pub(crate) async fn load_active_all(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<Vec<ScopedHealthVerdictRow>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT v.scope, v.scope_kind, v.failure_dimension, v.verdict, v.cooldown_until_ms, v.evidence_code, v.updated_at_ms FROM routing_health_projector_state state JOIN routing_health_verdicts v ON v.generation_id = state.active_generation_id WHERE state.singleton_key = 1 ORDER BY v.scope ASC, v.failure_dimension ASC",
        )
        .fetch_all(&mut *connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(ScopedHealthVerdictRow {
                    subject_scope: row.get::<String, _>("scope"),
                    scope_kind: parse_scope_kind(&row.get::<String, _>("scope_kind"))?,
                    dimension: parse_failure_dimension(&row.get::<String, _>("failure_dimension"))?,
                    verdict: DurableHealthVerdict::parse(&row.get::<String, _>("verdict"))?,
                    cooldown_until_ms: row.get("cooldown_until_ms"),
                    evidence_code: row.get("evidence_code"),
                    updated_at_ms: row.get("updated_at_ms"),
                })
            })
            .collect()
    }

    /// Builds a complete shadow generation from the immutable ingestion log.
    /// It intentionally does not cut over; callers can compare the returned
    /// watermark/count/hash before invoking `activate_generation`.
    pub(crate) async fn rebuild_shadow(
        &self,
        connection: &mut SqliteConnection,
        generation_id: &str,
        now_ms: i64,
    ) -> Result<RebuildProof, PersistenceError> {
        validate_text(generation_id, 96)?;
        sqlx::query("INSERT INTO routing_health_generations (generation_id, projector_version, status, created_at_ms) VALUES (?1, ?2, 'shadow', ?3)")
            .bind(generation_id)
            .bind(SCOPED_HEALTH_PROJECTOR_VERSION)
            .bind(now_ms)
            .execute(&mut *connection)
            .await?;
        sqlx::query(
            "INSERT INTO routing_health_verdicts (generation_id, scope, scope_kind, failure_dimension, station_id, station_key_id, group_binding_id, resolved_model_commitment, credential_revision, account_revision, group_revision, endpoint_revision, model_alias_revision, verdict, cooldown_until_ms, evidence_code, source_observation_id, source_ingested_at_ms, source_ingestion_sequence, projector_version, updated_at_ms) SELECT ?1, scope, scope_kind, failure_dimension, station_id, station_key_id, group_binding_id, resolved_model_commitment, credential_revision, account_revision, group_revision, endpoint_revision, model_alias_revision, verdict, cooldown_until_ms, evidence_code, observation_id, ingested_at_ms, ingestion_sequence, ?2, ?3 FROM (SELECT o.*, ROW_NUMBER() OVER (PARTITION BY scope, failure_dimension ORDER BY ingestion_sequence DESC) AS rank FROM routing_health_observations o) latest WHERE rank = 1 AND verdict IS NOT NULL",
        )
        .bind(generation_id)
        .bind(SCOPED_HEALTH_PROJECTOR_VERSION)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        let proof = generation_proof(connection, generation_id).await?;
        sqlx::query("UPDATE routing_health_generations SET watermark_ingested_at_ms = ?1, watermark_ingestion_sequence = ?2, watermark_observation_id = ?3, projected_row_count = ?4, projected_content_hash = ?5 WHERE generation_id = ?6 AND status = 'shadow'")
            .bind(proof.watermark.as_ref().map(|cursor| cursor.0))
            .bind(proof.watermark.as_ref().map(|cursor| cursor.1))
            .bind(proof.watermark.as_ref().map(|cursor| cursor.2.as_str()))
            .bind(i64::try_from(proof.row_count).map_err(|_| PersistenceError::ConstraintViolation)?)
            .bind(&proof.content_hash)
            .bind(generation_id)
            .execute(&mut *connection)
            .await?;
        Ok(proof)
    }

    pub(crate) async fn activate_generation(
        &self,
        connection: &mut SqliteConnection,
        proof: &RebuildProof,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        let stored = generation_proof(connection, &proof.generation_id).await?;
        if &stored != proof {
            return Err(PersistenceError::InvariantViolation(
                "scoped health shadow generation parity changed".into(),
            ));
        }
        let status: String = sqlx::query_scalar(
            "SELECT status FROM routing_health_generations WHERE generation_id = ?1",
        )
        .bind(&proof.generation_id)
        .fetch_one(&mut *connection)
        .await?;
        if status != "shadow" {
            return Err(PersistenceError::InvariantViolation(
                "only a verified shadow generation can be activated".into(),
            ));
        }
        sqlx::query(
            "UPDATE routing_health_generations SET status = 'retired' WHERE status = 'active'",
        )
        .execute(&mut *connection)
        .await?;
        sqlx::query("UPDATE routing_health_generations SET status = 'active', activated_at_ms = ?1 WHERE generation_id = ?2 AND status = 'shadow'")
            .bind(now_ms)
            .bind(&proof.generation_id)
            .execute(&mut *connection)
            .await?;
        sqlx::query("UPDATE routing_health_projector_state SET active_generation_id = ?1, projector_version = ?2, watermark_ingested_at_ms = ?3, watermark_ingestion_sequence = ?4, watermark_observation_id = ?5, updated_at_ms = ?6 WHERE singleton_key = 1")
            .bind(&proof.generation_id)
            .bind(SCOPED_HEALTH_PROJECTOR_VERSION)
            .bind(proof.watermark.as_ref().map(|cursor| cursor.0))
            .bind(proof.watermark.as_ref().map(|cursor| cursor.1))
            .bind(proof.watermark.as_ref().map(|cursor| cursor.2.as_str()))
            .bind(now_ms)
            .execute(&mut *connection)
            .await?;
        Ok(())
    }
}

fn bind_observation_insert<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    observation: &'q ScopedHealthObservation,
    payload_hash: &'q str,
    ingested_at_ms: i64,
    now_ms: i64,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    let subject = &observation.subject;
    query
        .bind(&observation.observation_id)
        .bind(&observation.producer_id)
        .bind(observation.producer_sequence as i64)
        .bind(payload_hash)
        .bind(&observation.logical_request_id)
        .bind(i64::from(observation.attempt_ordinal))
        .bind(&observation.terminal_kind)
        .bind(ingested_at_ms)
        .bind(&subject.scope)
        .bind(subject.scope_kind.as_str())
        .bind(observation.dimension.as_str())
        .bind(&subject.station_id)
        .bind(&subject.station_key_id)
        .bind(&subject.group_binding_id)
        .bind(&subject.resolved_model_commitment)
        .bind(subject.credential_revision)
        .bind(subject.account_revision)
        .bind(subject.group_revision)
        .bind(subject.endpoint_revision)
        .bind(subject.model_alias_revision)
        .bind(observation.verdict.map(DurableHealthVerdict::as_str))
        .bind(observation.cooldown_until_ms)
        .bind(&observation.evidence_code)
        .bind(&observation.classifier_profile_version)
        .bind(now_ms)
}

async fn upsert_verdict(
    connection: &mut SqliteConnection,
    generation_id: &str,
    observation: &ScopedHealthObservation,
    ingestion_sequence: i64,
    ingested_at_ms: i64,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    let s = &observation.subject;
    sqlx::query("INSERT INTO routing_health_verdicts (generation_id, scope, scope_kind, failure_dimension, station_id, station_key_id, group_binding_id, resolved_model_commitment, credential_revision, account_revision, group_revision, endpoint_revision, model_alias_revision, verdict, cooldown_until_ms, evidence_code, source_observation_id, source_ingested_at_ms, source_ingestion_sequence, projector_version, updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21) ON CONFLICT(generation_id, scope, failure_dimension) DO UPDATE SET verdict=excluded.verdict, cooldown_until_ms=excluded.cooldown_until_ms, evidence_code=excluded.evidence_code, source_observation_id=excluded.source_observation_id, source_ingested_at_ms=excluded.source_ingested_at_ms, source_ingestion_sequence=excluded.source_ingestion_sequence, projector_version=excluded.projector_version, updated_at_ms=excluded.updated_at_ms WHERE excluded.source_ingestion_sequence > routing_health_verdicts.source_ingestion_sequence")
        .bind(generation_id).bind(&s.scope).bind(s.scope_kind.as_str()).bind(observation.dimension.as_str()).bind(&s.station_id)
        .bind(&s.station_key_id).bind(&s.group_binding_id).bind(&s.resolved_model_commitment)
        .bind(s.credential_revision).bind(s.account_revision).bind(s.group_revision)
        .bind(s.endpoint_revision).bind(s.model_alias_revision)
        .bind(observation.verdict.map(DurableHealthVerdict::as_str))
        .bind(observation.cooldown_until_ms).bind(&observation.evidence_code)
        .bind(&observation.observation_id).bind(ingested_at_ms)
        .bind(ingestion_sequence)
        .bind(SCOPED_HEALTH_PROJECTOR_VERSION).bind(now_ms)
        .execute(&mut *connection).await?;
    Ok(())
}

async fn generation_proof(
    connection: &mut SqliteConnection,
    generation_id: &str,
) -> Result<RebuildProof, PersistenceError> {
    let rows = sqlx::query("SELECT scope, failure_dimension, verdict, COALESCE(cooldown_until_ms, -1) AS cooldown, evidence_code, source_observation_id, source_ingestion_sequence FROM routing_health_verdicts WHERE generation_id = ?1 ORDER BY scope ASC, failure_dimension ASC")
        .bind(generation_id).fetch_all(&mut *connection).await?;
    let mut hasher = Sha256::new();
    for row in &rows {
        for value in [
            row.get::<String, _>("scope"),
            row.get::<String, _>("failure_dimension"),
            row.get::<String, _>("verdict"),
            row.get::<i64, _>("cooldown").to_string(),
            row.get::<String, _>("evidence_code"),
            row.get::<String, _>("source_observation_id"),
            row.get::<i64, _>("source_ingestion_sequence").to_string(),
        ] {
            hasher.update(value.len().to_le_bytes());
            hasher.update(value.as_bytes());
        }
    }
    let watermark = sqlx::query("SELECT ingested_at_ms, ingestion_sequence, observation_id FROM routing_health_observations ORDER BY ingestion_sequence DESC LIMIT 1")
        .fetch_optional(&mut *connection).await?
        .map(|row| (row.get("ingested_at_ms"), row.get("ingestion_sequence"), row.get("observation_id")));
    Ok(RebuildProof {
        generation_id: generation_id.to_string(),
        row_count: rows.len() as u64,
        content_hash: format!("{:x}", hasher.finalize()),
        watermark,
    })
}

fn observation_payload_hash(value: &ScopedHealthObservation) -> Result<String, PersistenceError> {
    let encoded = serde_json::to_vec(&(
        &value.logical_request_id,
        value.attempt_ordinal,
        &value.terminal_kind,
        &value.subject,
        value.dimension,
        value.verdict,
        value.cooldown_until_ms,
        &value.evidence_code,
        &value.classifier_profile_version,
    ))
    .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    Ok(hex_digest(encoded))
}

fn validate_observation(
    value: &ScopedHealthObservation,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    for (text, limit) in [
        (value.observation_id.as_str(), 160),
        (value.producer_id.as_str(), 96),
        (value.logical_request_id.as_str(), 160),
        (value.terminal_kind.as_str(), 64),
        (value.evidence_code.as_str(), 96),
        (value.classifier_profile_version.as_str(), 96),
    ] {
        validate_text(text, limit)?;
    }
    if now_ms < 0
        || value.cooldown_until_ms.is_some_and(|time| time < 0)
        || matches!(value.verdict, Some(DurableHealthVerdict::Cooldown))
            != value.cooldown_until_ms.is_some()
        || value.verdict.is_none() && value.cooldown_until_ms.is_some()
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn validate_text(value: &str, max: usize) -> Result<(), PersistenceError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn parse_scope_kind(value: &str) -> Result<HealthScopeKind, PersistenceError> {
    match value {
        "station_key_credential" => Ok(HealthScopeKind::StationKeyCredential),
        "station_account" => Ok(HealthScopeKind::StationAccount),
        "station_group" => Ok(HealthScopeKind::StationGroup),
        "station_endpoint" => Ok(HealthScopeKind::StationEndpoint),
        "model_on_key" => Ok(HealthScopeKind::ModelOnKey),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown scoped routing health kind".into(),
        )),
    }
}

fn parse_failure_dimension(value: &str) -> Result<FailureDimension, PersistenceError> {
    match value {
        "credential" => Ok(FailureDimension::Credential),
        "account_lifecycle" => Ok(FailureDimension::AccountLifecycle),
        "group_subscription" => Ok(FailureDimension::GroupSubscription),
        "balance" => Ok(FailureDimension::Balance),
        "quota" => Ok(FailureDimension::Quota),
        "rate_limit" => Ok(FailureDimension::RateLimit),
        "endpoint_availability" => Ok(FailureDimension::EndpointAvailability),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown scoped routing health dimension".into(),
        )),
    }
}

fn health_protection_scope(
    scope_kind: HealthScopeKind,
    commitment: &str,
) -> Result<HealthProtectionScope, PersistenceError> {
    let kind = match scope_kind {
        HealthScopeKind::StationKeyCredential => HealthProtectionScopeKind::Credential,
        HealthScopeKind::StationAccount => HealthProtectionScopeKind::Account,
        HealthScopeKind::StationGroup => HealthProtectionScopeKind::Group,
        HealthScopeKind::StationEndpoint => HealthProtectionScopeKind::Endpoint,
        HealthScopeKind::ModelOnKey => HealthProtectionScopeKind::Model,
    };
    HealthProtectionScope::new(kind, commitment.to_string())
        .map_err(|_| PersistenceError::ConstraintViolation)
}

fn hex_digest(value: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(value.as_ref()))
}

#[cfg(test)]
mod tests {
    use sqlx::{Connection, Executor, SqliteConnection};

    use super::*;
    use crate::application::error_rate_protection::{
        ErrorRateHistoryEventV1, ErrorRateHistoryOutcome, ErrorRateProtectionConfigV1,
        HealthProtectionTransitionCode,
    };
    use crate::application::health_protection::{
        HealthProtectionFailureCode, HealthProtectionState,
    };
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

    fn error_rate_input(
        id: &str,
        at_ms: i64,
        scope: HealthProtectionScope,
        outcome: HealthProtectionObservationOutcome,
    ) -> (HealthProtectionObservation, ErrorRateHistoryEventV1) {
        let failure_code = outcome.failure_code();
        let failed = outcome.is_failure();
        (
            HealthProtectionObservation {
                id: id.to_string(),
                scope: scope.clone(),
                observed_at_ms: at_ms,
                outcome,
                probe: false,
                probe_state_revision: None,
                retry_after_ms: None,
            },
            ErrorRateHistoryEventV1 {
                observed_at_ms: at_ms,
                scope_kind: scope.kind,
                scope_commitment: scope.commitment,
                outcome: if failed {
                    ErrorRateHistoryOutcome::Failure
                } else {
                    ErrorRateHistoryOutcome::Success
                },
                failure_code,
                sample_count: 1,
                failure_count: usize::from(failed),
                failure_rate_percent: if failed { 100 } else { 0 },
                transition: None,
            },
        )
    }

    fn observation(
        id: &str,
        sequence: u64,
        subject: ScopedHealthSubject,
        verdict: Option<DurableHealthVerdict>,
    ) -> ScopedHealthObservation {
        ScopedHealthObservation {
            observation_id: id.to_string(),
            producer_id: "lifecycle-owner".to_string(),
            producer_sequence: sequence,
            logical_request_id: format!("request-{sequence}"),
            attempt_ordinal: 0,
            terminal_kind: "failed".to_string(),
            subject,
            dimension: FailureDimension::Credential,
            verdict,
            cooldown_until_ms: matches!(verdict, Some(DurableHealthVerdict::Cooldown))
                .then_some(20_000),
            evidence_code: if verdict.is_some() {
                "invalid_api_key".to_string()
            } else {
                "credential_replaced".to_string()
            },
            classifier_profile_version: "provider-rules-v1".to_string(),
        }
    }

    #[test]
    fn typed_scope_matrix_is_closed_and_canonical_keys_do_not_leak_model_names() {
        let credential = ScopedHealthSubject::credential("station", "key", 7).unwrap();
        let account = ScopedHealthSubject::account("station", 3).unwrap();
        let group = ScopedHealthSubject::group("station", "binding", 9).unwrap();
        let endpoint = ScopedHealthSubject::endpoint("station", 11).unwrap();
        let model = ScopedHealthSubject::model_on_key(
            "station",
            "key",
            "private-model-name",
            "deployment-a",
            7,
            11,
            4,
        )
        .unwrap();
        assert_eq!(
            credential.scope_kind(),
            HealthScopeKind::StationKeyCredential
        );
        assert_eq!(account.scope_kind(), HealthScopeKind::StationAccount);
        assert_eq!(group.scope_kind(), HealthScopeKind::StationGroup);
        assert_eq!(endpoint.scope_kind(), HealthScopeKind::StationEndpoint);
        assert_eq!(model.scope_kind(), HealthScopeKind::ModelOnKey);
        assert!(!model.scope().contains("private-model-name"));
        assert!(ScopedHealthSubject::credential("station", "key", 0).is_err());
    }

    #[tokio::test]
    async fn append_projection_cursor_and_recovery_are_atomic_and_idempotent() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let subject = ScopedHealthSubject::credential("station", "key", 1).unwrap();
        let blocked = observation(
            "observation-1",
            1,
            subject.clone(),
            Some(DurableHealthVerdict::Blocked),
        );
        connection.execute("BEGIN IMMEDIATE").await.unwrap();
        assert_eq!(
            store
                .apply_observation(&mut connection, &blocked, 1_000)
                .await
                .unwrap(),
            ScopedObservationApplyResult::Applied
        );
        connection.execute("COMMIT").await.unwrap();
        assert_eq!(
            store
                .apply_observation(&mut connection, &blocked, 1_001)
                .await
                .unwrap(),
            ScopedObservationApplyResult::Existing
        );
        let rows = store
            .load_active_batch(&mut connection, &[subject.clone()])
            .await
            .unwrap();
        assert_eq!(
            rows[&(subject.scope().to_string(), FailureDimension::Credential)].verdict,
            DurableHealthVerdict::Blocked
        );

        let recovery = observation("observation-2", 2, subject.clone(), None);
        store
            .apply_observation(&mut connection, &recovery, 1_002)
            .await
            .unwrap();
        assert!(store
            .load_active_batch(&mut connection, &[subject])
            .await
            .unwrap()
            .is_empty());
        let cursor: (i64, String) = sqlx::query_as(
            "SELECT watermark_ingested_at_ms, watermark_observation_id FROM routing_health_projector_state WHERE singleton_key = 1",
        )
        .fetch_one(&mut connection)
        .await
        .unwrap();
        assert_eq!(cursor, (1_002, "observation-2".to_string()));
    }

    #[tokio::test]
    async fn recovery_does_not_clear_a_projection_from_a_newer_ingestion_source() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let subject = ScopedHealthSubject::credential("station", "key", 1).unwrap();
        let blocked = observation(
            "newer-verdict",
            1,
            subject.clone(),
            Some(DurableHealthVerdict::Blocked),
        );
        store
            .apply_observation(&mut connection, &blocked, 1_000)
            .await
            .unwrap();

        // Model a projection source that was committed after the recovery's
        // source but became visible before the recovery transaction ran.
        sqlx::query(
            "UPDATE routing_health_verdicts SET source_ingestion_sequence = 99 WHERE scope = ?1",
        )
        .bind(subject.scope())
        .execute(&mut connection)
        .await
        .unwrap();

        let recovery = observation("older-recovery", 2, subject.clone(), None);
        store
            .apply_observation(&mut connection, &recovery, 1_001)
            .await
            .unwrap();

        let rows = store
            .load_active_batch(&mut connection, &[subject])
            .await
            .unwrap();
        let key = (
            blocked.subject.scope().to_string(),
            FailureDimension::Credential,
        );
        assert_eq!(rows[&key].verdict, DurableHealthVerdict::Blocked);
    }

    #[tokio::test]
    async fn durable_reducer_state_is_written_once_and_restored_from_snapshot() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let subject = ScopedHealthSubject::endpoint("station", 1).unwrap();
        let blocked = observation(
            "reducer-blocked",
            1,
            subject.clone(),
            Some(DurableHealthVerdict::Blocked),
        );
        store
            .apply_observation(&mut connection, &blocked, 1_000)
            .await
            .unwrap();
        let snapshot_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM routing_health_protection_state")
                .fetch_one(&mut connection)
                .await
                .unwrap();
        assert_eq!(snapshot_count, 1);
        let statuses = store
            .load_health_protection_statuses(&mut connection, 1_001)
            .await
            .unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(
            statuses[0].state,
            crate::application::health_protection::HealthProtectionState::Open
        );
        assert_eq!(
            statuses[0].persistence_kind,
            crate::application::health_protection::HealthProtectionPersistenceKind::Durable
        );

        let recovery = observation("reducer-recovery", 2, subject, None);
        store
            .apply_observation(&mut connection, &recovery, 1_002)
            .await
            .unwrap();
        let restored = store
            .load_health_protection_reducer(&mut connection)
            .await
            .unwrap()
            .expect("durable reducer snapshot");
        let restored_statuses = {
            let mut reducer = restored;
            reducer.statuses(1_003).unwrap()
        };
        assert_eq!(
            restored_statuses[0].state,
            crate::application::health_protection::HealthProtectionState::Closed
        );
    }

    #[tokio::test]
    async fn malformed_or_future_reducer_snapshot_fails_closed() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let subject = ScopedHealthSubject::endpoint("station", 1).unwrap();
        store
            .apply_observation(
                &mut connection,
                &observation(
                    "snapshot-integrity",
                    1,
                    subject,
                    Some(DurableHealthVerdict::Blocked),
                ),
                1_000,
            )
            .await
            .unwrap();
        sqlx::query(
            "UPDATE routing_health_protection_state SET snapshot_version = 'health_protection_future' WHERE singleton_key = 1",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        assert!(matches!(
            store.load_health_protection_reducer(&mut connection).await,
            Err(PersistenceError::InvariantViolation(_))
        ));
    }

    #[tokio::test]
    async fn observation_and_reducer_snapshot_share_caller_transaction() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let subject = ScopedHealthSubject::endpoint("station", 1).unwrap();
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut connection)
            .await
            .unwrap();
        store
            .apply_observation(
                &mut connection,
                &observation(
                    "atomic-health-state",
                    1,
                    subject,
                    Some(DurableHealthVerdict::Blocked),
                ),
                1_000,
            )
            .await
            .unwrap();
        sqlx::query("ROLLBACK")
            .execute(&mut connection)
            .await
            .unwrap();
        let observation_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_health_observations WHERE observation_id = 'atomic-health-state'",
        )
        .fetch_one(&mut connection)
        .await
        .unwrap();
        let state_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM routing_health_protection_state")
                .fetch_one(&mut connection)
                .await
                .unwrap();
        assert_eq!(observation_count, 0);
        assert_eq!(state_count, 0);
    }

    #[tokio::test]
    async fn current_projection_rebuilds_a_reset_runtime_projection_from_preserved_observations() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let subject = ScopedHealthSubject::group("station", "group-binding", 1).unwrap();
        let scope = subject.scope().to_string();
        let observation = ScopedHealthObservation {
            dimension: FailureDimension::GroupSubscription,
            evidence_code: "group_disabled".to_string(),
            ..observation(
                "portable-history-observation",
                1,
                subject.clone(),
                Some(DurableHealthVerdict::Blocked),
            )
        };
        store
            .apply_observation(&mut connection, &observation, 1_000)
            .await
            .unwrap();

        // This is the portable-import shape: immutable history is retained,
        // while device-local projection tables and their checkpoint are reset.
        connection
            .execute("DELETE FROM routing_health_verdicts")
            .await
            .unwrap();
        connection
            .execute("DELETE FROM routing_health_generations WHERE generation_id <> 'scoped-health-bootstrap-v1'")
            .await
            .unwrap();
        connection
            .execute("UPDATE routing_health_generations SET status = 'active' WHERE generation_id = 'scoped-health-bootstrap-v1'")
            .await
            .unwrap();
        connection
            .execute("UPDATE routing_health_projector_state SET active_generation_id = 'scoped-health-bootstrap-v1', projector_version = 'scoped-health-projector-v1', watermark_ingested_at_ms = NULL, watermark_ingestion_sequence = NULL, watermark_observation_id = NULL")
            .await
            .unwrap();

        assert!(store
            .ensure_current_projection(&mut connection, 2_000)
            .await
            .unwrap());
        let rows = store
            .load_active_batch(&mut connection, &[subject])
            .await
            .unwrap();
        assert_eq!(
            rows[&(scope, FailureDimension::GroupSubscription)].verdict,
            DurableHealthVerdict::Blocked
        );
        assert!(
            !store
                .ensure_current_projection(&mut connection, 2_001)
                .await
                .unwrap(),
            "the rebuilt checkpoint must make the second startup a no-op"
        );
    }

    #[tokio::test]
    async fn identity_collision_fails_closed_and_revision_change_recovers_without_ttl() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let old = ScopedHealthSubject::credential("station", "key", 1).unwrap();
        let blocked = observation(
            "collision",
            1,
            old.clone(),
            Some(DurableHealthVerdict::Blocked),
        );
        store
            .apply_observation(&mut connection, &blocked, 100)
            .await
            .unwrap();
        let mut conflicting = blocked;
        conflicting.evidence_code = "different_payload".to_string();
        assert!(matches!(
            store
                .apply_observation(&mut connection, &conflicting, 101)
                .await,
            Err(PersistenceError::InvariantViolation(_))
        ));
        let changed_revision = ScopedHealthSubject::credential("station", "key", 2).unwrap();
        assert!(store
            .load_active_batch(&mut connection, &[changed_revision])
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .load_active_batch(&mut connection, &[old.clone(), old])
                .await
                .unwrap()
                .len(),
            1,
            "batch input is deduplicated"
        );
    }

    #[tokio::test]
    async fn shadow_rebuild_has_no_empty_window_and_requires_verified_cutover() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let subject = ScopedHealthSubject::endpoint("station", 1).unwrap();
        let failure = observation(
            "endpoint-failure",
            1,
            subject.clone(),
            Some(DurableHealthVerdict::Degraded),
        );
        store
            .apply_observation(&mut connection, &failure, 500)
            .await
            .unwrap();
        let active_before: String = sqlx::query_scalar(
            "SELECT active_generation_id FROM routing_health_projector_state WHERE singleton_key = 1",
        )
        .fetch_one(&mut connection)
        .await
        .unwrap();
        let proof = store
            .rebuild_shadow(&mut connection, "shadow-v2", 501)
            .await
            .unwrap();
        assert_eq!(proof.row_count, 1);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT active_generation_id FROM routing_health_projector_state WHERE singleton_key = 1",
            )
            .fetch_one(&mut connection)
            .await
            .unwrap(),
            active_before,
            "a crash before cutover leaves the prior generation authoritative"
        );
        connection.execute("BEGIN IMMEDIATE").await.unwrap();
        store
            .activate_generation(&mut connection, &proof, 502)
            .await
            .unwrap();
        connection.execute("COMMIT").await.unwrap();
        assert_eq!(
            store
                .load_active_batch(&mut connection, &[subject])
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn shadow_rebuild_rejects_a_stale_checkpoint_and_swaps_the_complete_cursor() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let endpoint = ScopedHealthSubject::endpoint("station", 1).unwrap();
        let account = ScopedHealthSubject::account("station", 1).unwrap();
        store
            .apply_observation(
                &mut connection,
                &observation(
                    "endpoint-failure",
                    1,
                    endpoint.clone(),
                    Some(DurableHealthVerdict::Degraded),
                ),
                500,
            )
            .await
            .unwrap();
        let stale_proof = store
            .rebuild_shadow(&mut connection, "shadow-stale", 501)
            .await
            .unwrap();
        assert_eq!(
            stale_proof.watermark,
            Some((500, 1, "endpoint-failure".to_string()))
        );

        let mut account_failure = observation(
            "account-failure",
            2,
            account.clone(),
            Some(DurableHealthVerdict::Blocked),
        );
        account_failure.dimension = FailureDimension::AccountLifecycle;
        store
            .apply_observation(&mut connection, &account_failure, 400)
            .await
            .unwrap();
        assert!(matches!(
            store
                .activate_generation(&mut connection, &stale_proof, 502)
                .await,
            Err(PersistenceError::InvariantViolation(_))
        ));

        let proof = store
            .rebuild_shadow(&mut connection, "shadow-current", 503)
            .await
            .unwrap();
        assert_eq!(
            proof.watermark,
            Some((400, 2, "account-failure".to_string()))
        );
        connection.execute("BEGIN IMMEDIATE").await.unwrap();
        store
            .activate_generation(&mut connection, &proof, 504)
            .await
            .unwrap();
        connection.execute("COMMIT").await.unwrap();

        let checkpoint: (i64, i64, String) = sqlx::query_as(
            "SELECT watermark_ingested_at_ms, watermark_ingestion_sequence, watermark_observation_id FROM routing_health_projector_state WHERE singleton_key = 1",
        )
        .fetch_one(&mut connection)
        .await
        .unwrap();
        assert_eq!(checkpoint, (400, 2, "account-failure".to_string()));
        assert_eq!(
            store
                .load_active_batch(&mut connection, &[endpoint, account])
                .await
                .unwrap()
                .len(),
            2,
            "the atomically activated shadow contains every observation through its sequence cursor"
        );
    }

    #[tokio::test]
    async fn batch_read_uses_one_bind_and_index_lookup_for_maximum_candidate_shape() {
        let mut connection = connection().await;
        let subjects = (0..4_096)
            .map(|index| {
                ScopedHealthSubject::credential("station", format!("key-{index}"), 1).unwrap()
            })
            .collect::<Vec<_>>();
        assert!(RoutingHealthVerdictStore
            .load_active_batch(&mut connection, &subjects)
            .await
            .unwrap()
            .is_empty());
        assert!(RoutingHealthVerdictStore
            .load_active_batch(
                &mut connection,
                &[
                    subjects,
                    vec![ScopedHealthSubject::credential("station", "overflow", 1).unwrap()]
                ]
                .concat(),
            )
            .await
            .is_err());
        let plan = sqlx::query(
            "EXPLAIN QUERY PLAN WITH requested(scope) AS (SELECT value FROM json_each(?1)) SELECT v.scope FROM requested r JOIN routing_health_projector_state state ON state.singleton_key = 1 JOIN routing_health_verdicts v ON v.generation_id = state.active_generation_id AND v.scope = r.scope",
        )
        .bind("[]")
        .fetch_all(&mut connection)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect::<Vec<_>>()
        .join("\n");
        assert!(
            plan.contains("sqlite_autoindex_routing_health_verdicts_1"),
            "{plan}"
        );
    }

    #[tokio::test]
    async fn dimensions_coexist_and_recovery_only_clears_the_matching_dimension() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let subject = ScopedHealthSubject::account("station", 1).unwrap();
        let mut disabled = observation(
            "account-disabled",
            1,
            subject.clone(),
            Some(DurableHealthVerdict::Blocked),
        );
        disabled.dimension = FailureDimension::AccountLifecycle;
        let mut balance = observation(
            "balance-depleted",
            2,
            subject.clone(),
            Some(DurableHealthVerdict::Blocked),
        );
        balance.dimension = FailureDimension::Balance;
        store
            .apply_observation(&mut connection, &disabled, 100)
            .await
            .unwrap();
        store
            .apply_observation(&mut connection, &balance, 100)
            .await
            .unwrap();
        let rows = store
            .load_active_batch(&mut connection, &[subject.clone()])
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);

        let mut balance_recovered = observation("balance-recovered", 3, subject.clone(), None);
        balance_recovered.dimension = FailureDimension::Balance;
        store
            .apply_observation(&mut connection, &balance_recovered, 50)
            .await
            .unwrap();
        let rows = store
            .load_active_batch(&mut connection, &[subject])
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows
            .keys()
            .any(|(_, dimension)| *dimension == FailureDimension::AccountLifecycle));
        let sequences: Vec<i64> = sqlx::query_scalar(
            "SELECT ingestion_sequence FROM routing_health_observations ORDER BY ingestion_sequence",
        ).fetch_all(&mut connection).await.unwrap();
        assert_eq!(
            sequences,
            vec![1, 2, 3],
            "wall-clock regression cannot reorder ingestion"
        );
    }

    #[tokio::test]
    async fn unsupported_model_has_one_capability_owner_and_no_health_duplicate() {
        let mut connection = connection().await;
        let observation = UnsupportedModelObservation {
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
        };
        assert_eq!(
            RoutingHealthVerdictStore
                .apply_unsupported_model(&mut connection, &observation, 100)
                .await
                .unwrap(),
            ScopedObservationApplyResult::Applied
        );
        assert_eq!(
            RoutingHealthVerdictStore
                .apply_unsupported_model(&mut connection, &observation, 101)
                .await
                .unwrap(),
            ScopedObservationApplyResult::Existing
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

        let store = RoutingHealthVerdictStore;
        let exact = vec![("key".to_string(), "gpt-test".to_string(), 1, 1, 1)];
        assert_eq!(
            store
                .load_unsupported_model_batch(&mut connection, &exact)
                .await
                .unwrap()
                .len(),
            1,
            "the next planning snapshot must exclude the exact learned tuple"
        );
        for changed_revision in [(2, 1, 1), (1, 2, 1)] {
            let changed = vec![(
                "key".to_string(),
                "gpt-test".to_string(),
                changed_revision.0,
                changed_revision.1,
                changed_revision.2,
            )];
            assert!(store
                .load_unsupported_model_batch(&mut connection, &changed)
                .await
                .unwrap()
                .is_empty());
        }
        let changed_mapping_revision = vec![("key".to_string(), "gpt-test".to_string(), 1, 1, 99)];
        assert_eq!(
            store
                .load_unsupported_model_batch(&mut connection, &changed_mapping_revision)
                .await
                .unwrap()
                .len(),
            1,
            "mapping revision is provenance and must not invalidate native capability identity"
        );
        let mut newer_mapping_observation = observation.clone();
        newer_mapping_observation.observation_id = "model-unsupported-newer".to_string();
        newer_mapping_observation.logical_request_id = "request-model-newer".to_string();
        newer_mapping_observation.model_alias_revision = 99;
        newer_mapping_observation.model_mapping_revision = Some(99);
        newer_mapping_observation.model_resolution_fence = Some("mapping-fence-v99".to_string());
        assert_eq!(
            store
                .apply_unsupported_model(&mut connection, &newer_mapping_observation, 102)
                .await
                .unwrap(),
            ScopedObservationApplyResult::Applied
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM routing_capability_model_verdicts WHERE station_key_id = 'key' AND resolved_model = 'gpt-test'",
            )
            .fetch_one(&mut connection)
            .await
            .unwrap(),
            1,
            "mapping revision changes update one native identity instead of creating a new verdict"
        );
        let native_identity: (i64, String, String, i64, Option<i64>) = sqlx::query_as(
            "SELECT identity_version, endpoint_kind, protocol_kind, model_alias_revision, model_mapping_revision FROM routing_capability_model_verdicts WHERE station_key_id = 'key' AND resolved_model = 'gpt-test'",
        )
        .fetch_one(&mut connection)
        .await
        .unwrap();
        assert_eq!(native_identity.0, NATIVE_MODEL_CAPABILITY_IDENTITY_VERSION);
        assert_eq!(native_identity.1, "unknown");
        assert_eq!(native_identity.2, "unknown");
        assert_eq!(native_identity.3, 99);
        assert_eq!(native_identity.4, Some(99));

        sqlx::query("INSERT INTO routing_capability_model_verdicts (station_key_id,resolved_model,credential_revision,endpoint_revision,model_alias_revision,verdict,source_observation_id,source_ingestion_sequence,projector_version,updated_at_ms) VALUES ('legacy-key','legacy-model',1,1,77,'unsupported','legacy-observation',1,'capability_evidence_v1',100)")
            .execute(&mut connection)
            .await
            .unwrap();
        assert!(
            store
                .load_unsupported_model_batch(
                    &mut connection,
                    &[(
                        "legacy-key".to_string(),
                        "legacy-model".to_string(),
                        1,
                        1,
                        77,
                    )]
                )
                .await
                .unwrap()
                .is_empty(),
            "legacy alias-keyed rows are not consumed by native planner reads"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM routing_capability_model_verdicts WHERE resolved_model = 'manual-blocked-model'"
            )
            .fetch_one(&mut connection)
            .await
            .unwrap(),
            0,
            "manual model blocklists are configuration and never become learned verdicts"
        );
    }

    #[tokio::test]
    async fn error_rate_history_and_reducer_transition_are_atomic_and_restartable() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let config = ErrorRateProtectionConfigV1 {
            enabled: true,
            history_max_events: 3,
            history_retention_ms: 10_000,
            ..Default::default()
        };
        let scope = HealthProtectionScope::from_untrusted(
            HealthProtectionScopeKind::Endpoint,
            "https://provider.invalid/v1?token=secret",
        );
        for index in 0..5_i64 {
            let (input, event) = error_rate_input(
                &format!("rate-{index}"),
                index + 1,
                scope.clone(),
                HealthProtectionObservationOutcome::Failure(
                    HealthProtectionFailureCode::Upstream5xx,
                ),
            );
            store
                .apply_error_rate_observation(
                    &mut connection,
                    input,
                    event,
                    &format!("rate-{index}"),
                    &config,
                    index + 1,
                )
                .await
                .expect("apply error-rate observation");
        }
        let history = RoutingErrorRateHistoryStore
            .list_page(&mut connection, None, 20, &config, 20)
            .await
            .expect("list durable history");
        assert_eq!(history.events.len(), 3);
        assert!(history.dropped_events >= 2);
        assert_eq!(
            history.events.last().and_then(|event| event.transition),
            Some(HealthProtectionTransitionCode::Opened)
        );
        let statuses = store
            .load_health_protection_statuses(&mut connection, 20)
            .await
            .expect("load reducer status");
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].state, HealthProtectionState::Open);
        assert!(statuses[0]
            .scope
            .commitment
            .chars()
            .all(|value| value.is_ascii_hexdigit()));

        let restored = store
            .load_health_protection_reducer(&mut connection)
            .await
            .expect("restore reducer")
            .expect("durable reducer");
        assert_eq!(restored.snapshot(20).generated_at_ms, 20);

        sqlx::query("BEGIN")
            .execute(&mut connection)
            .await
            .expect("begin rollback transaction");
        let (input, event) = error_rate_input(
            "rolled-back",
            21,
            scope,
            HealthProtectionObservationOutcome::Success,
        );
        store
            .apply_error_rate_observation(&mut connection, input, event, "rolled-back", &config, 21)
            .await
            .expect("apply in transaction");
        sqlx::query("ROLLBACK")
            .execute(&mut connection)
            .await
            .expect("rollback transaction");
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_error_rate_history WHERE observation_id = 'rolled-back'",
        )
        .fetch_one(&mut connection)
        .await
        .expect("count rolled-back history");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn durable_probe_reservation_is_single_use_and_revision_fenced() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let config = ErrorRateProtectionConfigV1 {
            enabled: true,
            history_max_events: 16,
            history_retention_ms: 10_000,
            ..Default::default()
        };
        let scope = HealthProtectionScope::from_untrusted(
            HealthProtectionScopeKind::Endpoint,
            "https://provider.invalid/probe?secret=redacted",
        );
        for index in 0..5_i64 {
            let (input, event) = error_rate_input(
                &format!("probe-open-{index}"),
                index + 1,
                scope.clone(),
                HealthProtectionObservationOutcome::Failure(
                    HealthProtectionFailureCode::EndpointUnavailable,
                ),
            );
            store
                .apply_error_rate_observation(
                    &mut connection,
                    input,
                    event,
                    &format!("probe-open-{index}"),
                    &config,
                    index + 1,
                )
                .await
                .expect("open durable scope");
        }
        let first = store
            .begin_health_protection_probe(&mut connection, &scope, 31_000)
            .await
            .expect("reserve probe")
            .expect("cooldown expired");
        assert!(store
            .begin_health_protection_probe(&mut connection, &scope, 31_000)
            .await
            .expect("second reservation")
            .is_none());
        let statuses = store
            .load_health_protection_statuses(&mut connection, 31_000)
            .await
            .expect("load half-open status");
        assert_eq!(statuses[0].state, HealthProtectionState::HalfOpen);
        assert!(statuses[0].half_open_probe_in_flight);

        let mut stale = HealthProtectionObservation {
            id: "probe-stale".to_string(),
            scope: scope.clone(),
            observed_at_ms: 31_000,
            outcome: HealthProtectionObservationOutcome::Success,
            probe: true,
            probe_state_revision: Some(first.state_revision.saturating_add(1)),
            retry_after_ms: None,
        };
        assert!(store
            .apply_health_protection_probe(&mut connection, stale.clone(), 31_000)
            .await
            .is_err());
        stale.probe_state_revision = Some(first.state_revision);
        assert_eq!(
            store
                .apply_health_protection_probe(&mut connection, stale, 31_000)
                .await
                .expect("consume current probe"),
            crate::application::health_protection::HealthProtectionTransition::ProbeSucceeded
        );
    }

    #[tokio::test]
    async fn durable_probe_cancellation_releases_slot_without_recording_sample() {
        let mut connection = connection().await;
        let store = RoutingHealthVerdictStore;
        let config = ErrorRateProtectionConfigV1 {
            enabled: true,
            history_max_events: 16,
            history_retention_ms: 10_000,
            ..Default::default()
        };
        let scope = HealthProtectionScope::from_untrusted(
            HealthProtectionScopeKind::Endpoint,
            "https://provider.invalid/cancel?secret=redacted",
        );
        for index in 0..5_i64 {
            let (input, event) = error_rate_input(
                &format!("cancel-open-{index}"),
                index + 1,
                scope.clone(),
                HealthProtectionObservationOutcome::Failure(
                    HealthProtectionFailureCode::EndpointUnavailable,
                ),
            );
            store
                .apply_error_rate_observation(
                    &mut connection,
                    input,
                    event,
                    &format!("cancel-open-{index}"),
                    &config,
                    index + 1,
                )
                .await
                .expect("open durable scope");
        }
        let first = store
            .begin_health_protection_probe(&mut connection, &scope, 31_000)
            .await
            .expect("reserve probe")
            .expect("cooldown expired");
        let history_count_before: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_error_rate_history WHERE scope_commitment = ?1",
        )
        .bind(&scope.commitment)
        .fetch_one(&mut connection)
        .await
        .expect("history count before cancel");
        assert!(store
            .cancel_health_protection_probe(&mut connection, &first, 31_001)
            .await
            .expect("cancel probe"));
        let status = store
            .load_health_protection_statuses(&mut connection, 31_001)
            .await
            .expect("status")
            .into_iter()
            .find(|status| status.scope == scope)
            .expect("scope status");
        assert_eq!(status.state, HealthProtectionState::HalfOpen);
        assert!(!status.half_open_probe_in_flight);
        let history_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_error_rate_history WHERE scope_commitment = ?1",
        )
        .bind(&scope.commitment)
        .fetch_one(&mut connection)
        .await
        .expect("history count");
        assert_eq!(history_count, history_count_before);
        assert!(store
            .apply_health_protection_probe(
                &mut connection,
                HealthProtectionObservation {
                    id: "cancelled-late".to_string(),
                    scope: scope.clone(),
                    observed_at_ms: 31_001,
                    outcome: HealthProtectionObservationOutcome::Success,
                    probe: true,
                    probe_state_revision: Some(first.state_revision),
                    retry_after_ms: None,
                },
                31_001,
            )
            .await
            .is_err());
        let next = store
            .begin_health_protection_probe(&mut connection, &scope, 31_001)
            .await
            .expect("reserve next probe")
            .expect("half-open slot available");
        assert!(next.state_revision > first.state_revision);
    }
}
