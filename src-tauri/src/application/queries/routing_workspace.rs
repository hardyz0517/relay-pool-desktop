use serde::{Deserialize, Serialize};

use crate::{
    application::operational_facts::candidate_projector::RouteCandidateProjection,
    models::routing::{RouteEndpointKind, RoutingGroupFilter, RoutingPolicy},
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
    pub(crate) group: Option<RoutingCandidateGroupSnapshot>,
    pub(crate) multiplier: RoutingCandidateMultiplierSnapshot,
    pub(crate) capability_summary: RoutingCapabilitySummary,
    pub(crate) capability_verdicts: RoutingCapabilityVerdictSnapshot,
    pub(crate) price_basis: String,
    pub(crate) pricing: RoutingCandidatePricingSnapshot,
    pub(crate) balance_status: Option<String>,
    pub(crate) capacity: RoutingCandidateCapacitySnapshot,
    pub(crate) source_refs: RoutingCandidateSourceRefs,
    pub(crate) hard_rejection_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateGroupSnapshot {
    pub(crate) stable_key: String,
    pub(crate) display_name: String,
    pub(crate) available: bool,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateMultiplierSnapshot {
    pub(crate) status: String,
    pub(crate) multiplier: Option<f64>,
    pub(crate) selected_source: Option<String>,
    pub(crate) ceiling_rejected: bool,
    pub(crate) reason: String,
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
pub(crate) struct RoutingCapabilityVerdictSnapshot {
    pub(crate) protocol: String,
    pub(crate) model: String,
    pub(crate) stream: String,
    pub(crate) tools: String,
    pub(crate) vision: String,
    pub(crate) reasoning: String,
    pub(crate) rejection_subjects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidatePricingSnapshot {
    pub(crate) basis: String,
    pub(crate) comparison_value: Option<f64>,
    pub(crate) reason: Option<String>,
    pub(crate) currency: Option<String>,
    pub(crate) unit: Option<String>,
    pub(crate) estimated_input_price: Option<f64>,
    pub(crate) estimated_output_price: Option<f64>,
    pub(crate) estimated_fixed_price: Option<f64>,
    pub(crate) status_label: String,
    pub(crate) source_chain: Vec<String>,
    pub(crate) observed_at: Option<String>,
    pub(crate) confidence: Option<f64>,
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
    pub(crate) snapshot_id: String,
    pub(crate) fact_version_vector: String,
    pub(crate) projector_version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutingWorkspaceProjectionCandidate {
    pub(crate) station_name: String,
    pub(crate) key_name: String,
    pub(crate) projection: RouteCandidateProjection,
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

pub(crate) fn workspace_snapshot_from_projection_candidates(
    settings: &crate::models::routing::RuntimeRoutingSettings,
    candidates: Vec<RoutingWorkspaceProjectionCandidate>,
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
        .map(candidate_from_projection)
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

fn candidate_from_projection(
    row: RoutingWorkspaceProjectionCandidate,
) -> RoutingWorkspaceCandidate {
    let projection = row.projection;
    let max_concurrency = projection
        .capacity
        .scopes
        .iter()
        .find_map(|scope| scope.limit.map(i64::from))
        .unwrap_or(0);
    let in_flight = projection
        .capacity
        .scopes
        .iter()
        .map(|scope| i64::from(scope.in_flight))
        .max();
    RoutingWorkspaceCandidate {
        station_key_id: projection.identity.station_key_id.clone(),
        station_id: projection.identity.station_id.clone(),
        station_name: row.station_name,
        key_name: row.key_name,
        endpoint_revision: projection.identity.endpoint_revision,
        priority: projection.priority,
        schedulable: projection.hard_rejection_codes.is_empty(),
        health_state: format!("{:?}", projection.health.station_key).to_lowercase(),
        group: projection.group.as_ref().map(|group| RoutingCandidateGroupSnapshot {
            stable_key: group.stable_key.clone(),
            display_name: group.display_name.clone(),
            available: group.available,
            reason: group.reason.to_string(),
        }),
        multiplier: RoutingCandidateMultiplierSnapshot {
            status: format!("{:?}", projection.multiplier.status).to_lowercase(),
            multiplier: projection.multiplier.multiplier,
            selected_source: projection.multiplier.selected_source.map(ToString::to_string),
            ceiling_rejected: projection.multiplier.ceiling_rejected,
            reason: projection.multiplier.reason.to_string(),
        },
        capability_summary: RoutingCapabilitySummary {
            chat_completions: projection.capability.protocol
                == crate::application::operational_facts::capability_projector::CapabilityDecision::Allow,
            responses: projection.capability.protocol
                == crate::application::operational_facts::capability_projector::CapabilityDecision::Allow,
            embeddings: false,
            stream: projection.capability.stream
                == crate::application::operational_facts::capability_projector::CapabilityDecision::Allow,
            tools: projection.capability.tools
                == crate::application::operational_facts::capability_projector::CapabilityDecision::Allow,
            vision: projection.capability.vision
                == crate::application::operational_facts::capability_projector::CapabilityDecision::Allow,
            reasoning: projection.capability.reasoning
                == crate::application::operational_facts::capability_projector::CapabilityDecision::Allow,
        },
        capability_verdicts: RoutingCapabilityVerdictSnapshot {
            protocol: format!("{:?}", projection.capability.protocol).to_lowercase(),
            model: format!("{:?}", projection.capability.model).to_lowercase(),
            stream: format!("{:?}", projection.capability.stream).to_lowercase(),
            tools: format!("{:?}", projection.capability.tools).to_lowercase(),
            vision: format!("{:?}", projection.capability.vision).to_lowercase(),
            reasoning: format!("{:?}", projection.capability.reasoning).to_lowercase(),
            rejection_subjects: projection.capability.rejection_subjects.clone(),
        },
        price_basis: projection.pricing.basis.as_str().to_string(),
        pricing: RoutingCandidatePricingSnapshot {
            basis: projection.pricing.basis.as_str().to_string(),
            comparison_value: projection.pricing.comparison_value,
            reason: projection.pricing.reason.map(ToString::to_string),
            currency: projection.pricing.currency.clone(),
            unit: projection.pricing.unit.clone(),
            estimated_input_price: projection.pricing.estimated_input_price,
            estimated_output_price: projection.pricing.estimated_output_price,
            estimated_fixed_price: projection.pricing.estimated_fixed_price,
            status_label: projection.pricing.status_label.clone(),
            source_chain: projection.pricing.source_chain.clone(),
            observed_at: projection.pricing.observed_at.clone(),
            confidence: projection.pricing.confidence,
        },
        balance_status: Some(format!("{:?}", projection.balance.status).to_lowercase()),
        capacity: RoutingCandidateCapacitySnapshot {
            mode: RoutingCapacityReadMode::SnapshotOnly,
            max_concurrency,
            in_flight,
            acquired: false,
        },
        source_refs: RoutingCandidateSourceRefs {
            station_key_id: projection.identity.station_key_id.clone(),
            station_id: projection.identity.station_id.clone(),
            endpoint_revision: projection.identity.endpoint_revision,
            snapshot_id: projection.provenance.snapshot_id.clone(),
            fact_version_vector: projection.provenance.fact_version_vector.clone(),
            projector_version: projection.provenance.projector_version.to_string(),
        },
        hard_rejection_codes: projection
            .hard_rejection_codes
            .iter()
            .map(|code| (*code).to_string())
            .collect(),
    }
}
