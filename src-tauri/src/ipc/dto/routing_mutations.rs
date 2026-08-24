use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use crate::models::routing::{ModelAlias, StationKeyCapabilities};
use crate::models::{routing::UpdateStationKeyCapabilitiesInput, stations::EndpointPingResult};

use crate::models::routing::RoutingGroupFilter;
use crate::models::routing_policy::{
    ProtectionProfileConfigV2, RetryFailoverPolicyV2, RoutingPolicyConfigV1, RoutingPolicyConfigV2,
    RoutingPolicyDocumentV2, TimeoutPolicyV2,
};

use super::{invalid_input, TypeDescriptor};

const MAX_ID_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 256;
const MAX_TAG_BYTES: usize = 128;
#[cfg(test)]
const MAX_NOTE_BYTES: usize = 4_096;
const MAX_MODEL_LIST_ITEMS: usize = 256;
const MAX_ROUTING_TAGS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingPolicyConfigV1Dto {
    pub version: u16,
    pub reliability_weight: u16,
    pub responsiveness_weight: u16,
    pub cost_weight: u16,
    pub preference_weight: u16,
    pub max_candidates: u16,
    pub exploration_share_basis_points: u16,
    pub allow_depleted_fallback: bool,
    pub affinity_enabled: bool,
    pub affinity_ttl_seconds: u32,
    #[serde(default)]
    pub max_rate_multiplier: Option<f64>,
    #[serde(default)]
    pub routing_group_filter: RoutingGroupFilter,
    #[serde(default = "default_outbound_proxy_mode")]
    pub outbound_proxy_mode: String,
    #[serde(default)]
    pub outbound_proxy_url: Option<String>,
}

fn default_outbound_proxy_mode() -> String {
    "inherit".to_string()
}

fn deserialize_required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

impl From<RoutingPolicyConfigV1> for RoutingPolicyConfigV1Dto {
    fn from(value: RoutingPolicyConfigV1) -> Self {
        Self {
            version: value.version,
            reliability_weight: value.reliability_weight,
            responsiveness_weight: value.responsiveness_weight,
            cost_weight: value.cost_weight,
            preference_weight: value.preference_weight,
            max_candidates: value.max_candidates,
            exploration_share_basis_points: value.exploration_share_basis_points,
            allow_depleted_fallback: value.allow_depleted_fallback,
            affinity_enabled: value.affinity_enabled,
            affinity_ttl_seconds: value.affinity_ttl_seconds,
            max_rate_multiplier: value.max_rate_multiplier,
            routing_group_filter: value.routing_group_filter,
            outbound_proxy_mode: value.outbound_proxy_mode,
            outbound_proxy_url: value.outbound_proxy_url,
        }
    }
}

impl RoutingPolicyConfigV1Dto {
    pub fn into_domain(
        self,
    ) -> Result<RoutingPolicyConfigV1, crate::commands::error::CommandError> {
        let config = RoutingPolicyConfigV1 {
            version: self.version,
            reliability_weight: self.reliability_weight,
            responsiveness_weight: self.responsiveness_weight,
            cost_weight: self.cost_weight,
            preference_weight: self.preference_weight,
            max_candidates: self.max_candidates,
            exploration_share_basis_points: self.exploration_share_basis_points,
            allow_depleted_fallback: self.allow_depleted_fallback,
            affinity_enabled: self.affinity_enabled,
            affinity_ttl_seconds: self.affinity_ttl_seconds,
            max_rate_multiplier: self.max_rate_multiplier,
            routing_group_filter: self.routing_group_filter,
            outbound_proxy_mode: self.outbound_proxy_mode,
            outbound_proxy_url: self.outbound_proxy_url,
        };
        config.validate().map_err(|_| {
            invalid_input("config", "invalid_policy", "The routing policy is invalid.")
        })?;
        Ok(config)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetryFailoverPolicyV2Dto {
    pub version: u16,
    pub max_total_attempts: u16,
    pub max_same_target_capacity_retries: u16,
    pub capacity_retry_wait_budget_seconds: f64,
    pub allow_cross_capacity_domain_fallback: bool,
}

impl From<RetryFailoverPolicyV2> for RetryFailoverPolicyV2Dto {
    fn from(value: RetryFailoverPolicyV2) -> Self {
        Self {
            version: value.version,
            max_total_attempts: value.max_total_attempts,
            max_same_target_capacity_retries: value.max_same_target_capacity_retries,
            capacity_retry_wait_budget_seconds: value.capacity_retry_wait_budget_seconds,
            allow_cross_capacity_domain_fallback: value.allow_cross_capacity_domain_fallback,
        }
    }
}

impl RetryFailoverPolicyV2Dto {
    fn into_domain(self) -> Result<RetryFailoverPolicyV2, crate::commands::error::CommandError> {
        let policy = RetryFailoverPolicyV2 {
            version: self.version,
            max_total_attempts: self.max_total_attempts,
            max_same_target_capacity_retries: self.max_same_target_capacity_retries,
            capacity_retry_wait_budget_seconds: self.capacity_retry_wait_budget_seconds,
            allow_cross_capacity_domain_fallback: self.allow_cross_capacity_domain_fallback,
        };
        policy
            .validate()
            .map_err(|error| invalid_input(error.field, error.code, error.message_key))?;
        Ok(policy)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProtectionProfileConfigV2Dto {
    pub version: u16,
    pub enabled: bool,
    pub window_max_samples: u16,
    pub window_seconds: f64,
    pub min_samples: u16,
    pub failure_threshold_percent: u8,
    pub half_open_successes_to_close: u8,
}

impl From<ProtectionProfileConfigV2> for ProtectionProfileConfigV2Dto {
    fn from(value: ProtectionProfileConfigV2) -> Self {
        Self {
            version: value.version,
            enabled: value.enabled,
            window_max_samples: value.window_max_samples,
            window_seconds: value.window_seconds,
            min_samples: value.min_samples,
            failure_threshold_percent: value.failure_threshold_percent,
            half_open_successes_to_close: value.half_open_successes_to_close,
        }
    }
}

impl ProtectionProfileConfigV2Dto {
    fn into_domain(
        self,
    ) -> Result<ProtectionProfileConfigV2, crate::commands::error::CommandError> {
        let profile = ProtectionProfileConfigV2 {
            version: self.version,
            enabled: self.enabled,
            window_max_samples: self.window_max_samples,
            window_seconds: self.window_seconds,
            min_samples: self.min_samples,
            failure_threshold_percent: self.failure_threshold_percent,
            half_open_successes_to_close: self.half_open_successes_to_close,
        };
        profile
            .validate()
            .map_err(|error| invalid_input(error.field, error.code, error.message_key))?;
        Ok(profile)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeoutPolicyV2Dto {
    pub version: u16,
    pub connect_seconds: f64,
    pub first_byte_seconds: f64,
    pub precommit_seconds: f64,
    pub buffered_execution_seconds: f64,
    pub stream_idle_seconds: f64,
}

impl From<TimeoutPolicyV2> for TimeoutPolicyV2Dto {
    fn from(value: TimeoutPolicyV2) -> Self {
        Self {
            version: value.version,
            connect_seconds: value.connect_seconds,
            first_byte_seconds: value.first_byte_seconds,
            precommit_seconds: value.precommit_seconds,
            buffered_execution_seconds: value.buffered_execution_seconds,
            stream_idle_seconds: value.stream_idle_seconds,
        }
    }
}

impl TimeoutPolicyV2Dto {
    fn into_domain(self) -> Result<TimeoutPolicyV2, crate::commands::error::CommandError> {
        let policy = TimeoutPolicyV2 {
            version: self.version,
            connect_seconds: self.connect_seconds,
            first_byte_seconds: self.first_byte_seconds,
            precommit_seconds: self.precommit_seconds,
            buffered_execution_seconds: self.buffered_execution_seconds,
            stream_idle_seconds: self.stream_idle_seconds,
        };
        policy
            .validate()
            .map_err(|error| invalid_input(error.field, error.code, error.message_key))?;
        Ok(policy)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingPolicyConfigV2Dto {
    pub version: u16,
    pub reliability_weight: u16,
    pub responsiveness_weight: u16,
    pub cost_weight: u16,
    pub preference_weight: u16,
    pub max_candidates: u16,
    pub exploration_share_basis_points: u16,
    pub allow_depleted_fallback: bool,
    pub affinity_enabled: bool,
    pub affinity_ttl_seconds: u32,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub max_rate_multiplier: Option<f64>,
    pub routing_group_filter: RoutingGroupFilter,
    pub outbound_proxy_mode: String,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub outbound_proxy_url: Option<String>,
    pub retry_failover: RetryFailoverPolicyV2Dto,
    pub protection_profile: ProtectionProfileConfigV2Dto,
    pub timeout_policy: TimeoutPolicyV2Dto,
}

impl From<RoutingPolicyConfigV2> for RoutingPolicyConfigV2Dto {
    fn from(value: RoutingPolicyConfigV2) -> Self {
        Self {
            version: value.version,
            reliability_weight: value.reliability_weight,
            responsiveness_weight: value.responsiveness_weight,
            cost_weight: value.cost_weight,
            preference_weight: value.preference_weight,
            max_candidates: value.max_candidates,
            exploration_share_basis_points: value.exploration_share_basis_points,
            allow_depleted_fallback: value.allow_depleted_fallback,
            affinity_enabled: value.affinity_enabled,
            affinity_ttl_seconds: value.affinity_ttl_seconds,
            max_rate_multiplier: value.max_rate_multiplier,
            routing_group_filter: value.routing_group_filter,
            outbound_proxy_mode: value.outbound_proxy_mode,
            outbound_proxy_url: value.outbound_proxy_url,
            retry_failover: value.retry_failover.into(),
            protection_profile: value.protection_profile.into(),
            timeout_policy: value.timeout_policy.into(),
        }
    }
}

impl RoutingPolicyConfigV2Dto {
    pub fn into_domain(
        self,
    ) -> Result<RoutingPolicyConfigV2, crate::commands::error::CommandError> {
        let config = RoutingPolicyConfigV2 {
            version: self.version,
            reliability_weight: self.reliability_weight,
            responsiveness_weight: self.responsiveness_weight,
            cost_weight: self.cost_weight,
            preference_weight: self.preference_weight,
            max_candidates: self.max_candidates,
            exploration_share_basis_points: self.exploration_share_basis_points,
            allow_depleted_fallback: self.allow_depleted_fallback,
            affinity_enabled: self.affinity_enabled,
            affinity_ttl_seconds: self.affinity_ttl_seconds,
            max_rate_multiplier: self.max_rate_multiplier,
            routing_group_filter: self.routing_group_filter,
            outbound_proxy_mode: self.outbound_proxy_mode,
            outbound_proxy_url: self.outbound_proxy_url,
            retry_failover: self.retry_failover.into_domain()?,
            protection_profile: self.protection_profile.into_domain()?,
            timeout_policy: self.timeout_policy.into_domain()?,
        };
        config
            .validate()
            .map_err(|error| invalid_input(error.field, error.code, error.message_key))?;
        Ok(config)
    }
}

/// Complete routing-policy document envelope. `baseRevision` is the only
/// optimistic-concurrency field; source provenance is attached by the command
/// owner and is intentionally absent from this consumer DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyRoutingPolicyDocumentInputDto {
    pub format_version: u16,
    pub base_revision: u64,
    pub policy: RoutingPolicyConfigV2Dto,
}

impl ApplyRoutingPolicyDocumentInputDto {
    /// Parse the already-materialized Tauri IPC value and validate its shape
    /// and domain constraints. At this boundary serde has already converted
    /// the incoming JSON object into `Value`, so the original token stream is
    /// unavailable and duplicate raw object keys cannot be detected here.
    /// Raw/file documents must use `services::policy_documents::decode_strict_json`
    /// when duplicate-key rejection is required.
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_domain(
        self,
    ) -> Result<RoutingPolicyDocumentV2, crate::commands::error::CommandError> {
        let document = RoutingPolicyDocumentV2 {
            format_version: self.format_version,
            base_revision: self.base_revision,
            policy: self.policy.into_domain()?,
        };
        document
            .validate()
            .map_err(|error| invalid_input(error.field, error.code, error.message_key))?;
        Ok(document)
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        self.clone().into_domain().map(|_| ())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingPolicySnapshotDto {
    pub config: RoutingPolicyConfigV2Dto,
    pub revision: u64,
    pub policy_version: String,
    pub system_version: String,
    pub status: String,
    pub updated_at_ms: i64,
    pub document_sync: Option<RoutingDocumentSyncDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingDocumentSyncDto {
    pub state: String,
    pub desired_revision: u64,
    pub materialized_revision: Option<u64>,
    pub last_error_code: Option<String>,
    pub retry_count: u32,
    pub updated_at_ms: i64,
}

impl From<crate::persistence::stores::document_sync_store::StoredDocumentSync>
    for RoutingDocumentSyncDto
{
    fn from(value: crate::persistence::stores::document_sync_store::StoredDocumentSync) -> Self {
        Self {
            state: value.state.as_str().to_string(),
            desired_revision: value.desired_revision,
            materialized_revision: value.materialized_revision,
            last_error_code: value.last_error_code,
            retry_count: value.retry_count,
            updated_at_ms: value.updated_at_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EndpointPingStatusDto {
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointPingResultDto {
    pub station_id: String,
    pub ok: bool,
    pub status: EndpointPingStatusDto,
    pub latency_ms: Option<i64>,
    pub checked_at: String,
    pub error_summary: Option<String>,
}

impl TryFrom<EndpointPingResult> for EndpointPingResultDto {
    type Error = crate::commands::error::CommandError;

    fn try_from(value: EndpointPingResult) -> Result<Self, Self::Error> {
        let status = match (value.status.as_str(), value.ok) {
            ("success", true) => EndpointPingStatusDto::Success,
            ("failed", false) => EndpointPingStatusDto::Failed,
            _ => return Err(crate::commands::error::CommandError::internal(None)),
        };
        Ok(Self {
            station_id: value.station_id,
            ok: value.ok,
            status,
            latency_ms: value.latency_ms,
            checked_at: value.checked_at,
            error_summary: value.error_summary,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[expect(
    dead_code,
    reason = "contract=ipc-dto-type-descriptor; owner=ipc; remove_when=legacy alias mutation commands and generated descriptors are retired"
)]
pub struct DeleteModelAliasInputDto {
    pub id: String,
}

#[cfg(test)]
impl DeleteModelAliasInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("id", &input.id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateStationKeyCapabilitiesInputDto {
    pub station_key_id: String,
    pub supports_chat_completions: bool,
    pub supports_responses: bool,
    pub supports_embeddings: bool,
    pub supports_stream: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_reasoning: bool,
    pub model_allowlist: Vec<String>,
    pub model_blocklist: Vec<String>,
    pub preferred_models: Vec<String>,
    pub only_use_as_backup: bool,
    pub routing_tags: Vec<String>,
}

impl UpdateStationKeyCapabilitiesInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("stationKeyId", &input.station_key_id)?;
        validate_unique_text_list(
            "modelAllowlist",
            &input.model_allowlist,
            MAX_MODEL_LIST_ITEMS,
            MAX_MODEL_BYTES,
        )?;
        validate_unique_text_list(
            "modelBlocklist",
            &input.model_blocklist,
            MAX_MODEL_LIST_ITEMS,
            MAX_MODEL_BYTES,
        )?;
        validate_unique_text_list(
            "preferredModels",
            &input.preferred_models,
            MAX_MODEL_LIST_ITEMS,
            MAX_MODEL_BYTES,
        )?;
        validate_unique_text_list(
            "routingTags",
            &input.routing_tags,
            MAX_ROUTING_TAGS,
            MAX_TAG_BYTES,
        )?;
        Ok(input)
    }

    pub fn into_domain(self) -> UpdateStationKeyCapabilitiesInput {
        UpdateStationKeyCapabilitiesInput {
            station_key_id: self.station_key_id,
            supports_chat_completions: self.supports_chat_completions,
            supports_responses: self.supports_responses,
            supports_embeddings: self.supports_embeddings,
            supports_stream: self.supports_stream,
            supports_tools: self.supports_tools,
            supports_vision: self.supports_vision,
            supports_reasoning: self.supports_reasoning,
            model_allowlist: self.model_allowlist,
            model_blocklist: self.model_blocklist,
            preferred_models: self.preferred_models,
            only_use_as_backup: self.only_use_as_backup,
            routing_tags: self.routing_tags,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[expect(
    dead_code,
    reason = "contract=ipc-dto-type-descriptor; owner=ipc; remove_when=legacy alias mutation commands and generated descriptors are retired"
)]
pub struct UpsertModelAliasInputDto {
    pub id: Option<String>,
    pub client_model: String,
    pub upstream_model: String,
    pub enabled: bool,
    pub note: Option<String>,
}

#[cfg(test)]
impl UpsertModelAliasInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        if let Some(id) = input.id.as_deref() {
            validate_id("id", id)?;
        }
        validate_text("clientModel", &input.client_model, MAX_MODEL_BYTES, false)?;
        validate_text(
            "upstreamModel",
            &input.upstream_model,
            MAX_MODEL_BYTES,
            false,
        )?;
        if let Some(note) = input.note.as_deref() {
            validate_text("note", note, MAX_NOTE_BYTES, true)?;
        }
        Ok(input)
    }
}

fn parse_value<T: for<'de> Deserialize<'de>>(
    value: Value,
) -> Result<T, crate::commands::error::CommandError> {
    serde_json::from_value(value).map_err(|_| {
        invalid_input(
            "input",
            "invalid_shape",
            "The routing mutation payload is invalid.",
        )
    })
}

fn validate_id(
    field: &'static str,
    value: &str,
) -> Result<(), crate::commands::error::CommandError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if !valid {
        return Err(invalid_input(
            field,
            "invalid_id",
            "The identifier is invalid.",
        ));
    }
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
    allow_empty: bool,
) -> Result<(), crate::commands::error::CommandError> {
    if (!allow_empty && value.trim().is_empty())
        || value.len() > max_bytes
        || value.chars().any(char::is_control)
    {
        return Err(invalid_input(
            field,
            "invalid_text",
            "The text value is invalid.",
        ));
    }
    Ok(())
}

fn validate_unique_text_list(
    field: &'static str,
    values: &[String],
    max_items: usize,
    max_bytes: usize,
) -> Result<(), crate::commands::error::CommandError> {
    if values.len() > max_items {
        return Err(invalid_input(
            field,
            "too_many_items",
            "The list contains too many items.",
        ));
    }
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        validate_text(field, value, max_bytes, false)?;
        if !seen.insert(value.as_str()) {
            return Err(invalid_input(
                field,
                "duplicate_item",
                "The list contains duplicate items.",
            ));
        }
    }
    Ok(())
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-dto-type-descriptor; owner=ipc; remove_when=descriptor is registered in production binding export"
    )
)]
pub const ROUTING_MUTATIONS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "RoutingMutationsDto",
    typescript: include_str!("routing_mutations.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let ping_input = super::station_keys::StationIdInputDto::parse(serde_json::json!({
        "stationId":"station-1"
    }))
    .expect("endpoint ping fixture input");
    let capabilities_input = UpdateStationKeyCapabilitiesInputDto::parse(serde_json::json!({
        "stationKeyId":"key-1",
        "supportsChatCompletions":true,
        "supportsResponses":true,
        "supportsEmbeddings":false,
        "supportsStream":true,
        "supportsTools":false,
        "supportsVision":false,
        "supportsReasoning":false,
        "modelAllowlist":["fixture-model"],
        "modelBlocklist":[],
        "preferredModels":["fixture-model"],
        "onlyUseAsBackup":false,
        "routingTags":["fixture"]
    }))
    .expect("capabilities fixture input");
    let alias_input = UpsertModelAliasInputDto::parse(serde_json::json!({
        "id":null,
        "clientModel":"client-model",
        "upstreamModel":"upstream-model",
        "enabled":true,
        "note":null
    }))
    .expect("alias fixture input");
    let delete_input = DeleteModelAliasInputDto::parse(serde_json::json!({"id":"alias-1"}))
        .expect("delete alias fixture input");

    vec![
        serde_json::json!({
            "command":"ping_station_endpoint",
            "input":ping_input,
            "output":EndpointPingResultDto {
                station_id:"station-1".into(),
                ok:true,
                status:EndpointPingStatusDto::Success,
                latency_ms:Some(20),
                checked_at:"1700000000000".into(),
                error_summary:None,
            }
        }),
        serde_json::json!({
            "command":"update_station_key_capabilities",
            "input":capabilities_input,
            "output":fixture_capabilities()
        }),
        serde_json::json!({
            "command":"upsert_model_alias",
            "input":alias_input,
            "output":fixture_alias()
        }),
        serde_json::json!({"command":"delete_model_alias","input":delete_input,"output":null}),
    ]
}

#[cfg(test)]
fn fixture_capabilities() -> StationKeyCapabilities {
    StationKeyCapabilities {
        station_key_id: "key-1".into(),
        supports_chat_completions: true,
        supports_responses: true,
        supports_embeddings: false,
        supports_stream: true,
        supports_tools: false,
        supports_vision: false,
        supports_reasoning: false,
        model_allowlist: vec!["fixture-model".into()],
        model_blocklist: Vec::new(),
        preferred_models: vec!["fixture-model".into()],
        only_use_as_backup: false,
        routing_tags: vec!["fixture".into()],
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_alias() -> ModelAlias {
    ModelAlias {
        id: "alias-1".into(),
        client_model: "client-model".into(),
        upstream_model: "upstream-model".into(),
        enabled: true,
        note: None,
        created_at: "1700000000000".into(),
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;
    use crate::models::routing_policy::RoutingPolicyConfigV2;

    #[test]
    fn endpoint_ping_output_rejects_open_status_values() {
        for (status, ok) in [("unknown", false), ("success", false), ("failed", true)] {
            let error = EndpointPingResultDto::try_from(EndpointPingResult {
                station_id: "station-1".into(),
                ok,
                status: status.into(),
                latency_ms: None,
                checked_at: "1700000000000".into(),
                error_summary: None,
            })
            .expect_err("open or inconsistent output status must fail closed");
            assert_eq!(error.code, CommandErrorCode::Internal);
        }
    }

    #[test]
    fn rejects_unknown_fields_invalid_ids_and_malformed_alias_text() {
        for value in [
            serde_json::json!({"id":"bad id"}),
            serde_json::json!({"id":"alias-1","unexpected":true}),
        ] {
            let error = DeleteModelAliasInputDto::parse(value).expect_err("invalid delete input");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }

        for value in [
            serde_json::json!({"id":null,"clientModel":"","upstreamModel":"upstream","enabled":true,"note":null}),
            serde_json::json!({"id":null,"clientModel":"client","upstreamModel":"upstream\n","enabled":true,"note":null}),
            serde_json::json!({"id":null,"clientModel":"client","upstreamModel":"upstream","enabled":true,"note":null,"unexpected":true}),
        ] {
            let error = UpsertModelAliasInputDto::parse(value).expect_err("invalid alias input");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }

    #[test]
    fn v2_policy_input_is_strict_at_the_public_ipc_boundary() {
        let dto = RoutingPolicyConfigV2Dto::from(RoutingPolicyConfigV2::default());
        let complete = serde_json::to_value(dto).expect("complete V2 DTO");
        for field in [
            "reliabilityWeight",
            "responsivenessWeight",
            "costWeight",
            "preferenceWeight",
            "maxCandidates",
            "explorationShareBasisPoints",
            "allowDepletedFallback",
            "affinityEnabled",
            "affinityTtlSeconds",
            "maxRateMultiplier",
            "routingGroupFilter",
            "outboundProxyMode",
            "outboundProxyUrl",
            "retryFailover",
            "protectionProfile",
            "timeoutPolicy",
        ] {
            let mut missing = complete.clone();
            missing
                .as_object_mut()
                .expect("V2 DTO object")
                .remove(field);
            let error = ApplyRoutingPolicyDocumentInputDto::parse(serde_json::json!({
                "formatVersion": 1,
                "baseRevision": 1,
                "policy": missing,
            }))
            .expect_err("missing V2 DTO field must fail closed");
            assert_eq!(error.code, CommandErrorCode::InvalidInput, "field {field}");
        }

        let mut unknown = complete.clone();
        unknown["futureField"] = Value::Bool(true);
        let error = ApplyRoutingPolicyDocumentInputDto::parse(serde_json::json!({
            "formatVersion": 1,
            "baseRevision": 1,
            "policy": unknown,
        }))
        .expect_err("unknown V2 DTO field must fail closed");
        assert_eq!(error.code, CommandErrorCode::InvalidInput);

        for (field, value) in [
            ("formatVersion", Value::from(2_u16)),
            ("baseRevision", Value::from(0_u16)),
        ] {
            let mut document = serde_json::json!({
                "formatVersion": 1,
                "baseRevision": 1,
                "policy": complete.clone()
            });
            document[field] = value;
            let error = ApplyRoutingPolicyDocumentInputDto::parse(document)
                .expect_err("invalid V2 document envelope must fail closed");
            assert_eq!(error.code, CommandErrorCode::InvalidInput, "field {field}");
        }

        let mut invalid_retry = complete;
        invalid_retry["retryFailover"]["maxTotalAttempts"] = Value::from(0_u16);
        let error = ApplyRoutingPolicyDocumentInputDto::parse(serde_json::json!({
            "formatVersion": 1,
            "baseRevision": 1,
            "policy": invalid_retry,
        }))
        .expect_err("invalid retry value must fail closed");
        assert_eq!(error.code, CommandErrorCode::InvalidInput);

        let complete = serde_json::to_value(RoutingPolicyConfigV2Dto::from(
            RoutingPolicyConfigV2::default(),
        ))
        .expect("complete V2 DTO");
        for field in [
            "version",
            "enabled",
            "windowMaxSamples",
            "windowSeconds",
            "minSamples",
            "failureThresholdPercent",
            "halfOpenSuccessesToClose",
        ] {
            let mut missing = complete.clone();
            missing["protectionProfile"]
                .as_object_mut()
                .expect("protection profile object")
                .remove(field);
            let error = ApplyRoutingPolicyDocumentInputDto::parse(serde_json::json!({
                "formatVersion": 1,
                "baseRevision": 1,
                "policy": missing,
            }))
            .expect_err("missing protection profile field must fail closed");
            assert_eq!(error.code, CommandErrorCode::InvalidInput, "field {field}");
        }

        let mut unknown_profile = complete;
        unknown_profile["protectionProfile"]["futureField"] = Value::Bool(true);
        let error = ApplyRoutingPolicyDocumentInputDto::parse(serde_json::json!({
            "formatVersion": 1,
            "baseRevision": 1,
            "policy": unknown_profile,
        }))
        .expect_err("unknown protection profile field must fail closed");
        assert_eq!(error.code, CommandErrorCode::InvalidInput);

        let complete = serde_json::to_value(RoutingPolicyConfigV2Dto::from(
            RoutingPolicyConfigV2::default(),
        ))
        .expect("complete V2 DTO");
        for field in [
            "version",
            "connectSeconds",
            "firstByteSeconds",
            "precommitSeconds",
            "bufferedExecutionSeconds",
            "streamIdleSeconds",
        ] {
            let mut missing = complete.clone();
            missing["timeoutPolicy"]
                .as_object_mut()
                .expect("timeout policy object")
                .remove(field);
            let error = ApplyRoutingPolicyDocumentInputDto::parse(serde_json::json!({
                "formatVersion": 1,
                "baseRevision": 1,
                "policy": missing,
            }))
            .expect_err("missing timeout policy field must fail closed");
            assert_eq!(error.code, CommandErrorCode::InvalidInput, "field {field}");
        }
    }

    #[test]
    fn rejects_oversized_or_duplicate_capability_lists() {
        let base = serde_json::json!({
            "stationKeyId":"key-1",
            "supportsChatCompletions":true,
            "supportsResponses":true,
            "supportsEmbeddings":false,
            "supportsStream":true,
            "supportsTools":false,
            "supportsVision":false,
            "supportsReasoning":false,
            "modelAllowlist":[],
            "modelBlocklist":[],
            "preferredModels":[],
            "onlyUseAsBackup":false,
            "routingTags":[]
        });
        for value in [
            {
                let mut value = base.clone();
                value["modelAllowlist"] = serde_json::json!(["same", "same"]);
                value
            },
            {
                let mut value = base.clone();
                value["routingTags"] = serde_json::json!(["bad\ntag"]);
                value
            },
            {
                let mut value = base;
                value["unexpected"] = serde_json::json!(true);
                value
            },
        ] {
            let error = UpdateStationKeyCapabilitiesInputDto::parse(value)
                .expect_err("invalid capabilities input");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }
}
