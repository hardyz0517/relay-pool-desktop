use std::{
    collections::BTreeMap,
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::models::operational::{
    EndpointFacts, EndpointId, EndpointRef, EndpointRevision, OperationalValidationError,
    OutboundPolicyRef, RecordRevision, SanitizedOrigin, StationId, StationKeyId,
};

pub(crate) const MAX_OPERATIONAL_CANDIDATES: usize = 1024;

static SNAPSHOT_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OperationalFactAssemblyError {
    CandidateLimitExceeded { actual: usize, limit: usize },
    InvalidFact(OperationalValidationError),
}

impl fmt::Display for OperationalFactAssemblyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CandidateLimitExceeded { actual, limit } => {
                write!(formatter, "operational candidate count {actual} exceeds limit {limit}")
            }
            Self::InvalidFact(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for OperationalFactAssemblyError {}

impl From<OperationalValidationError> for OperationalFactAssemblyError {
    fn from(error: OperationalValidationError) -> Self {
        Self::InvalidFact(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationalFactReadOptions {
    candidate_limit: usize,
    requested_model: Option<String>,
    include_model_catalog: bool,
}

impl OperationalFactReadOptions {
    pub(crate) fn for_request_model(model: impl Into<String>) -> Self {
        Self {
            candidate_limit: MAX_OPERATIONAL_CANDIDATES,
            requested_model: Some(model.into()),
            include_model_catalog: false,
        }
    }

    pub(crate) fn for_model_catalog() -> Self {
        Self {
            candidate_limit: MAX_OPERATIONAL_CANDIDATES,
            requested_model: None,
            include_model_catalog: true,
        }
    }

    pub(crate) fn with_candidate_limit(mut self, candidate_limit: usize) -> Self {
        self.candidate_limit = candidate_limit;
        self
    }

    pub(crate) fn candidate_limit(&self) -> usize {
        self.candidate_limit
    }

    pub(crate) fn requested_model(&self) -> Option<&str> {
        self.requested_model.as_deref()
    }

    pub(crate) fn include_model_catalog(&self) -> bool {
        self.include_model_catalog
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationalFactSnapshotId(String);

impl OperationalFactSnapshotId {
    fn next() -> Self {
        let value = SNAPSHOT_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("ofs-{value:016x}"))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FactVersionVector {
    max_station_revision: i64,
    max_key_revision: i64,
    max_settings_revision: i64,
    max_alias_revision: i64,
}

impl FactVersionVector {
    pub(crate) fn new(
        max_station_revision: i64,
        max_key_revision: i64,
        max_settings_revision: i64,
        max_alias_revision: i64,
    ) -> Self {
        Self {
            max_station_revision,
            max_key_revision,
            max_settings_revision,
            max_alias_revision,
        }
    }

    pub(crate) fn max_key_revision(&self) -> i64 {
        self.max_key_revision
    }

    pub(crate) fn max_settings_revision(&self) -> i64 {
        self.max_settings_revision
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawOperationalCandidateRow {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) api_base_url: String,
    pub(crate) credential_available: bool,
    pub(crate) key_record_revision: i64,
    pub(crate) station_record_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawOperationalSettingRow {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) record_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawOperationalModelAliasRow {
    pub(crate) client_model: String,
    pub(crate) upstream_model: String,
    pub(crate) record_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawOperationalFactRows {
    pub(crate) candidates: Vec<RawOperationalCandidateRow>,
    pub(crate) settings: Vec<RawOperationalSettingRow>,
    pub(crate) model_aliases: Vec<RawOperationalModelAliasRow>,
    pub(crate) query_count: usize,
    pub(crate) loaded_full_model_catalog: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CredentialAvailabilityFact {
    available: bool,
    record_revision: RecordRevision,
}

impl CredentialAvailabilityFact {
    pub(crate) fn new(
        available: bool,
        record_revision: RecordRevision,
    ) -> Self {
        Self {
            available,
            record_revision,
        }
    }

    pub(crate) fn available(&self) -> bool {
        self.available
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationalCandidateFact {
    station_key_id: StationKeyId,
    station_id: StationId,
    endpoint: EndpointFacts,
    credential: CredentialAvailabilityFact,
}

impl OperationalCandidateFact {
    pub(crate) fn station_key_id(&self) -> &StationKeyId {
        &self.station_key_id
    }

    pub(crate) fn endpoint(&self) -> &EndpointFacts {
        &self.endpoint
    }

    pub(crate) fn credential(&self) -> &CredentialAvailabilityFact {
        &self.credential
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettingFact {
    key: String,
    value: String,
    record_revision: RecordRevision,
}

impl SettingFact {
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelAliasFact {
    client_model: String,
    upstream_model: String,
    record_revision: RecordRevision,
}

impl ModelAliasFact {
    pub(crate) fn client_model(&self) -> &str {
        &self.client_model
    }

    pub(crate) fn upstream_model(&self) -> &str {
        &self.upstream_model
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationalFactBundle {
    snapshot_id: OperationalFactSnapshotId,
    version_vector: FactVersionVector,
    candidates: Vec<OperationalCandidateFact>,
    settings_by_key: BTreeMap<String, SettingFact>,
    model_aliases: Vec<ModelAliasFact>,
    query_count: usize,
    loaded_full_model_catalog: bool,
}

impl OperationalFactBundle {
    pub(crate) fn snapshot_id(&self) -> &OperationalFactSnapshotId {
        &self.snapshot_id
    }

    pub(crate) fn version_vector(&self) -> &FactVersionVector {
        &self.version_vector
    }

    pub(crate) fn candidates(&self) -> &[OperationalCandidateFact] {
        &self.candidates
    }

    pub(crate) fn settings_by_key(&self) -> &BTreeMap<String, SettingFact> {
        &self.settings_by_key
    }

    pub(crate) fn model_aliases(&self) -> &[ModelAliasFact] {
        &self.model_aliases
    }

    pub(crate) fn query_count(&self) -> usize {
        self.query_count
    }

    pub(crate) fn loaded_full_model_catalog(&self) -> bool {
        self.loaded_full_model_catalog
    }
}

pub(crate) fn assemble_operational_fact_bundle(
    raw: RawOperationalFactRows,
    options: &OperationalFactReadOptions,
) -> Result<OperationalFactBundle, OperationalFactAssemblyError> {
    if raw.candidates.len() > options.candidate_limit() {
        return Err(OperationalFactAssemblyError::CandidateLimitExceeded {
            actual: raw.candidates.len(),
            limit: options.candidate_limit(),
        });
    }

    let mut max_station_revision = 1;
    let mut max_key_revision = 1;
    let candidates = raw
        .candidates
        .into_iter()
        .map(|row| {
            let station_id = StationId::new(row.station_id)?;
            let station_key_id = StationKeyId::new(row.station_key_id)?;
            let endpoint_revision = EndpointRevision::new(row.endpoint_revision)?;
            let key_record_revision = RecordRevision::new(row.key_record_revision)?;
            max_station_revision = max_station_revision.max(row.station_record_revision);
            max_key_revision = max_key_revision.max(row.key_record_revision);
            let endpoint = EndpointFacts::new(
                EndpointRef::new(
                    station_id.clone(),
                    EndpointId::new("primary")?,
                    endpoint_revision,
                ),
                SanitizedOrigin::from_endpoint_url(&row.api_base_url)?,
                OutboundPolicyRef::new("station-default")?,
            );
            Ok(OperationalCandidateFact {
                station_key_id,
                station_id,
                endpoint,
                credential: CredentialAvailabilityFact::new(
                    row.credential_available,
                    key_record_revision,
                ),
            })
        })
        .collect::<Result<Vec<_>, OperationalFactAssemblyError>>()?;

    let mut max_settings_revision = 1;
    let settings_by_key = raw
        .settings
        .into_iter()
        .map(|row| {
            let revision = RecordRevision::new(row.record_revision)?;
            max_settings_revision = max_settings_revision.max(row.record_revision);
            Ok((
                row.key.clone(),
                SettingFact {
                    key: row.key,
                    value: row.value,
                    record_revision: revision,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, OperationalFactAssemblyError>>()?;

    let mut max_alias_revision = 1;
    let model_aliases = raw
        .model_aliases
        .into_iter()
        .map(|row| {
            let revision = RecordRevision::new(row.record_revision)?;
            max_alias_revision = max_alias_revision.max(row.record_revision);
            Ok(ModelAliasFact {
                client_model: row.client_model,
                upstream_model: row.upstream_model,
                record_revision: revision,
            })
        })
        .collect::<Result<Vec<_>, OperationalFactAssemblyError>>()?;

    Ok(OperationalFactBundle {
        snapshot_id: OperationalFactSnapshotId::next(),
        version_vector: FactVersionVector::new(
            max_station_revision,
            max_key_revision,
            max_settings_revision,
            max_alias_revision,
        ),
        candidates,
        settings_by_key,
        model_aliases,
        query_count: raw.query_count,
        loaded_full_model_catalog: raw.loaded_full_model_catalog,
    })
}
