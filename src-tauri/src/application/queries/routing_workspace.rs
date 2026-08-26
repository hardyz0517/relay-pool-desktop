use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{
    application::{
        operational_facts::pricing_projector::{
            effective_rate_multiplier, request_cost_comparison_context, PricingRouteKind,
        },
        quality_projection::QualitySummary,
        routing_engine::{
            failure_domains::ProviderCapacityDomain, intelligent_planner::CandidateScoreBreakdown,
            request::RouteRequestFacts,
        },
    },
    models::{
        pricing::ResolvedPricingContext,
        routing::{CanonicalRoutingCandidate, RoutingGroupFilter, RuntimeRoutingBalance},
        routing_policy::RoutingPolicyConfigV2,
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
    pub(crate) policy_config: RoutingPolicyConfigV2,
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
    /// Normalized utility score (0..=10000) from the active routing policy.
    pub(crate) score: Option<u16>,
    pub(crate) score_details: Option<RoutingCandidateScoreSnapshot>,
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
    /// Trusted capacity-domain configuration facts. This is deliberately
    /// separate from runtime protection status: a configured identity is not
    /// proof that the provider is open, half-open, or currently failing.
    pub(crate) failure_domain: RoutingCandidateFailureDomainSnapshot,
    pub(crate) source_refs: RoutingCandidateSourceRefs,
    pub(crate) hard_rejection_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateFailureDomainSnapshot {
    pub(crate) kind: RoutingFailureDomainKind,
    pub(crate) resolution: RoutingFailureDomainResolution,
    pub(crate) provider_family: Option<String>,
    pub(crate) deployment_identity: Option<String>,
    pub(crate) region_identity: Option<String>,
    pub(crate) revision: Option<i64>,
    /// Present only when the current read-model request has a concrete model
    /// and the trusted identity can be converted to the canonical commitment.
    pub(crate) commitment: Option<String>,
    pub(crate) explanation_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoutingFailureDomainKind {
    CapacityDomain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoutingFailureDomainResolution {
    NotConfigured,
    InvalidIdentity,
    ModelRequired,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateScoreFactorSnapshot {
    pub(crate) score: u16,
    pub(crate) weight: u16,
    pub(crate) contribution: u16,
    pub(crate) inputs: Vec<RoutingCandidateScoreInputSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) window_details: Option<RoutingCandidateScoreWindowSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateScoreInputSnapshot {
    pub(crate) label: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateScoreWindowSnapshot {
    pub(crate) recent_observation_count: u64,
    pub(crate) recent_effective_mass_basis_points: u64,
    pub(crate) recent_success_mass_basis_points: u64,
    pub(crate) recent_failure_mass_basis_points: u64,
    pub(crate) recent_score: u16,
    pub(crate) recent_weight_basis_points: u16,
    pub(crate) recent_responsiveness_weight_basis_points: u16,
    pub(crate) recent_p95_latency_ms: Option<u32>,
    pub(crate) recent_latency_coverage_basis_points: u16,
    pub(crate) recent_responsiveness_basis_points: u16,
    pub(crate) historical_observation_count: u64,
    pub(crate) historical_effective_mass_basis_points: u64,
    pub(crate) historical_success_mass_basis_points: u64,
    pub(crate) historical_failure_mass_basis_points: u64,
    pub(crate) historical_score: u16,
    pub(crate) historical_weight_basis_points: u16,
    pub(crate) historical_responsiveness_weight_basis_points: u16,
    pub(crate) historical_p95_latency_ms: Option<u32>,
    pub(crate) historical_latency_coverage_basis_points: u16,
    pub(crate) historical_responsiveness_basis_points: u16,
    pub(crate) historical_age_window_days: u16,
    pub(crate) historical_half_life_days: u16,
}

impl From<&crate::application::quality_projection::QualitySummary>
    for RoutingCandidateScoreWindowSnapshot
{
    fn from(summary: &crate::application::quality_projection::QualitySummary) -> Self {
        let has_responsiveness_weights = summary.recent_responsiveness_weight_basis_points > 0
            || summary.historical_responsiveness_weight_basis_points > 0;
        Self {
            recent_observation_count: summary.recent_observation_count,
            recent_effective_mass_basis_points: summary.recent_effective_mass_basis_points,
            recent_success_mass_basis_points: summary.recent_success_mass_basis_points,
            recent_failure_mass_basis_points: summary.recent_failure_mass_basis_points,
            recent_score: summary.recent_reliability_basis_points,
            recent_weight_basis_points: summary.recent_reliability_weight_basis_points,
            recent_responsiveness_weight_basis_points: if has_responsiveness_weights {
                summary.recent_responsiveness_weight_basis_points
            } else {
                summary.recent_reliability_weight_basis_points
            },
            recent_p95_latency_ms: summary.recent_p95_latency_ms,
            recent_latency_coverage_basis_points: summary.recent_latency_coverage_basis_points,
            recent_responsiveness_basis_points: summary.recent_responsiveness_basis_points,
            historical_observation_count: summary.historical_observation_count,
            historical_effective_mass_basis_points: summary.historical_effective_mass_basis_points,
            historical_success_mass_basis_points: summary.historical_success_mass_basis_points,
            historical_failure_mass_basis_points: summary.historical_failure_mass_basis_points,
            historical_score: summary.historical_reliability_basis_points,
            historical_weight_basis_points: summary.historical_reliability_weight_basis_points,
            historical_responsiveness_weight_basis_points: if has_responsiveness_weights {
                summary.historical_responsiveness_weight_basis_points
            } else {
                summary.historical_reliability_weight_basis_points
            },
            historical_p95_latency_ms: summary.historical_p95_latency_ms,
            historical_latency_coverage_basis_points: summary
                .historical_latency_coverage_basis_points,
            historical_responsiveness_basis_points: summary.historical_responsiveness_basis_points,
            historical_age_window_days: summary.historical_age_window_days,
            historical_half_life_days: summary.historical_half_life_days,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateScoreSnapshot {
    pub(crate) total: u16,
    pub(crate) reliability: RoutingCandidateScoreFactorSnapshot,
    pub(crate) responsiveness: RoutingCandidateScoreFactorSnapshot,
    pub(crate) cost: RoutingCandidateScoreFactorSnapshot,
    pub(crate) preference: RoutingCandidateScoreFactorSnapshot,
}

impl From<CandidateScoreBreakdown> for RoutingCandidateScoreSnapshot {
    fn from(breakdown: CandidateScoreBreakdown) -> Self {
        let [reliability, responsiveness, cost, preference] = breakdown.factors;
        Self {
            total: breakdown.total,
            reliability: factor_snapshot(reliability),
            responsiveness: factor_snapshot(responsiveness),
            cost: factor_snapshot(cost),
            preference: factor_snapshot(preference),
        }
    }
}

fn factor_snapshot(
    factor: crate::application::routing_engine::fixed_point::FactorContribution,
) -> RoutingCandidateScoreFactorSnapshot {
    RoutingCandidateScoreFactorSnapshot {
        score: factor.score.get(),
        weight: factor.weight.get(),
        contribution: factor.contribution.get(),
        inputs: Vec::new(),
        window_details: None,
    }
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
    policy_config: RoutingPolicyConfigV2,
    max_rate_multiplier: Option<f64>,
    routing_group_filter: RoutingGroupFilter,
    candidates: Vec<(CanonicalRoutingCandidate, Option<ResolvedPricingContext>)>,
    scores: &BTreeMap<String, RoutingCandidateScoreSnapshot>,
    quality_summaries: &BTreeMap<String, QualitySummary>,
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
            let score_details = scores.get(&candidate.station_key_id).cloned();
            candidate_from_canonical(
                candidate,
                pricing,
                request,
                generated_at_ms,
                score_details,
                quality_summaries,
            )
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
    use super::{
        candidate_matches_group_scope, depleted_rank, failure_domain_snapshot,
        RoutingCandidateGroupSnapshot, RoutingFailureDomainResolution,
    };
    use crate::application::routing_engine::request::{
        CanonicalRouteRequest, GroupFilterMode, OrderingProfile, RouteKind, RouteRequestClassifier,
        ValidatedLocalRouteSettings,
    };
    use crate::models::{
        proxy::UpstreamApiFormat,
        routing::{CanonicalRoutingCandidate, StationKeyCapabilities},
    };

    fn request(
        model: Option<&str>,
    ) -> crate::application::routing_engine::request::RouteRequestFacts {
        RouteRequestClassifier::classify(
            CanonicalRouteRequest {
                route_kind: RouteKind::Inference,
                requested_model: model.map(ToOwned::to_owned),
                stream: false,
                uses_tools: false,
                uses_vision: false,
                uses_reasoning: false,
                untrusted_headers: Vec::new(),
            },
            ValidatedLocalRouteSettings {
                ordering_profile: OrderingProfile::PriorityFirst,
                max_rate_multiplier: None,
                group_filter_mode: GroupFilterMode::Any,
                required_group_stable_key: None,
                preferred_models: Vec::new(),
                required_tags: Vec::new(),
                allow_depleted_fallback: false,
                affinity_enabled: false,
            },
            1_800_000_000_000,
        )
    }

    fn candidate() -> CanonicalRoutingCandidate {
        CanonicalRoutingCandidate {
            station_key_id: "key-1".into(),
            station_id: "station-1".into(),
            station_type: "newapi".into(),
            capacity_provider_family: Some("OpenAI".into()),
            capacity_deployment_identity: Some("primary".into()),
            capacity_region_identity: Some("US".into()),
            capacity_domain_revision: Some(3),
            station_account_concurrency_limit: None,
            station_endpoint_revision: 1,
            sanitized_origin: "https://station.example.test".into(),
            upstream_api_format: UpstreamApiFormat::CustomOpenAiCompatible,
            routing_order: None,
            priority: 1,
            max_concurrency: 4,
            load_factor: None,
            schedulable: true,
            collector_proxy_mode: "inherit".into(),
            collector_proxy_url: None,
            station_name: "Station".into(),
            key_name: "Key".into(),
            capabilities: StationKeyCapabilities {
                station_key_id: "key-1".into(),
                supports_chat_completions: true,
                supports_responses: true,
                supports_embeddings: false,
                supports_stream: true,
                supports_tools: false,
                supports_vision: false,
                supports_reasoning: false,
                model_allowlist: Vec::new(),
                model_blocklist: Vec::new(),
                preferred_models: Vec::new(),
                only_use_as_backup: false,
                routing_tags: Vec::new(),
                updated_at: "1".into(),
            },
            health: None,
            balance_snapshot: None,
            economic_snapshot: None,
            api_key: None,
            api_key_secret: None,
        }
    }

    #[test]
    fn negative_balance_is_always_in_the_depleted_display_tier() {
        assert_eq!(depleted_rank(Some(-0.05), Some("normal")), 1);
        assert_eq!(depleted_rank(Some(0.06), Some("normal")), 0);
    }

    #[test]
    fn configured_capacity_identity_is_projected_without_claiming_runtime_health() {
        let projected = failure_domain_snapshot(&candidate(), &request(Some("gpt-test")));
        assert_eq!(
            projected.resolution,
            RoutingFailureDomainResolution::Resolved
        );
        assert_eq!(projected.provider_family.as_deref(), Some("OpenAI"));
        assert_eq!(projected.deployment_identity.as_deref(), Some("primary"));
        assert_eq!(projected.region_identity.as_deref(), Some("US"));
        assert_eq!(projected.revision, Some(3));
        assert!(projected
            .commitment
            .as_deref()
            .is_some_and(|value| value.starts_with("v1:")));
        assert_eq!(projected.explanation_key, "routing.failure_domain.resolved");
    }

    #[test]
    fn configured_capacity_identity_requires_model_before_commitment() {
        let projected = failure_domain_snapshot(&candidate(), &request(None));
        assert_eq!(
            projected.resolution,
            RoutingFailureDomainResolution::ModelRequired
        );
        assert!(projected.commitment.is_none());
        assert_eq!(
            projected.explanation_key,
            "routing.failure_domain.model_required"
        );
    }

    #[test]
    fn group_type_matches_canonical_category_instead_of_binding_id() {
        let request = RouteRequestClassifier::classify(
            CanonicalRouteRequest {
                route_kind: RouteKind::Inference,
                requested_model: None,
                stream: false,
                uses_tools: false,
                uses_vision: false,
                uses_reasoning: false,
                untrusted_headers: Vec::new(),
            },
            ValidatedLocalRouteSettings {
                ordering_profile: OrderingProfile::PriorityFirst,
                max_rate_multiplier: None,
                group_filter_mode: GroupFilterMode::Required,
                required_group_stable_key: Some("group-type:gpt".to_string()),
                preferred_models: Vec::new(),
                required_tags: Vec::new(),
                allow_depleted_fallback: false,
                affinity_enabled: false,
            },
            1_800_000_000_000,
        );
        let group = RoutingCandidateGroupSnapshot {
            stable_key: "binding:opaque-id".to_string(),
            display_name: "Plus".to_string(),
            available: true,
            reason: "bound".to_string(),
        };

        assert!(candidate_matches_group_scope(
            &request,
            Some(&group),
            Some("GPT")
        ));
        assert!(!candidate_matches_group_scope(
            &request,
            Some(&group),
            Some("claude")
        ));
        assert!(!candidate_matches_group_scope(&request, Some(&group), None));
    }
}

fn candidate_from_canonical(
    candidate: CanonicalRoutingCandidate,
    pricing: Option<ResolvedPricingContext>,
    request: &RouteRequestFacts,
    generated_at_ms: i64,
    score_details: Option<RoutingCandidateScoreSnapshot>,
    quality_summaries: &BTreeMap<String, QualitySummary>,
) -> RoutingWorkspaceCandidate {
    let failure_domain = failure_domain_snapshot(&candidate, request);
    let score_details = score_details.map(|mut details| {
        let quality_summary = quality_summaries
            .get(&format!("station_key:{}", candidate.station_key_id))
            .filter(|summary| {
                summary.recent_observation_count > 0 || summary.historical_observation_count > 0
            });
        if let Some(summary) = quality_summary {
            let window_details = Some(summary.into());
            details.reliability.window_details = window_details.clone();
            details.responsiveness.window_details = window_details;
            details.reliability.inputs = vec![
                score_input(
                    "近24小时成功",
                    format_mass_value(summary.recent_success_mass_basis_points),
                ),
                score_input(
                    "近24小时失败",
                    format_mass_value(summary.recent_failure_mass_basis_points),
                ),
                score_input(
                    "历史成功（衰减后）",
                    format_mass_value(summary.historical_success_mass_basis_points),
                ),
                score_input(
                    "历史失败（衰减后）",
                    format_mass_value(summary.historical_failure_mass_basis_points),
                ),
                score_input("先验", "2 成功 + 2 失败"),
            ];
            details.responsiveness.inputs = vec![
                score_input(
                    "近24小时 P95",
                    format_latency_value(summary.recent_p95_latency_ms),
                ),
                score_input(
                    "历史 P95",
                    format_latency_value(summary.historical_p95_latency_ms),
                ),
                score_input(
                    "近24小时延迟覆盖",
                    format_basis_points(summary.recent_latency_coverage_basis_points),
                ),
                score_input(
                    "历史延迟覆盖",
                    format_basis_points(summary.historical_latency_coverage_basis_points),
                ),
                score_input("延迟上限", "120000 ms"),
            ];
        } else {
            // Keep the pre-projection aggregate as a first-run compatibility
            // path; once the quality projector catches up, the window data
            // above becomes the only source shown to users.
            details.reliability.inputs = vec![
                score_input(
                    "成功请求",
                    candidate
                        .health
                        .as_ref()
                        .map(|health| health.success_count.max(0).to_string())
                        .unwrap_or_else(|| "暂无数据".to_string()),
                ),
                score_input(
                    "失败请求",
                    candidate
                        .health
                        .as_ref()
                        .map(|health| health.failure_count.max(0).to_string())
                        .unwrap_or_else(|| "暂无数据".to_string()),
                ),
                score_input("先验", "2 成功 + 2 失败"),
            ];
            details.responsiveness.inputs = vec![
                score_input(
                    "最近平均延迟",
                    candidate
                        .health
                        .as_ref()
                        .and_then(|health| health.avg_latency_ms)
                        .map(|value| format!("{value} ms"))
                        .unwrap_or_else(|| "暂无数据".to_string()),
                ),
                score_input("延迟上限", "120000 ms"),
            ];
        }
        details.cost.inputs = vec![
            score_input(
                "密钥有效倍率",
                pricing
                    .as_ref()
                    .and_then(|value| value.effective_rate_multiplier)
                    .or_else(|| {
                        let economics = candidate.economic_snapshot.as_ref()?;
                        effective_rate_multiplier(
                            economics.rate_multiplier,
                            economics.credit_per_cny.unwrap_or(1.0),
                        )
                    })
                    .map(|value| format!("{value:.4}x"))
                    .unwrap_or_else(|| "暂无数据".to_string()),
            ),
            score_input("倍率代理成本分", format_basis_points(details.cost.score)),
        ];
        details.preference.inputs = vec![
            score_input("候选优先级", candidate.priority.to_string()),
            score_input(
                "优先级换算分",
                format_basis_points(details.preference.score),
            ),
        ];
        details
    });
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
    // Pricing resolution is the canonical source for inference routes. The
    // runtime economic snapshot stores the station-native/raw multiplier and
    // therefore must not bypass exchange-rate normalization here.
    let multiplier = match request.route_kind() {
        crate::application::routing_engine::request::RouteKind::Inference => pricing
            .as_ref()
            .and_then(|context| context.effective_rate_multiplier),
        crate::application::routing_engine::request::RouteKind::ModelCatalog => {
            candidate.economic_snapshot.as_ref().and_then(|economics| {
                effective_rate_multiplier(
                    economics.rate_multiplier,
                    economics.credit_per_cny.unwrap_or(1.0),
                )
            })
        }
    };
    let multiplier = multiplier.or_else(|| {
        let economics = candidate.economic_snapshot.as_ref()?;
        effective_rate_multiplier(
            economics.rate_multiplier,
            economics.credit_per_cny.unwrap_or(1.0),
        )
    });
    let ceiling_rejected = request
        .max_rate_multiplier()
        .zip(multiplier)
        .is_some_and(|(ceiling, value)| value > ceiling);
    let multiplier_source_is_pricing_context = matches!(
        request.route_kind(),
        crate::application::routing_engine::request::RouteKind::Inference
    ) && pricing
        .as_ref()
        .and_then(|context| context.effective_rate_multiplier)
        .is_some();
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
    let group_matches = candidate_matches_group_scope(
        request,
        group.as_ref(),
        candidate
            .economic_snapshot
            .as_ref()
            .and_then(|economics| economics.group_category.as_deref()),
    );
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
        // Keep the administrative switch separate from request-specific
        // eligibility. `hard_rejection_codes` owns the latter.
        schedulable: candidate.schedulable,
        health_state,
        score: score_details.as_ref().map(|details| details.total),
        score_details,
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
            } else if multiplier_source_is_pricing_context {
                "pricing_context_effective_rate".to_string()
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
        failure_domain,
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

fn failure_domain_snapshot(
    candidate: &CanonicalRoutingCandidate,
    request: &RouteRequestFacts,
) -> RoutingCandidateFailureDomainSnapshot {
    let provider_family = candidate
        .capacity_provider_family
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let deployment_identity = candidate
        .capacity_deployment_identity
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let region_identity = candidate
        .capacity_region_identity
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let revision = candidate.capacity_domain_revision;

    let Some(provider) = provider_family.clone() else {
        return RoutingCandidateFailureDomainSnapshot {
            kind: RoutingFailureDomainKind::CapacityDomain,
            resolution: RoutingFailureDomainResolution::NotConfigured,
            provider_family: None,
            deployment_identity: None,
            region_identity: None,
            revision: None,
            commitment: None,
            explanation_key: "routing.failure_domain.not_configured".to_string(),
        };
    };

    let Some(model) = request.requested_model() else {
        return RoutingCandidateFailureDomainSnapshot {
            kind: RoutingFailureDomainKind::CapacityDomain,
            resolution: RoutingFailureDomainResolution::ModelRequired,
            provider_family: Some(provider),
            deployment_identity,
            region_identity,
            revision,
            commitment: None,
            explanation_key: "routing.failure_domain.model_required".to_string(),
        };
    };

    let domain = ProviderCapacityDomain::from_trusted_identity(
        provider.as_str(),
        model,
        deployment_identity.as_deref(),
        region_identity.as_deref(),
    );
    let Some(domain) = domain else {
        return RoutingCandidateFailureDomainSnapshot {
            kind: RoutingFailureDomainKind::CapacityDomain,
            resolution: RoutingFailureDomainResolution::InvalidIdentity,
            provider_family: Some(provider),
            deployment_identity,
            region_identity,
            revision,
            commitment: None,
            explanation_key: "routing.failure_domain.invalid_identity".to_string(),
        };
    };
    let commitment = domain.commitment();
    RoutingCandidateFailureDomainSnapshot {
        kind: RoutingFailureDomainKind::CapacityDomain,
        resolution: RoutingFailureDomainResolution::Resolved,
        provider_family: Some(provider),
        deployment_identity,
        region_identity,
        revision,
        commitment: Some(format!(
            "v{}:{}",
            commitment.schema_version, commitment.digest_hex
        )),
        explanation_key: "routing.failure_domain.resolved".to_string(),
    }
}

fn score_input(
    label: impl Into<String>,
    value: impl Into<String>,
) -> RoutingCandidateScoreInputSnapshot {
    RoutingCandidateScoreInputSnapshot {
        label: label.into(),
        value: value.into(),
    }
}

fn format_mass_value(value: u64) -> String {
    let tenths = value.saturating_mul(10).saturating_add(5_000) / 10_000;
    if tenths % 10 == 0 {
        format!("{} 次", tenths / 10)
    } else {
        format!("{}.{} 次", tenths / 10, tenths % 10)
    }
}

fn format_latency_value(value: Option<u32>) -> String {
    value
        .map(|value| {
            if value >= 1_000 {
                format!("{:.2} s", value as f64 / 1_000.0)
            } else {
                format!("{value} ms")
            }
        })
        .unwrap_or_else(|| "暂无数据".to_string())
}

fn format_basis_points(value: u16) -> String {
    format!("{}%", value as f64 / 100.0)
}

fn candidate_matches_group_scope(
    request: &RouteRequestFacts,
    group: Option<&RoutingCandidateGroupSnapshot>,
    group_category: Option<&str>,
) -> bool {
    use crate::application::routing_engine::request::GroupFilterMode;

    match request.group_filter_mode() {
        GroupFilterMode::Any => true,
        GroupFilterMode::UngroupedOnly => group.is_none(),
        GroupFilterMode::Required => {
            let Some(required) = request.required_group_stable_key() else {
                return false;
            };
            group.is_some_and(|candidate_group| candidate_group.stable_key == required)
                || required
                    .strip_prefix("group-type:")
                    .is_some_and(|category| {
                        group_category.is_some_and(|actual| actual.eq_ignore_ascii_case(category))
                    })
        }
    }
}
