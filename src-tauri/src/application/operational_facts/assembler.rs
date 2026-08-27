use std::{
    collections::BTreeMap,
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::models::operational::{
    EndpointFacts, EndpointId, EndpointRef, EndpointRevision, OperationalFactReadOptions,
    OperationalValidationError, OutboundPolicyRef, RawOperationalFactRows, RecordRevision,
    SanitizedOrigin, StationId, StationKeyId,
};

static SNAPSHOT_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OperationalFactAssemblyError {
    CandidateLimitExceeded { actual: usize, limit: usize },
    InvalidJson { field: &'static str },
    InvalidFact(OperationalValidationError),
}

impl fmt::Display for OperationalFactAssemblyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CandidateLimitExceeded { actual, limit } => {
                write!(
                    formatter,
                    "operational candidate count {actual} exceeds limit {limit}"
                )
            }
            Self::InvalidJson { field } => {
                write!(formatter, "operational fact JSON is invalid for {field}")
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

    pub(crate) fn max_station_revision(&self) -> i64 {
        self.max_station_revision
    }

    pub(crate) fn max_alias_revision(&self) -> i64 {
        self.max_alias_revision
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CredentialAvailabilityFact {
    available: bool,
    record_revision: RecordRevision,
}

impl CredentialAvailabilityFact {
    pub(crate) fn new(available: bool, record_revision: RecordRevision) -> Self {
        Self {
            available,
            record_revision,
        }
    }

    pub(crate) fn available(&self) -> bool {
        self.available
    }

    pub(crate) fn record_revision(&self) -> RecordRevision {
        self.record_revision
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OperationalCandidateFact {
    station_key_id: StationKeyId,
    station_id: StationId,
    capacity_provider_family: Option<String>,
    capacity_deployment_identity: Option<String>,
    capacity_region_identity: Option<String>,
    capacity_domain_revision: Option<RecordRevision>,
    endpoint: EndpointFacts,
    credential: CredentialAvailabilityFact,
    schedulable: bool,
    priority: i64,
    backup_only: bool,
    group_binding_id: Option<String>,
    group_record_revision: Option<RecordRevision>,
    group_binding_status: Option<String>,
    station_native_multiplier: Option<f64>,
    credit_per_cny: Option<f64>,
    account_record_revision: RecordRevision,
    group_id_hash: Option<String>,
    group_category: Option<String>,
    supports_chat_completions: bool,
    supports_responses: bool,
    supports_stream: bool,
    supports_tools: bool,
    supports_vision: bool,
    supports_reasoning: bool,
    model_allowlist: Vec<String>,
    model_blocklist: Vec<String>,
    preferred_models: Vec<String>,
    routing_tags: Vec<String>,
    success_count: i64,
    failure_count: i64,
    consecutive_failures: i64,
    avg_latency_ms: Option<i64>,
    last_error_summary: Option<String>,
    cooldown_until: Option<String>,
    balance_status: Option<String>,
    balance_value: Option<f64>,
}

impl OperationalCandidateFact {
    pub(crate) fn station_key_id(&self) -> &StationKeyId {
        &self.station_key_id
    }

    pub(crate) fn station_id(&self) -> &StationId {
        &self.station_id
    }

    pub(crate) fn capacity_provider_family(&self) -> Option<&str> {
        self.capacity_provider_family.as_deref()
    }

    pub(crate) fn capacity_deployment_identity(&self) -> Option<&str> {
        self.capacity_deployment_identity.as_deref()
    }

    pub(crate) fn capacity_region_identity(&self) -> Option<&str> {
        self.capacity_region_identity.as_deref()
    }

    pub(crate) fn capacity_domain_revision(&self) -> Option<RecordRevision> {
        self.capacity_domain_revision
    }

    pub(crate) fn endpoint(&self) -> &EndpointFacts {
        &self.endpoint
    }

    pub(crate) fn credential(&self) -> &CredentialAvailabilityFact {
        &self.credential
    }

    pub(crate) fn schedulable(&self) -> bool {
        self.schedulable
    }

    pub(crate) fn priority(&self) -> i64 {
        self.priority
    }
    pub(crate) fn backup_only(&self) -> bool {
        self.backup_only
    }
    pub(crate) fn group_binding_id(&self) -> Option<&str> {
        self.group_binding_id.as_deref()
    }
    pub(crate) fn group_record_revision(&self) -> Option<RecordRevision> {
        self.group_record_revision
    }
    pub(crate) fn station_native_multiplier(&self) -> Option<f64> {
        self.station_native_multiplier
    }
    pub(crate) fn credit_per_cny(&self) -> Option<f64> {
        self.credit_per_cny
    }
    pub(crate) fn account_record_revision(&self) -> RecordRevision {
        self.account_record_revision
    }
    pub(crate) fn group_id_hash(&self) -> Option<&str> {
        self.group_id_hash.as_deref()
    }
    pub(crate) fn group_category(&self) -> Option<&str> {
        self.group_category.as_deref()
    }
    pub(crate) fn supports_chat_completions(&self) -> bool {
        self.supports_chat_completions
    }
    pub(crate) fn supports_responses(&self) -> bool {
        self.supports_responses
    }
    pub(crate) fn supports_stream(&self) -> bool {
        self.supports_stream
    }
    pub(crate) fn supports_tools(&self) -> bool {
        self.supports_tools
    }
    pub(crate) fn supports_vision(&self) -> bool {
        self.supports_vision
    }
    pub(crate) fn supports_reasoning(&self) -> bool {
        self.supports_reasoning
    }
    pub(crate) fn model_allowlist(&self) -> &[String] {
        &self.model_allowlist
    }
    pub(crate) fn model_blocklist(&self) -> &[String] {
        &self.model_blocklist
    }
    pub(crate) fn preferred_models(&self) -> &[String] {
        &self.preferred_models
    }
    pub(crate) fn routing_tags(&self) -> &[String] {
        &self.routing_tags
    }
    pub(crate) fn success_count(&self) -> i64 {
        self.success_count
    }
    pub(crate) fn failure_count(&self) -> i64 {
        self.failure_count
    }
    pub(crate) fn avg_latency_ms(&self) -> Option<i64> {
        self.avg_latency_ms
    }
    pub(crate) fn balance_status(&self) -> Option<&str> {
        self.balance_status.as_deref()
    }
    pub(crate) fn balance_value(&self) -> Option<f64> {
        self.balance_value
    }

    #[cfg(test)]
    pub(crate) fn for_planning_test(
        group_binding_id: Option<&str>,
        group_id_hash: Option<&str>,
        group_category: Option<&str>,
    ) -> Self {
        let station_id = StationId::new("station-a").expect("valid test station");
        Self {
            station_key_id: StationKeyId::new("key-a").expect("valid test key"),
            station_id: station_id.clone(),
            capacity_provider_family: None,
            capacity_deployment_identity: None,
            capacity_region_identity: None,
            capacity_domain_revision: None,
            endpoint: EndpointFacts::new(
                EndpointRef::new(
                    station_id,
                    EndpointId::new("primary").expect("valid endpoint"),
                    EndpointRevision::new(1).expect("valid revision"),
                ),
                SanitizedOrigin::from_endpoint_url("https://example.test")
                    .expect("valid test origin"),
                OutboundPolicyRef::new("station-default").expect("valid outbound policy"),
            ),
            credential: CredentialAvailabilityFact::new(
                true,
                RecordRevision::new(1).expect("valid record revision"),
            ),
            schedulable: true,
            priority: 0,
            backup_only: false,
            group_binding_id: group_binding_id.map(ToString::to_string),
            group_record_revision: group_binding_id.map(|_| RecordRevision::new(1).unwrap()),
            group_binding_status: group_binding_id.map(|_| "bound".to_string()),
            station_native_multiplier: Some(1.0),
            credit_per_cny: Some(1.0),
            account_record_revision: RecordRevision::new(1).unwrap(),
            group_id_hash: group_id_hash.map(ToString::to_string),
            group_category: group_category.map(ToString::to_string),
            supports_chat_completions: true,
            supports_responses: false,
            supports_stream: true,
            supports_tools: true,
            supports_vision: true,
            supports_reasoning: true,
            model_allowlist: Vec::new(),
            model_blocklist: Vec::new(),
            preferred_models: Vec::new(),
            routing_tags: Vec::new(),
            success_count: 0,
            failure_count: 0,
            consecutive_failures: 0,
            avg_latency_ms: None,
            last_error_summary: None,
            cooldown_until: None,
            balance_status: None,
            balance_value: Some(1.0),
        }
    }

    #[cfg(test)]
    pub(crate) fn set_durable_health_for_planning_test(
        &mut self,
        cooldown_until: Option<&str>,
        last_error_summary: Option<&str>,
    ) {
        self.cooldown_until = cooldown_until.map(ToString::to_string);
        self.last_error_summary = last_error_summary.map(ToString::to_string);
    }

    #[cfg(test)]
    pub(crate) fn set_multiplier_for_planning_test(
        &mut self,
        station_native_multiplier: Option<f64>,
        credit_per_cny: Option<f64>,
    ) {
        self.station_native_multiplier = station_native_multiplier;
        self.credit_per_cny = credit_per_cny;
    }

    #[cfg(test)]
    pub(crate) fn set_balance_for_planning_test(
        &mut self,
        balance_value: Option<f64>,
        balance_status: Option<&str>,
    ) {
        self.balance_value = balance_value;
        self.balance_status = balance_status.map(ToString::to_string);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettingFact {
    key: String,
    value: String,
    record_revision: RecordRevision,
}

impl SettingFact {
    #[cfg(test)]
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    #[cfg(test)]
    pub(crate) fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelAliasFact {
    alias_id: String,
    client_model: String,
    upstream_model: String,
    record_revision: RecordRevision,
}

impl ModelAliasFact {}

#[derive(Debug, Clone, PartialEq)]
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

    #[cfg(test)]
    pub(crate) fn settings_by_key(&self) -> &BTreeMap<String, SettingFact> {
        &self.settings_by_key
    }

    #[cfg(test)]
    pub(crate) fn model_aliases(&self) -> &[ModelAliasFact] {
        &self.model_aliases
    }

    #[cfg(test)]
    pub(crate) fn query_count(&self) -> usize {
        self.query_count
    }

    #[cfg(test)]
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

    let mut max_station_revision = 0;
    let mut max_key_revision = 0;
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
                capacity_provider_family: row.capacity_provider_family,
                capacity_deployment_identity: row.capacity_deployment_identity,
                capacity_region_identity: row.capacity_region_identity,
                capacity_domain_revision: row
                    .capacity_domain_revision
                    .map(RecordRevision::new)
                    .transpose()?,
                endpoint,
                credential: CredentialAvailabilityFact::new(
                    row.credential_available,
                    key_record_revision,
                ),
                schedulable: row.schedulable,
                priority: row.priority,
                backup_only: row.backup_only,
                group_binding_id: row.group_binding_id,
                group_record_revision: row
                    .group_record_revision
                    .map(RecordRevision::new)
                    .transpose()?,
                group_binding_status: row.group_binding_status,
                station_native_multiplier: row.station_native_multiplier,
                credit_per_cny: row.credit_per_cny,
                account_record_revision: RecordRevision::new(row.account_record_revision)?,
                group_id_hash: row.group_id_hash,
                group_category: row.group_category,
                supports_chat_completions: row.supports_chat_completions,
                supports_responses: row.supports_responses,
                supports_stream: row.supports_stream,
                supports_tools: row.supports_tools,
                supports_vision: row.supports_vision,
                supports_reasoning: row.supports_reasoning,
                model_allowlist: parse_json_string_list(
                    &row.model_allowlist_json,
                    "model_allowlist_json",
                )?,
                model_blocklist: parse_json_string_list(
                    &row.model_blocklist_json,
                    "model_blocklist_json",
                )?,
                preferred_models: parse_json_string_list(
                    &row.preferred_models_json,
                    "preferred_models_json",
                )?,
                routing_tags: parse_json_string_list(&row.routing_tags_json, "routing_tags_json")?,
                success_count: row.success_count.max(0),
                failure_count: row.failure_count.max(0),
                consecutive_failures: row.consecutive_failures.max(0),
                avg_latency_ms: row.avg_latency_ms.filter(|value| *value >= 0),
                last_error_summary: row.last_error_summary,
                cooldown_until: row.cooldown_until,
                balance_status: row.balance_status,
                balance_value: row.balance_value.filter(|value| value.is_finite()),
            })
        })
        .collect::<Result<Vec<_>, OperationalFactAssemblyError>>()?;

    let mut max_settings_revision = 0;
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

    let mut max_alias_revision = 0;
    let model_aliases = raw
        .model_aliases
        .into_iter()
        .map(|row| {
            let revision = RecordRevision::new(row.record_revision)?;
            max_alias_revision = max_alias_revision.max(row.record_revision);
            Ok(ModelAliasFact {
                alias_id: row.alias_id,
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

fn parse_json_string_list(
    value: &str,
    field: &'static str,
) -> Result<Vec<String>, OperationalFactAssemblyError> {
    serde_json::from_str::<Vec<String>>(value)
        .map_err(|_| OperationalFactAssemblyError::InvalidJson { field })
}
