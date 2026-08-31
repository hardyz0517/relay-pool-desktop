use serde::Deserialize;
use serde_json::Value;

use crate::application::{
    error_rate_protection::ErrorRateHistoryPageV1,
    queries::{
        operational_detail::{StationKeyOperationalDetail, StationKeyOperationalDetailInput},
        request_decision_trace::{
            RecentRouteDecisionsInput, RecentRouteDecisionsPage, RequestDecisionTrace,
        },
        routing_protection::RoutingProtectionStatus,
        routing_runtime::RoutingRuntimeOverlay,
        routing_workspace::{RoutingWorkspaceSnapshot, RoutingWorkspaceSnapshotInput},
    },
};
#[cfg(test)]
use crate::models::routing::{PricingGroupType, RoutingPolicy};
use crate::models::{
    routing::{
        ModelAlias, RouteEndpointKind, RouteSimulationInput, RouteSimulationResult,
        RoutingGroupFilter, StationKeyCapabilities, StationKeyHealth,
    },
    stations::StationEndpointHealth,
};

use super::{invalid_input, routing_mutations::RoutingPolicyConfigV1Dto, TypeDescriptor};

const MAX_TEXT_BYTES: usize = 512;
const MAX_GROUP_ID_BYTES: usize = 256;
const MAX_STATION_KEY_ID_BYTES: usize = 128;
const MAX_RATE_MULTIPLIER: f64 = 1.0e6;

pub type ModelAliasDto = ModelAlias;
pub type RouteSimulationResultDto = RouteSimulationResult;
pub type RecentRouteDecisionsPageDto = RecentRouteDecisionsPage;
pub type RequestDecisionTraceDto = RequestDecisionTrace;
pub type RoutingRuntimeOverlayDto = RoutingRuntimeOverlay;
pub type RoutingProtectionStatusDto = RoutingProtectionStatus;
pub type ErrorRateHistoryPageDto = ErrorRateHistoryPageV1;
pub type StationKeyOperationalDetailDto = StationKeyOperationalDetail;
pub type RoutingWorkspaceSnapshotDto = RoutingWorkspaceSnapshot;
pub type StationEndpointHealthDto = StationEndpointHealth;
pub type StationKeyCapabilitiesDto = StationKeyCapabilities;
pub type StationKeyHealthDto = StationKeyHealth;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingProtectionStatusInputDto {}

impl RoutingProtectionStatusInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The routing protection status payload is invalid.",
            )
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingStationKeyIdInputDto {
    pub station_key_id: String,
}

impl RoutingStationKeyIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The routing key payload is invalid.",
            )
        })?;
        let valid = !input.station_key_id.is_empty()
            && input.station_key_id.len() <= MAX_STATION_KEY_ID_BYTES
            && input.station_key_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            });
        if !valid {
            return Err(invalid_input(
                "stationKeyId",
                "invalid_id",
                "The station key ID is invalid.",
            ));
        }
        Ok(input)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestDecisionTraceInputDto {
    pub request_log_id: String,
}

impl RequestDecisionTraceInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The request decision trace payload is invalid.",
            )
        })?;
        validate_stable_id("requestLogId", &input.request_log_id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecentRouteDecisionsInputDto {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=legacy-error-rate-ipc-reference; owner=ipc/dto/routing_health_reads; remove_when=legacy error-rate diagnostics are deleted"
    )
)]
pub struct ErrorRateHistoryInputDto {
    #[serde(default)]
    pub before_ms: Option<i64>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=legacy-error-rate-ipc-reference; owner=ipc/dto/routing_health_reads; remove_when=legacy error-rate diagnostics are deleted"
    )
)]
impl ErrorRateHistoryInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The error-rate history payload is invalid.",
            )
        })?;
        if input.before_ms.is_some_and(|value| value < 0)
            || input.limit.is_some_and(|value| value == 0 || value > 200)
        {
            return Err(invalid_input(
                "history",
                "out_of_range",
                "The error-rate history range is outside the supported range.",
            ));
        }
        Ok(input)
    }
}

impl RecentRouteDecisionsInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The recent route decisions payload is invalid.",
            )
        })?;
        if input.limit.is_some_and(|limit| limit == 0 || limit > 200) {
            return Err(invalid_input(
                "limit",
                "out_of_range",
                "The recent route decisions limit is outside the supported range.",
            ));
        }
        if input.cursor.as_deref().is_some_and(|cursor| {
            cursor.len() > MAX_TEXT_BYTES || cursor.chars().any(char::is_control)
        }) {
            return Err(invalid_input(
                "cursor",
                "invalid_text",
                "The recent route decisions cursor is invalid.",
            ));
        }
        Ok(input)
    }

    pub fn into_domain(self) -> RecentRouteDecisionsInput {
        RecentRouteDecisionsInput {
            limit: self.limit,
            cursor: self.cursor,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StationKeyOperationalDetailInputDto {
    pub station_key_id: String,
}

impl StationKeyOperationalDetailInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The station key operational detail payload is invalid.",
            )
        })?;
        validate_stable_id("stationKeyId", &input.station_key_id)?;
        Ok(input)
    }

    pub fn into_domain(self) -> StationKeyOperationalDetailInput {
        StationKeyOperationalDetailInput {
            station_key_id: self.station_key_id,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingWorkspaceSnapshotInputDto {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub cursor: Option<String>,
}

impl RoutingWorkspaceSnapshotInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The routing workspace snapshot payload is invalid.",
            )
        })?;
        if input.limit.is_some_and(|limit| limit == 0 || limit > 1024) {
            return Err(invalid_input(
                "limit",
                "out_of_range",
                "The routing workspace snapshot limit is outside the supported range.",
            ));
        }
        if input.cursor.as_deref().is_some_and(|cursor| {
            cursor.len() > MAX_TEXT_BYTES || cursor.chars().any(char::is_control)
        }) {
            return Err(invalid_input(
                "cursor",
                "invalid_text",
                "The routing workspace snapshot cursor is invalid.",
            ));
        }
        Ok(input)
    }

    pub fn into_domain(self) -> RoutingWorkspaceSnapshotInput {
        RoutingWorkspaceSnapshotInput {
            limit: self.limit,
            cursor: self.cursor,
        }
    }
}

fn validate_stable_id(
    field: &'static str,
    value: &str,
) -> Result<(), crate::commands::error::CommandError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_STATION_KEY_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if !valid {
        return Err(invalid_input(
            field,
            "invalid_id",
            "The stable ID is invalid.",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouteSimulationInputDto {
    pub endpoint: RouteEndpointKind,
    pub model: Option<String>,
    pub stream: bool,
    pub uses_tools: bool,
    pub uses_vision: bool,
    pub uses_reasoning: bool,
    pub policy: Option<RoutingPolicyConfigV1Dto>,
    #[serde(default)]
    pub max_rate_multiplier: Option<f64>,
    #[serde(default)]
    pub routing_group_filter: Option<RoutingGroupFilter>,
    #[serde(default)]
    pub session_hash: Option<String>,
    #[serde(default)]
    pub previous_response_id: Option<String>,
}

impl RouteSimulationInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The route simulation payload is invalid.",
            )
        })?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_domain(self) -> Result<RouteSimulationInput, crate::commands::error::CommandError> {
        Ok(RouteSimulationInput {
            endpoint: self.endpoint,
            model: self.model,
            stream: self.stream,
            uses_tools: self.uses_tools,
            uses_vision: self.uses_vision,
            uses_reasoning: self.uses_reasoning,
            policy: self
                .policy
                .map(RoutingPolicyConfigV1Dto::into_domain)
                .transpose()?,
            max_rate_multiplier: self.max_rate_multiplier,
            routing_group_filter: self.routing_group_filter,
            session_hash: self.session_hash,
            previous_response_id: self.previous_response_id,
        })
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        validate_optional_text("model", self.model.as_deref(), MAX_TEXT_BYTES)?;
        validate_optional_text("sessionHash", self.session_hash.as_deref(), MAX_TEXT_BYTES)?;
        validate_optional_text(
            "previousResponseId",
            self.previous_response_id.as_deref(),
            MAX_TEXT_BYTES,
        )?;
        if self.max_rate_multiplier.is_some_and(|value| {
            !value.is_finite() || !(0.0..=MAX_RATE_MULTIPLIER).contains(&value)
        }) {
            return Err(invalid_input(
                "maxRateMultiplier",
                "out_of_range",
                "The rate multiplier is outside the supported range.",
            ));
        }
        if let Some(filter) = self.routing_group_filter.as_ref() {
            validate_group_filter(filter)?;
        }
        Ok(())
    }
}

fn validate_optional_text(
    field: &'static str,
    value: Option<&str>,
    max_bytes: usize,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_some_and(|value| value.len() > max_bytes || value.chars().any(char::is_control)) {
        return Err(invalid_input(
            field,
            "invalid_text",
            "The text field is invalid.",
        ));
    }
    Ok(())
}

fn validate_group_filter(
    filter: &RoutingGroupFilter,
) -> Result<(), crate::commands::error::CommandError> {
    let value = match filter {
        RoutingGroupFilter::GroupBindingId(value) | RoutingGroupFilter::GroupIdHash(value) => {
            Some(value.as_str())
        }
        RoutingGroupFilter::AllGroups
        | RoutingGroupFilter::UngroupedOnly
        | RoutingGroupFilter::GroupType(_) => None,
    };
    if value.is_some_and(|value| {
        value.is_empty() || value.len() > MAX_GROUP_ID_BYTES || value.chars().any(char::is_control)
    }) {
        return Err(invalid_input(
            "routingGroupFilter",
            "invalid_id",
            "The routing group filter is invalid.",
        ));
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
pub const ROUTING_HEALTH_READS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "RoutingHealthReadsDto",
    typescript: include_str!("routing_health_reads.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let capabilities = fixture_capabilities();
    let alias = fixture_alias();
    let health = fixture_health();
    let endpoint_health = fixture_endpoint_health();
    vec![
        serde_json::json!({"command":"get_station_key_capabilities","input":{"stationKeyId":"key-1"},"output":capabilities}),
        serde_json::json!({"command":"list_model_aliases","input":{},"output":[alias]}),
        serde_json::json!({"command":"list_station_key_health","input":{},"output":[health.clone()]}),
        serde_json::json!({"command":"list_station_endpoint_health","input":{},"output":[endpoint_health]}),
        serde_json::json!({
            "command":"list_recent_route_decisions",
            "input":{"limit":50,"cursor":null},
            "output":{
                "pageVersion":"recent_route_decisions_v1",
                "decisions":[{
                    "requestLogId":"request-log-1",
                    "requestId":"request-1",
                    "createdAt":"1700000000000",
                    "startedAt":"1700000000000",
                    "finishedAt":"1700000000100",
                    "durationMs":100,
                    "endpoint":"/v1/chat/completions",
                    "model":"fixture-model",
                    "status":"success",
                    "lifecycleStatus":"completed",
                    "stationKeyId":"key-1",
                    "stationId":"station-1",
                    "routePolicy":"cost_stable_first",
                    "routeReason":"selected",
                    "fallbackCount":0,
                    "costStatus":"estimated",
                    "estimatedTotalCost":0.01,
                    "costCurrency":"USD"
                }],
                "nextCursor":null,
                "readModelStatus":"available"
            }
        }),
        serde_json::json!({
            "command":"get_station_key_operational_detail",
            "input":{"stationKeyId":"key-1"},
            "output":{
                "detailVersion":"station_key_operational_detail_v1",
                "stationKeyId":"key-1",
                "stationId":"station-1",
                "endpointRevision":1,
                "facts":[{
                    "scope":"health",
                    "name":"station_key_health",
                    "value":"ready",
                    "source":"station_key_health.runtime_overlay",
                    "freshness":"1700000000000",
                    "reason":null
                }],
                "lazyHistoryAvailable":true,
                "readModelStatus":"available"
            }
        }),
        serde_json::json!({
            "command":"get_request_decision_trace",
            "input":{"requestLogId":"request-log-1"},
            "output":{
                "traceVersion":"request_decision_trace_v2",
                "requestLogId":"request-log-1",
                "status":"legacy_summary",
                "detailAvailability":"summary_only",
                "reason":"legacy_summary_only_before_cutover",
                "explanationKey":"legacy_summary_only_before_cutover",
                "policyRevision":null,
                "legacySummary":{
                    "routePolicy":"cost_stable_first",
                    "routeReason":"selected",
                    "stationKeyId":"key-1",
                    "stationId":"station-1",
                    "fallbackCount":0
                },
                "timeline":[],
                "planningRounds":[]
            }
        }),
        serde_json::json!({
            "command":"load_routing_workspace_snapshot",
            "input":{"limit":64,"cursor":null},
            "output":{
                "readModelVersion":"routing_workspace_read_model_v1",
                "generatedAtMs":1700000000000_i64,
                "policyConfig":{
                    "version":1,
                    "reliabilityWeight":4000,
                    "responsivenessWeight":2500,
                    "costWeight":2000,
                    "preferenceWeight":1500,
                    "maxCandidates":64,
                    "explorationShareBasisPoints":500,
                    "allowDepletedFallback":false,
                    "affinityEnabled":false,
                    "affinityTtlSeconds":300
                },
                "previewPolicyVersion":"intelligent_planner_v1",
                "maxRateMultiplier":2.0,
                "routingGroupFilter":{"group_type":"gpt"},
                "capacityMode":"snapshot_only",
                "page":{"limit":64,"returned":1,"nextCursor":null},
                "candidates":[{
                    "stationKeyId":"key-1",
                    "stationId":"station-1",
                    "stationName":"Station",
                    "keyName":"Key",
                    "endpointRevision":1,
                    "priority":10,
                    "schedulable":true,
                    "healthState":"ready",
                    "score":null,
                    "scoreDetails":null,
                    "group":{
                        "stableKey":"binding:group-1",
                        "displayName":"Group 1",
                        "available":true,
                        "reason":"bound_group"
                    },
                    "multiplier":{
                        "status":"missing",
                        "multiplier":null,
                        "selectedSource":null,
                        "ceilingRejected":false,
                        "reason":"multiplier_missing"
                    },
                    "capabilitySummary":{
                        "chatCompletions":true,
                        "responses":true,
                        "embeddings":false,
                        "stream":true,
                        "tools":false,
                        "vision":false,
                        "reasoning":false
                    },
                    "capabilityVerdicts":{
                        "protocol":"allow",
                        "model":"allow",
                        "stream":"allow",
                        "tools":"reject",
                        "vision":"reject",
                        "reasoning":"reject",
                        "rejectionSubjects":[]
                    },
                    "priceBasis":"unpriced",
                    "pricing":{
                        "basis":"unpriced",
                        "comparisonValue":null,
                        "reason":"pricing_context_missing",
                        "currency":null,
                        "unit":null,
                        "sourceChain":["pricing_projector"],
                        "observedAt":null,
                        "confidence":null
                    },
                    "balanceStatus":null,
                    "capacity":{"mode":"snapshot_only","maxConcurrency":8,"inFlight":1,"acquired":false},
                    "sourceRefs":{
                        "stationKeyId":"key-1",
                        "stationId":"station-1",
                        "endpointRevision":1,
                        "snapshotId":"snapshot-1",
                        "factVersionVector":"endpoint:1",
                        "projectorVersion":"route_candidate_projection_v1"
                    },
                    "hardRejectionCodes":[]
                }],
                "readModelStatus":"available"
            }
        }),
        serde_json::json!({
            "command":"load_routing_runtime_overlay",
            "input":{},
            "output":{
                "overlayVersion":"routing_runtime_overlay_v2",
                "sampledAtMs":1700000000000_i64,
                "revision":1,
                "candidates":[{
                    "stationKeyId":"key-1",
                    "stationId":"station-1",
                    "endpointRevision":1,
                    "inFlight":1,
                    "stationKeyInFlight":1,
                    "healthState":"ready",
                    "cooldownUntil":null
                }]
            }
        }),
        serde_json::json!({"command":"get_station_key_health","input":{"stationKeyId":"key-1"},"output":health}),
        serde_json::json!({
            "command":"simulate_route",
            "input":{
                "endpoint":"chat_completions",
                "model":"fixture-model",
                "stream":true,
                "usesTools":false,
                "usesVision":false,
                "usesReasoning":false,
                "policy":{
                    "version":1,
                    "reliabilityWeight":4000,
                    "responsivenessWeight":2500,
                    "costWeight":2000,
                    "preferenceWeight":1500,
                    "maxCandidates":64,
                    "explorationShareBasisPoints":500,
                    "allowDepletedFallback":false,
                    "affinityEnabled":false,
                    "affinityTtlSeconds":300
                },
                "maxRateMultiplier":2.0,
                "routingGroupFilter":{"group_type":"gpt"},
                "sessionHash":"session-1",
                "previousResponseId":null
            },
            "output":fixture_simulation_result()
        }),
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
        client_model: "fixture-model".into(),
        upstream_model: "fixture-upstream-model".into(),
        enabled: true,
        note: None,
        created_at: "1700000000000".into(),
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_health() -> StationKeyHealth {
    StationKeyHealth {
        station_key_id: "key-1".into(),
        last_success_at: Some("1700000000000".into()),
        last_failure_at: None,
        consecutive_failures: 0,
        success_count: 1,
        failure_count: 0,
        avg_latency_ms: Some(120),
        last_error_summary: None,
        cooldown_until: None,
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_endpoint_health() -> StationEndpointHealth {
    StationEndpointHealth {
        station_id: "station-1".into(),
        endpoint_revision: 1,
        status: "success".into(),
        latency_ms: Some(120),
        checked_at: Some("1700000000000".into()),
        error_summary: None,
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_simulation_result() -> RouteSimulationResult {
    RouteSimulationResult {
        preview_policy_version: "intelligent_planner_v1".into(),
        capacity_mode: "snapshot_only".into(),
        selected_capacity_acquired: false,
        selected_station_key_id: Some("key-1".into()),
        selected_station_id: Some("station-1".into()),
        mapped_model: Some("fixture-model".into()),
        policy: RoutingPolicy::CostStableFirst,
        max_rate_multiplier: Some(2.0),
        routing_group_filter: RoutingGroupFilter::GroupType(PricingGroupType::Gpt),
        planner_error_code: None,
        candidates: Vec::new(),
        message: "Route simulation completed.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    fn valid_input() -> Value {
        serde_json::json!({
            "endpoint":"chat_completions",
            "model":"fixture-model",
            "stream":true,
            "usesTools":false,
            "usesVision":false,
            "usesReasoning":false,
            "policy":{
                "version":1,
                "reliabilityWeight":4000,
                "responsivenessWeight":2500,
                "costWeight":2000,
                "preferenceWeight":1500,
                "maxCandidates":64,
                "explorationShareBasisPoints":500,
                "allowDepletedFallback":false,
                "affinityEnabled":false,
                "affinityTtlSeconds":300
            },
            "maxRateMultiplier":2.0,
            "routingGroupFilter":{"group_type":"gpt"},
            "sessionHash":"session-1",
            "previousResponseId":null
        })
    }

    #[test]
    fn rejects_unknown_fields_invalid_filters_and_out_of_range_multipliers() {
        let mut unknown = valid_input();
        unknown["unexpected"] = serde_json::json!(true);
        let mut invalid_filter = valid_input();
        invalid_filter["routingGroupFilter"] = serde_json::json!({"unknown":"value"});
        let mut invalid_multiplier = valid_input();
        invalid_multiplier["maxRateMultiplier"] = serde_json::json!(-1.0);

        for value in [unknown, invalid_filter, invalid_multiplier] {
            let error = RouteSimulationInputDto::parse(value).expect_err("invalid route input");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }

    #[test]
    fn rejects_oversized_or_control_character_text() {
        let mut oversized = valid_input();
        oversized["model"] = serde_json::json!("x".repeat(MAX_TEXT_BYTES + 1));
        let mut control = valid_input();
        control["sessionHash"] = serde_json::json!("session\nvalue");

        for value in [oversized, control] {
            let error = RouteSimulationInputDto::parse(value).expect_err("invalid route text");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }

    #[test]
    fn accepts_null_policy_for_current_routing_settings() {
        let mut input = valid_input();
        input["policy"] = Value::Null;

        assert!(RouteSimulationInputDto::parse(input).is_ok());
    }

    #[test]
    fn station_key_id_input_rejects_unknown_fields_and_invalid_ids() {
        for value in [
            serde_json::json!({"stationKeyId":"bad id"}),
            serde_json::json!({"stationKeyId":"key-1","unexpected":true}),
        ] {
            let error =
                RoutingStationKeyIdInputDto::parse(value).expect_err("invalid station key ID");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }

    #[test]
    fn routing_protection_status_input_keeps_empty_input_compatibility() {
        RoutingProtectionStatusInputDto::parse(serde_json::json!({}))
            .expect("empty protection input remains valid");
    }

    #[test]
    fn routing_protection_status_input_rejects_unknown_fields() {
        let error = RoutingProtectionStatusInputDto::parse(serde_json::json!({
            "model":"gpt-5-mini"
        }))
        .expect_err("retired capacity-domain query input is rejected");
        assert_eq!(error.code, CommandErrorCode::InvalidInput);
    }
}
