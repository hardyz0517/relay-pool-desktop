use serde::{Deserialize, Serialize};

use crate::{
    application::{
        operational_facts::pricing_projector::{request_cost_comparison_context, PricingRouteKind},
        routing_engine::request::RouteRequestFacts,
    },
    models::{
        pricing::ResolvedPricingContext,
        routing::{CanonicalRoutingCandidate, RoutingGroupFilter, RuntimeRoutingBalance},
        routing_policy::RoutingPolicyConfigV1,
    },
};

pub(crate) const ROUTING_WORKSPACE_READ_MODEL_VERSION: &str = "routing_workspace_read_model_v1";
pub(crate) const ROUTING_PREVIEW_POLICY_VERSION: &str = "intelligent_planner_v1";

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
    pub(crate) policy_config: RoutingPolicyConfigV1,
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
    pub(crate) balance_value: Option<f64>,
    pub(crate) balance_currency: Option<String>,
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

pub(crate) fn workspace_snapshot_from_canonical_candidates(
    policy_config: RoutingPolicyConfigV1,
    max_rate_multiplier: Option<f64>,
    routing_group_filter: RoutingGroupFilter,
    candidates: Vec<(CanonicalRoutingCandidate, Option<ResolvedPricingContext>)>,
    request: &RouteRequestFacts,
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
    let mut ordered_candidates = candidates
        .into_iter()
        .map(|(candidate, pricing)| {
            candidate_from_canonical(candidate, pricing, request, generated_at_ms)
        })
        .collect::<Vec<_>>();
    ordered_candidates.sort_by(|left, right| {
        depleted_rank(left.balance_value, left.balance_status.as_deref())
            .cmp(&depleted_rank(
                right.balance_value,
                right.balance_status.as_deref(),
            ))
            .then_with(|| (!left.schedulable).cmp(&(!right.schedulable)))
            .then_with(|| left.priority.cmp(&right.priority))
            .then_with(|| left.station_key_id.cmp(&right.station_key_id))
    });
    let total = ordered_candidates.len();
    let rows = ordered_candidates
        .into_iter()
        .skip(start)
        .take(limit)
        .collect::<Vec<_>>();
    let next = start + rows.len();
    RoutingWorkspaceSnapshot {
        read_model_version: ROUTING_WORKSPACE_READ_MODEL_VERSION,
        generated_at_ms,
        policy_config,
        preview_policy_version: ROUTING_PREVIEW_POLICY_VERSION,
        max_rate_multiplier,
        routing_group_filter,
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

fn depleted_rank(value: Option<f64>, status: Option<&str>) -> u8 {
    if value.is_some_and(|value| value.is_finite() && value <= 0.0)
        || status.is_some_and(|status| {
            matches!(
                status.trim().to_ascii_lowercase().as_str(),
                "low" | "depleted" | "exhausted" | "empty"
            )
        })
    {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::depleted_rank;

    #[test]
    fn negative_balance_is_always_in_the_depleted_display_tier() {
        assert_eq!(depleted_rank(Some(-0.05), Some("normal")), 1);
        assert_eq!(depleted_rank(Some(0.06), Some("normal")), 0);
    }
}

fn candidate_from_canonical(
    candidate: CanonicalRoutingCandidate,
    pricing: Option<ResolvedPricingContext>,
    request: &RouteRequestFacts,
    generated_at_ms: i64,
) -> RoutingWorkspaceCandidate {
    let group = candidate.economic_snapshot.as_ref().and_then(|economics| {
        let stable_key = economics
            .group_binding_id
            .as_deref()
            .map(|value| format!("binding:{value}"))
            .or_else(|| {
                economics
                    .group_id_hash
                    .as_deref()
                    .map(|value| format!("group-id:{value}"))
            })
            .or_else(|| {
                economics
                    .group_key_hash
                    .as_deref()
                    .map(|value| format!("key-hash:{value}"))
            })?;
        Some(RoutingCandidateGroupSnapshot {
            stable_key,
            display_name: economics
                .group_name
                .clone()
                .unwrap_or_else(|| "Unnamed group".to_string()),
            available: !matches!(
                economics.group_status.as_deref(),
                Some("disabled" | "missing")
            ),
            reason: economics
                .group_status
                .clone()
                .unwrap_or_else(|| "available".to_string()),
        })
    });
    let multiplier = candidate
        .economic_snapshot
        .as_ref()
        .and_then(|economics| economics.rate_multiplier);
    let ceiling_rejected = request
        .max_rate_multiplier()
        .zip(multiplier)
        .is_some_and(|(ceiling, value)| value > ceiling);
    let pricing_context =
        request_cost_comparison_context(PricingRouteKind::Inference, pricing.as_ref());
    let capability = &candidate.capabilities;
    let model_allowed = request.requested_model().is_none_or(|model| {
        !capability
            .model_blocklist
            .iter()
            .any(|blocked| blocked.eq_ignore_ascii_case(model))
            && (capability.model_allowlist.is_empty()
                || capability
                    .model_allowlist
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(model)))
    });
    let protocol_allowed = capability.supports_chat_completions || capability.supports_responses;
    let group_matches = request.required_group_stable_key().is_none_or(|required| {
        group
            .as_ref()
            .is_some_and(|candidate_group| candidate_group.stable_key == required)
    });
    let mut hard_rejection_codes = Vec::new();
    if !candidate.schedulable {
        hard_rejection_codes.push("candidate_unschedulable".to_string());
    }
    if candidate.api_key.is_none() && candidate.api_key_secret.is_none() {
        hard_rejection_codes.push("credential_missing".to_string());
    }
    if !group_matches {
        hard_rejection_codes.push("group_mismatch".to_string());
    }
    if ceiling_rejected {
        hard_rejection_codes.push("multiplier_ceiling".to_string());
    }
    if !protocol_allowed || !model_allowed {
        hard_rejection_codes.push("capability_rejected".to_string());
    }
    if request.stream() && !capability.supports_stream {
        hard_rejection_codes.push("capability_rejected".to_string());
    }
    if request.uses_tools() && !capability.supports_tools {
        hard_rejection_codes.push("capability_rejected".to_string());
    }
    if request.uses_vision() && !capability.supports_vision {
        hard_rejection_codes.push("capability_rejected".to_string());
    }
    if request.uses_reasoning() && !capability.supports_reasoning {
        hard_rejection_codes.push("capability_rejected".to_string());
    }
    if !request.allow_depleted_fallback()
        && candidate
            .balance_snapshot
            .as_ref()
            .is_some_and(RuntimeRoutingBalance::is_depleted)
    {
        hard_rejection_codes.push("balance_depleted".to_string());
    }
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
    let capacity_limit = if matches!(
        candidate.station_type.trim().to_ascii_lowercase().as_str(),
        "sub2api" | "newapi"
    ) {
        candidate
            .station_account_concurrency_limit
            .filter(|value| *value > 0)
            .unwrap_or(candidate.max_concurrency)
    } else {
        candidate.max_concurrency
    }
    .max(0);
    let in_flight = candidate.load_factor.map(|value| value.max(0));
    RoutingWorkspaceCandidate {
        station_key_id: candidate.station_key_id.clone(),
        station_id: candidate.station_id.clone(),
        station_name: candidate.station_name,
        key_name: candidate.key_name,
        endpoint_revision: candidate.station_endpoint_revision,
        priority: candidate.routing_order.unwrap_or(candidate.priority),
        schedulable: hard_rejection_codes.is_empty(),
        health_state,
        group,
        multiplier: RoutingCandidateMultiplierSnapshot {
            status: multiplier
                .map(|_| "resolved".to_string())
                .unwrap_or_else(|| "missing".to_string()),
            multiplier,
            selected_source: candidate
                .economic_snapshot
                .as_ref()
                .and_then(|economics| economics.rate_source.clone()),
            ceiling_rejected,
            reason: if ceiling_rejected {
                "above_policy_ceiling".to_string()
            } else {
                "canonical_economic_snapshot".to_string()
            },
        },
        capability_summary: RoutingCapabilitySummary {
            chat_completions: capability.supports_chat_completions,
            responses: capability.supports_responses,
            embeddings: capability.supports_embeddings,
            stream: capability.supports_stream,
            tools: capability.supports_tools,
            vision: capability.supports_vision,
            reasoning: capability.supports_reasoning,
        },
        capability_verdicts: RoutingCapabilityVerdictSnapshot {
            protocol: if protocol_allowed { "allow" } else { "deny" }.to_string(),
            model: if model_allowed { "allow" } else { "deny" }.to_string(),
            stream: if capability.supports_stream {
                "allow"
            } else {
                "deny"
            }
            .to_string(),
            tools: if capability.supports_tools {
                "allow"
            } else {
                "deny"
            }
            .to_string(),
            vision: if capability.supports_vision {
                "allow"
            } else {
                "deny"
            }
            .to_string(),
            reasoning: if capability.supports_reasoning {
                "allow"
            } else {
                "deny"
            }
            .to_string(),
            rejection_subjects: hard_rejection_codes
                .iter()
                .filter(|code| code.as_str() == "capability_rejected")
                .cloned()
                .collect(),
        },
        price_basis: pricing_context.basis.as_str().to_string(),
        pricing: RoutingCandidatePricingSnapshot {
            basis: pricing_context.basis.as_str().to_string(),
            comparison_value: pricing_context.comparison_value,
            reason: pricing_context.reason.map(ToString::to_string),
            currency: pricing_context.currency,
            unit: pricing_context.unit,
            estimated_input_price: pricing_context.estimated_input_price,
            estimated_output_price: pricing_context.estimated_output_price,
            estimated_fixed_price: pricing_context.estimated_fixed_price,
            status_label: pricing_context.status_label,
            source_chain: pricing_context.source_chain,
            observed_at: pricing_context.observed_at,
            confidence: pricing_context.confidence,
        },
        balance_status: candidate
            .balance_snapshot
            .as_ref()
            .map(|balance| balance.status.clone()),
        balance_value: candidate
            .balance_snapshot
            .as_ref()
            .and_then(|balance| balance.value),
        balance_currency: candidate
            .balance_snapshot
            .as_ref()
            .map(|balance| balance.currency.clone()),
        capacity: RoutingCandidateCapacitySnapshot {
            mode: RoutingCapacityReadMode::SnapshotOnly,
            max_concurrency: capacity_limit,
            in_flight: in_flight,
            acquired: false,
        },
        source_refs: RoutingCandidateSourceRefs {
            station_key_id: candidate.station_key_id,
            station_id: candidate.station_id,
            endpoint_revision: candidate.station_endpoint_revision,
            snapshot_id: format!("workspace-{generated_at_ms}"),
            fact_version_vector: format!(
                "endpoint:{};capabilities:{};health:{};balance:{}",
                candidate.station_endpoint_revision,
                candidate.capabilities.updated_at,
                candidate
                    .health
                    .as_ref()
                    .map(|health| health.updated_at.as_str())
                    .unwrap_or("missing"),
                candidate
                    .balance_snapshot
                    .as_ref()
                    .and_then(|balance| balance.collected_at.as_deref())
                    .unwrap_or("missing")
            ),
            projector_version: "routing_workspace_canonical_v1".to_string(),
        },
        hard_rejection_codes,
    }
}
