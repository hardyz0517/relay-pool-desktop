use serde::{Deserialize, Serialize};

use crate::{
    application::operational_facts::candidate_projector::RouteCandidateProjection,
    models::routing::{
        RouteEndpointKind, RoutingGroupFilter, RoutingPolicy, RuntimeRoutingCandidate,
    },
};

pub(crate) const ROUTING_WORKSPACE_READ_MODEL_VERSION: &str = "routing_workspace_read_model_v1";
pub(crate) const ROUTING_PREVIEW_POLICY_VERSION: &str = "hierarchical_v1_preview";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoutingCapacityReadMode {
    SnapshotOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingWorkspaceSnapshotInput {
    pub(crate) limit: Option<usize>,
    pub(crate) cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingWorkspaceSnapshot {
    pub(crate) read_model_version: &'static str,
    pub(crate) generated_at_ms: i64,
    pub(crate) production_policy: RoutingPolicy,
    pub(crate) preview_policy_version: &'static str,
    pub(crate) max_rate_multiplier: Option<f64>,
    pub(crate) routing_group_filter: RoutingGroupFilter,
    pub(crate) capacity_mode: RoutingCapacityReadMode,
    pub(crate) page: RoutingReadPage,
    pub(crate) candidates: Vec<RoutingWorkspaceCandidate>,
    pub(crate) read_model_status: RoutingReadModelStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingReadPage {
    pub(crate) limit: usize,
    pub(crate) returned: usize,
    pub(crate) next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoutingReadModelStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingWorkspaceCandidate {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) station_name: String,
    pub(crate) key_name: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) priority: i64,
    pub(crate) schedulable: bool,
    pub(crate) health_state: String,
    pub(crate) capability_summary: RoutingCapabilitySummary,
    pub(crate) price_basis: String,
    pub(crate) balance_status: Option<String>,
    pub(crate) capacity: RoutingCandidateCapacitySnapshot,
    pub(crate) source_refs: RoutingCandidateSourceRefs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCapabilitySummary {
    pub(crate) chat_completions: bool,
    pub(crate) responses: bool,
    pub(crate) embeddings: bool,
    pub(crate) stream: bool,
    pub(crate) tools: bool,
    pub(crate) vision: bool,
    pub(crate) reasoning: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateCapacitySnapshot {
    pub(crate) mode: RoutingCapacityReadMode,
    pub(crate) max_concurrency: i64,
    pub(crate) in_flight: Option<i64>,
    pub(crate) acquired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateSourceRefs {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
}

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "preview simulation DTO is exercised by Task 9 integration tests before UI cutover"
    )
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutePreviewSimulationInput {
    pub(crate) endpoint: RouteEndpointKind,
    pub(crate) model: Option<String>,
    pub(crate) stream: bool,
}

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "preview simulation DTO is exercised by Task 9 integration tests before UI cutover"
    )
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutePreviewSimulation {
    pub(crate) preview_policy_version: &'static str,
    pub(crate) production_policy: RoutingPolicy,
    pub(crate) capacity_mode: RoutingCapacityReadMode,
    pub(crate) selected_station_key_id: Option<String>,
    pub(crate) selected_station_id: Option<String>,
    pub(crate) candidate_count: usize,
    pub(crate) rejection_count: usize,
    pub(crate) selected_capacity_acquired: bool,
    pub(crate) message: String,
}

pub(crate) fn workspace_snapshot_from_runtime(
    settings: &crate::models::routing::RuntimeRoutingSettings,
    candidates: Vec<RuntimeRoutingCandidate>,
    input: RoutingWorkspaceSnapshotInput,
    generated_at_ms: i64,
) -> RoutingWorkspaceSnapshot {
    let limit = input.limit.unwrap_or(128).clamp(1, 1024);
    let start = input
        .cursor
        .as_deref()
        .and_then(|cursor| cursor.strip_prefix("offset:"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let total = candidates.len();
    let rows = candidates
        .into_iter()
        .skip(start)
        .take(limit)
        .map(candidate_from_runtime)
        .collect::<Vec<_>>();
    let next = start + rows.len();
    RoutingWorkspaceSnapshot {
        read_model_version: ROUTING_WORKSPACE_READ_MODEL_VERSION,
        generated_at_ms,
        production_policy: settings.policy.clone(),
        preview_policy_version: ROUTING_PREVIEW_POLICY_VERSION,
        max_rate_multiplier: settings.max_rate_multiplier,
        routing_group_filter: settings.routing_group_filter.clone(),
        capacity_mode: RoutingCapacityReadMode::SnapshotOnly,
        page: RoutingReadPage {
            limit,
            returned: rows.len(),
            next_cursor: (next < total).then(|| format!("offset:{next}")),
        },
        candidates: rows,
        read_model_status: RoutingReadModelStatus::Available,
    }
}

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "pure projection preview seam is retained until the production planner cutover"
    )
)]
pub(crate) fn simulate_preview_from_candidate_projections(
    input: RoutePreviewSimulationInput,
    production_policy: RoutingPolicy,
    candidates: &[RouteCandidateProjection],
) -> RoutePreviewSimulation {
    let selected = candidates
        .iter()
        .find(|candidate| candidate.hard_rejection_codes.is_empty());
    let rejection_count = candidates
        .iter()
        .filter(|candidate| !candidate.hard_rejection_codes.is_empty())
        .count();
    RoutePreviewSimulation {
        preview_policy_version: ROUTING_PREVIEW_POLICY_VERSION,
        production_policy,
        capacity_mode: RoutingCapacityReadMode::SnapshotOnly,
        selected_station_key_id: selected
            .map(|candidate| candidate.identity.station_key_id.clone()),
        selected_station_id: selected.map(|candidate| candidate.identity.station_id.clone()),
        candidate_count: candidates.len(),
        rejection_count,
        selected_capacity_acquired: false,
        message: selected
            .map(|candidate| {
                format!(
                    "Preview selected {} for {:?}. Capacity is snapshot-only.",
                    candidate.identity.station_key_id, input.endpoint
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "Preview found no eligible route for {:?}. Capacity is snapshot-only.",
                    input.endpoint
                )
            }),
    }
}

fn candidate_from_runtime(candidate: RuntimeRoutingCandidate) -> RoutingWorkspaceCandidate {
    let health_state = candidate
        .health
        .as_ref()
        .map(|health| {
            if health.cooldown_until.is_some() {
                "cooldown"
            } else if health.consecutive_failures > 0 {
                "degraded"
            } else {
                "ready"
            }
        })
        .unwrap_or("unknown")
        .to_string();
    let balance_status = candidate
        .balance_snapshot
        .as_ref()
        .map(|balance| balance.status.clone());
    let price_basis = candidate
        .balance_snapshot
        .as_ref()
        .map(|_| "balance_only")
        .unwrap_or("unpriced")
        .to_string();
    RoutingWorkspaceCandidate {
        station_key_id: candidate.station_key_id.clone(),
        station_id: candidate.station_id.clone(),
        station_name: candidate.station_name,
        key_name: candidate.key_name,
        endpoint_revision: candidate.station_endpoint_revision,
        priority: candidate.routing_order.unwrap_or(candidate.priority),
        schedulable: candidate.schedulable,
        health_state,
        capability_summary: RoutingCapabilitySummary {
            chat_completions: candidate.capabilities.supports_chat_completions,
            responses: candidate.capabilities.supports_responses,
            embeddings: candidate.capabilities.supports_embeddings,
            stream: candidate.capabilities.supports_stream,
            tools: candidate.capabilities.supports_tools,
            vision: candidate.capabilities.supports_vision,
            reasoning: candidate.capabilities.supports_reasoning,
        },
        price_basis,
        balance_status,
        capacity: RoutingCandidateCapacitySnapshot {
            mode: RoutingCapacityReadMode::SnapshotOnly,
            max_concurrency: candidate.max_concurrency,
            in_flight: candidate.load_factor,
            acquired: false,
        },
        source_refs: RoutingCandidateSourceRefs {
            station_key_id: candidate.station_key_id,
            station_id: candidate.station_id,
            endpoint_revision: candidate.station_endpoint_revision,
        },
    }
}
