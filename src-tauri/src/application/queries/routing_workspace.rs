use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{
    application::{
        operational_facts::pricing_projector::{
            effective_rate_multiplier, request_cost_comparison_context, PricingRouteKind,
        },
        quality_projection::QualitySummary,
        routing_engine::{
            intelligent_planner::CandidateScoreBreakdown, request::RouteRequestFacts,
            tiers::AvailabilityTier,
        },
        station_key_circuit::{StationKeyCircuitState, StationKeyCircuitStatus},
    },
    models::{
        pricing::ResolvedPricingContext,
        routing::{CanonicalRoutingCandidate, RoutingGroupFilter, RuntimeRoutingBalance},
        routing_policy::RoutingPolicyConfigV3,
    },
    persistence::stores::routing_quality_store::RoutingAttemptCountDiagnostics,
};

pub(crate) const ROUTING_WORKSPACE_READ_MODEL_VERSION: &str = "routing_workspace_read_model_v3";
pub(crate) const ROUTING_PREVIEW_POLICY_VERSION: &str = "intelligent_planner_v3";

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
    pub(crate) policy_config: RoutingPolicyConfigV3,
    pub(crate) preview_policy_version: &'static str,
    pub(crate) max_rate_multiplier: Option<f64>,
    pub(crate) routing_group_filter: RoutingGroupFilter,
    pub(crate) capacity_mode: RoutingCapacityReadMode,
    pub(crate) page: RoutingReadPage,
    pub(crate) candidates: Vec<RoutingWorkspaceCandidate>,
    pub(crate) read_model_status: RoutingReadModelStatus,
    pub(crate) planner_evaluation: RoutingPlannerEvaluationStatus,
    pub(crate) planner_evaluation_code: Option<String>,
    pub(crate) availability_status: RoutingAvailabilityStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) runtime_generation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) policy_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) quality_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) health_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) quality_projection_backlog: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) quality_projection_lag_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) quality_stale: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RoutingWorkspaceRevisionSnapshot {
    pub(crate) runtime_generation_id: Option<String>,
    pub(crate) policy_revision: Option<u64>,
    pub(crate) quality_revision: Option<u64>,
    pub(crate) health_revision: Option<u64>,
    pub(crate) quality_projection_backlog: Option<u64>,
    pub(crate) quality_projection_lag_seconds: Option<u64>,
    pub(crate) quality_stale: Option<bool>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoutingPlannerEvaluationStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoutingAvailabilityStatus {
    Available,
    CapacityExhausted,
    CapacityStateUnavailable,
    AllKeysUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoutingScoreStatus {
    Scored,
    Excluded,
    CandidateLimit,
    ProbeDiscovery,
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
    pub(crate) score_status: RoutingScoreStatus,
    pub(crate) planner_exclusion_codes: Vec<String>,
    pub(crate) assessment_snapshot_id: Option<String>,
    pub(crate) assessment_durable_revision: Option<u64>,
    pub(crate) assessment_request_context_fingerprint: Option<String>,
    pub(crate) score_details: Option<RoutingCandidateScoreSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) diagnostics: Option<RoutingCandidateDiagnostics>,
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
pub(crate) struct RoutingCandidateDiagnostics {
    pub(crate) effective_score: Option<u16>,
    pub(crate) base_score: Option<u16>,
    pub(crate) quality: Option<RoutingCandidateQualityDiagnostics>,
    pub(crate) attempts: RoutingCandidateAttemptDiagnostics,
    pub(crate) circuit: RoutingCandidateCircuitDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateQualityDiagnostics {
    pub(crate) quality_revision: u64,
    pub(crate) quality_policy_revision: u64,
    pub(crate) algorithm_version: String,
    pub(crate) quality_basis: String,
    pub(crate) quality_unavailable: bool,
    pub(crate) canonical_sample_count: u64,
    pub(crate) real_reliability_basis_points: u16,
    pub(crate) monitoring_reliability_basis_points: u16,
    pub(crate) effective_real_weight_basis_points: u16,
    pub(crate) effective_monitoring_weight_basis_points: u16,
    pub(crate) real_source_eligible: bool,
    pub(crate) monitoring_source_eligible: bool,
    pub(crate) monitoring_source_status: RoutingMonitoringSourceStatus,
    pub(crate) recent_sample_count: u64,
    pub(crate) recent_effective_mass_basis_points: u64,
    pub(crate) recent_minimum_samples: u64,
    pub(crate) historical_sample_count: u64,
    pub(crate) historical_effective_mass_basis_points: u64,
    pub(crate) historical_minimum_samples: u64,
    pub(crate) real_source: RoutingCandidateQualitySourceDiagnostics,
    pub(crate) monitoring_source: RoutingCandidateQualitySourceDiagnostics,
    pub(crate) latency: RoutingCandidateLatencyDiagnostics,
    pub(crate) idle_real_route_sample: String,
    pub(crate) last_real_route_sample_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoutingMonitoringSourceStatus {
    Comparable,
    NoEvidence,
    Incomparable,
    WeightZero,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateQualityWindowDiagnostics {
    pub(crate) sample_count: u64,
    pub(crate) effective_weight: u64,
    pub(crate) success_weight: u64,
    pub(crate) failure_weight: u64,
    pub(crate) reliability_basis_points: u16,
    pub(crate) minimum_met: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateQualitySourceDiagnostics {
    pub(crate) eligible: bool,
    pub(crate) effective_weight_basis_points: u16,
    pub(crate) recent: RoutingCandidateQualityWindowDiagnostics,
    pub(crate) historical: RoutingCandidateQualityWindowDiagnostics,
    pub(crate) blended_reliability_basis_points: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateLatencyDiagnostics {
    pub(crate) recent_sample_count: u64,
    pub(crate) recent_effective_weight: u64,
    pub(crate) recent_weighted_latency_ms: u32,
    pub(crate) recent_minimum_met: bool,
    pub(crate) historical_sample_count: u64,
    pub(crate) historical_effective_weight: u64,
    pub(crate) historical_weighted_latency_ms: u32,
    pub(crate) historical_minimum_met: bool,
    pub(crate) blended_weighted_latency_ms: u32,
}

impl From<&crate::application::quality_projection::QualitySourceWindowSummary>
    for RoutingCandidateQualityWindowDiagnostics
{
    fn from(value: &crate::application::quality_projection::QualitySourceWindowSummary) -> Self {
        Self {
            sample_count: value.sample_count,
            effective_weight: value.effective_weight,
            success_weight: value.success_weight,
            failure_weight: value.failure_weight,
            reliability_basis_points: value.reliability_basis_points,
            minimum_met: value.minimum_met,
        }
    }
}

impl From<&crate::application::quality_projection::QualitySourceSummary>
    for RoutingCandidateQualitySourceDiagnostics
{
    fn from(value: &crate::application::quality_projection::QualitySourceSummary) -> Self {
        Self {
            eligible: value.eligible,
            effective_weight_basis_points: value.effective_weight_basis_points,
            recent: (&value.recent).into(),
            historical: (&value.historical).into(),
            blended_reliability_basis_points: value.blended_reliability_basis_points,
        }
    }
}

impl From<&crate::application::quality_projection::QualityLatencySummary>
    for RoutingCandidateLatencyDiagnostics
{
    fn from(value: &crate::application::quality_projection::QualityLatencySummary) -> Self {
        Self {
            recent_sample_count: value.recent_sample_count,
            recent_effective_weight: value.recent_effective_weight,
            recent_weighted_latency_ms: value.recent_weighted_latency_ms,
            recent_minimum_met: value.recent_minimum_met,
            historical_sample_count: value.historical_sample_count,
            historical_effective_weight: value.historical_effective_weight,
            historical_weighted_latency_ms: value.historical_weighted_latency_ms,
            historical_minimum_met: value.historical_minimum_met,
            blended_weighted_latency_ms: value.blended_weighted_latency_ms,
        }
    }
}

impl From<&QualitySummary> for RoutingCandidateQualityDiagnostics {
    fn from(summary: &QualitySummary) -> Self {
        Self {
            quality_revision: summary.checkpoint_sequence,
            quality_policy_revision: summary.quality_policy_revision,
            algorithm_version: summary.algorithm_version.clone(),
            quality_basis: summary.quality_basis.clone(),
            quality_unavailable: summary.quality_unavailable,
            canonical_sample_count: summary.observation_count,
            real_reliability_basis_points: summary.real_reliability_basis_points,
            monitoring_reliability_basis_points: summary.monitoring_reliability_basis_points,
            effective_real_weight_basis_points: summary.real_source_weight_basis_points,
            effective_monitoring_weight_basis_points: summary.monitoring_source_weight_basis_points,
            real_source_eligible: summary.real_source_eligible,
            monitoring_source_eligible: summary.monitoring_source_eligible,
            monitoring_source_status: summary.monitoring_source_status.into(),
            recent_sample_count: summary.recent_observation_count,
            recent_effective_mass_basis_points: summary.recent_effective_mass_basis_points,
            recent_minimum_samples: summary.recent_minimum_samples,
            historical_sample_count: summary.historical_observation_count,
            historical_effective_mass_basis_points: summary.historical_effective_mass_basis_points,
            historical_minimum_samples: summary.historical_minimum_samples,
            real_source: (&summary.real_source).into(),
            monitoring_source: (&summary.monitoring_source).into(),
            latency: (&summary.latency).into(),
            idle_real_route_sample: summary.idle_real_route_sample.clone(),
            last_real_route_sample_at_ms: summary.last_real_route_sample_at_ms,
        }
    }
}

impl From<crate::application::quality_projection::MonitoringSourceStatus>
    for RoutingMonitoringSourceStatus
{
    fn from(value: crate::application::quality_projection::MonitoringSourceStatus) -> Self {
        use crate::application::quality_projection::MonitoringSourceStatus as Source;
        match value {
            Source::Comparable => Self::Comparable,
            Source::NoEvidence => Self::NoEvidence,
            Source::Incomparable => Self::Incomparable,
            Source::WeightZero => Self::WeightZero,
            Source::Disabled => Self::Disabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateAttemptDiagnostics {
    pub(crate) raw_real_attempt_count: u64,
    pub(crate) deduplicated_real_request_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoutingCandidateCircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoutingCandidateScoreGateStatus {
    NotApplicable,
    WaitingCooldown,
    Passed,
    Denied,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCandidateCircuitDiagnostics {
    pub(crate) state: RoutingCandidateCircuitState,
    pub(crate) state_revision: Option<u64>,
    pub(crate) lifecycle_revision: Option<u64>,
    pub(crate) consecutive_failures: Option<u16>,
    pub(crate) reopen_level: u32,
    pub(crate) cooldown_until_ms: Option<u64>,
    pub(crate) cooldown_remaining_ms: Option<u64>,
    pub(crate) half_open_lease_in_flight: bool,
    pub(crate) half_open_lease_expires_at_ms: Option<u64>,
    pub(crate) recovery_successes: Option<u16>,
    pub(crate) score_gate_status: RoutingCandidateScoreGateStatus,
    pub(crate) score_gate_reason: String,
    pub(crate) best_closed_effective_score: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RoutingCandidatePlanDiagnostics {
    pub(crate) effective_score: u16,
    pub(crate) base_score: u16,
    pub(crate) target_rank: u16,
    pub(crate) tier: AvailabilityTier,
    pub(crate) lifecycle_revision: u64,
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
    pub(crate) recent_real_sample_count: u64,
    pub(crate) recent_monitoring_sample_count: u64,
    pub(crate) recent_effective_mass_basis_points: u64,
    pub(crate) recent_success_mass_basis_points: u64,
    pub(crate) recent_failure_mass_basis_points: u64,
    pub(crate) recent_minimum_samples: u64,
    pub(crate) recent_reliability_minimum_met: bool,
    pub(crate) recent_score: u16,
    pub(crate) recent_weight_basis_points: u16,
    pub(crate) recent_responsiveness_weight_basis_points: u16,
    pub(crate) recent_latency_sample_count: u64,
    pub(crate) recent_latency_effective_mass_basis_points: u64,
    pub(crate) recent_weighted_latency_ms: u32,
    pub(crate) recent_latency_minimum_met: bool,
    pub(crate) recent_real_latency_sample_count: u64,
    pub(crate) recent_monitoring_latency_sample_count: u64,
    pub(crate) recent_real_weighted_latency_ms: u32,
    pub(crate) recent_monitoring_weighted_latency_ms: u32,
    pub(crate) recent_real_latency_minimum_met: bool,
    pub(crate) recent_monitoring_latency_minimum_met: bool,
    pub(crate) responsiveness_real_source_weight_basis_points: u16,
    pub(crate) responsiveness_monitoring_source_weight_basis_points: u16,
    pub(crate) recent_latency_coverage_basis_points: u16,
    pub(crate) recent_responsiveness_basis_points: u16,
    pub(crate) historical_observation_count: u64,
    pub(crate) historical_real_sample_count: u64,
    pub(crate) historical_monitoring_sample_count: u64,
    pub(crate) historical_effective_mass_basis_points: u64,
    pub(crate) historical_success_mass_basis_points: u64,
    pub(crate) historical_failure_mass_basis_points: u64,
    pub(crate) historical_minimum_samples: u64,
    pub(crate) historical_reliability_minimum_met: bool,
    pub(crate) historical_score: u16,
    pub(crate) historical_weight_basis_points: u16,
    pub(crate) historical_responsiveness_weight_basis_points: u16,
    pub(crate) historical_latency_sample_count: u64,
    pub(crate) historical_latency_effective_mass_basis_points: u64,
    pub(crate) historical_weighted_latency_ms: u32,
    pub(crate) historical_latency_minimum_met: bool,
    pub(crate) historical_real_latency_sample_count: u64,
    pub(crate) historical_monitoring_latency_sample_count: u64,
    pub(crate) historical_real_weighted_latency_ms: u32,
    pub(crate) historical_monitoring_weighted_latency_ms: u32,
    pub(crate) historical_real_latency_minimum_met: bool,
    pub(crate) historical_monitoring_latency_minimum_met: bool,
    pub(crate) historical_latency_coverage_basis_points: u16,
    pub(crate) historical_responsiveness_basis_points: u16,
    pub(crate) historical_age_window_days: u16,
    pub(crate) historical_half_life_days: u16,
    pub(crate) monitoring_source_status: RoutingMonitoringSourceStatus,
}

impl From<&crate::application::quality_projection::QualitySummary>
    for RoutingCandidateScoreWindowSnapshot
{
    fn from(summary: &crate::application::quality_projection::QualitySummary) -> Self {
        let has_responsiveness_weights = summary.recent_responsiveness_weight_basis_points > 0
            || summary.historical_responsiveness_weight_basis_points > 0;
        Self {
            recent_observation_count: summary
                .real_source
                .recent
                .sample_count
                .saturating_add(summary.monitoring_source.recent.sample_count),
            recent_real_sample_count: summary.real_source.recent.sample_count,
            recent_monitoring_sample_count: summary.monitoring_source.recent.sample_count,
            recent_effective_mass_basis_points: summary
                .real_source
                .recent
                .effective_weight
                .saturating_add(summary.monitoring_source.recent.effective_weight),
            recent_success_mass_basis_points: summary
                .real_source
                .recent
                .success_weight
                .saturating_add(summary.monitoring_source.recent.success_weight),
            recent_failure_mass_basis_points: summary
                .real_source
                .recent
                .failure_weight
                .saturating_add(summary.monitoring_source.recent.failure_weight),
            recent_minimum_samples: summary.recent_minimum_samples,
            recent_reliability_minimum_met: summary.real_source.recent.minimum_met,
            recent_score: summary.recent_reliability_basis_points,
            recent_weight_basis_points: summary.recent_reliability_weight_basis_points,
            recent_responsiveness_weight_basis_points: if has_responsiveness_weights {
                summary.recent_responsiveness_weight_basis_points
            } else {
                summary.recent_reliability_weight_basis_points
            },
            recent_latency_sample_count: summary.latency.recent_sample_count,
            recent_latency_effective_mass_basis_points: summary.latency.recent_effective_weight,
            recent_weighted_latency_ms: summary.latency.recent_weighted_latency_ms,
            recent_latency_minimum_met: summary.latency.recent_minimum_met,
            recent_real_latency_sample_count: summary.latency.real_source.recent_sample_count,
            recent_monitoring_latency_sample_count: summary
                .latency
                .monitoring_source
                .recent_sample_count,
            recent_real_weighted_latency_ms: summary.latency.real_source.recent_weighted_latency_ms,
            recent_monitoring_weighted_latency_ms: summary
                .latency
                .monitoring_source
                .recent_weighted_latency_ms,
            recent_real_latency_minimum_met: summary.latency.real_source.recent_minimum_met,
            recent_monitoring_latency_minimum_met: summary
                .latency
                .monitoring_source
                .recent_minimum_met,
            responsiveness_real_source_weight_basis_points: summary
                .latency
                .real_source_weight_basis_points,
            responsiveness_monitoring_source_weight_basis_points: summary
                .latency
                .monitoring_source_weight_basis_points,
            recent_latency_coverage_basis_points: summary.recent_latency_coverage_basis_points,
            recent_responsiveness_basis_points: summary.recent_responsiveness_basis_points,
            historical_observation_count: summary
                .real_source
                .historical
                .sample_count
                .saturating_add(summary.monitoring_source.historical.sample_count),
            historical_real_sample_count: summary.real_source.historical.sample_count,
            historical_monitoring_sample_count: summary.monitoring_source.historical.sample_count,
            historical_effective_mass_basis_points: summary
                .real_source
                .historical
                .effective_weight
                .saturating_add(summary.monitoring_source.historical.effective_weight),
            historical_success_mass_basis_points: summary
                .real_source
                .historical
                .success_weight
                .saturating_add(summary.monitoring_source.historical.success_weight),
            historical_failure_mass_basis_points: summary
                .real_source
                .historical
                .failure_weight
                .saturating_add(summary.monitoring_source.historical.failure_weight),
            historical_minimum_samples: summary.historical_minimum_samples,
            historical_reliability_minimum_met: summary.real_source.historical.minimum_met,
            historical_score: summary.historical_reliability_basis_points,
            historical_weight_basis_points: summary.historical_reliability_weight_basis_points,
            historical_responsiveness_weight_basis_points: if has_responsiveness_weights {
                summary.historical_responsiveness_weight_basis_points
            } else {
                summary.historical_reliability_weight_basis_points
            },
            historical_latency_sample_count: summary.latency.historical_sample_count,
            historical_latency_effective_mass_basis_points: summary
                .latency
                .historical_effective_weight,
            historical_weighted_latency_ms: summary.latency.historical_weighted_latency_ms,
            historical_latency_minimum_met: summary.latency.historical_minimum_met,
            historical_real_latency_sample_count: summary
                .latency
                .real_source
                .historical_sample_count,
            historical_monitoring_latency_sample_count: summary
                .latency
                .monitoring_source
                .historical_sample_count,
            historical_real_weighted_latency_ms: summary
                .latency
                .real_source
                .historical_weighted_latency_ms,
            historical_monitoring_weighted_latency_ms: summary
                .latency
                .monitoring_source
                .historical_weighted_latency_ms,
            historical_real_latency_minimum_met: summary.latency.real_source.historical_minimum_met,
            historical_monitoring_latency_minimum_met: summary
                .latency
                .monitoring_source
                .historical_minimum_met,
            historical_latency_coverage_basis_points: summary
                .historical_latency_coverage_basis_points,
            historical_responsiveness_basis_points: summary.historical_responsiveness_basis_points,
            historical_age_window_days: summary.historical_age_window_days,
            historical_half_life_days: summary.historical_half_life_days,
            monitoring_source_status: summary.monitoring_source_status.into(),
        }
    }
}

impl RoutingCandidateScoreWindowSnapshot {
    fn optimistic(
        policy: &RoutingPolicyConfigV3,
        reliability_score: u16,
        responsiveness_score: u16,
    ) -> Self {
        Self {
            recent_observation_count: 0,
            recent_real_sample_count: 0,
            recent_monitoring_sample_count: 0,
            recent_effective_mass_basis_points: 0,
            recent_success_mass_basis_points: 0,
            recent_failure_mass_basis_points: 0,
            recent_minimum_samples: u64::from(policy.reliability_sampling.recent_minimum_samples),
            recent_reliability_minimum_met: false,
            recent_score: reliability_score,
            recent_weight_basis_points: 0,
            recent_responsiveness_weight_basis_points: 0,
            recent_latency_sample_count: 0,
            recent_latency_effective_mass_basis_points: 0,
            recent_weighted_latency_ms: policy.reliability_sampling.optimistic_latency_ms,
            recent_latency_minimum_met: false,
            recent_real_latency_sample_count: 0,
            recent_monitoring_latency_sample_count: 0,
            recent_real_weighted_latency_ms: policy.reliability_sampling.optimistic_latency_ms,
            recent_monitoring_weighted_latency_ms: policy
                .reliability_sampling
                .optimistic_latency_ms,
            recent_real_latency_minimum_met: false,
            recent_monitoring_latency_minimum_met: false,
            responsiveness_real_source_weight_basis_points: u16::from(
                policy.reliability_source_weights.real_traffic_percent,
            ) * 100,
            responsiveness_monitoring_source_weight_basis_points: u16::from(
                policy.reliability_source_weights.monitoring_percent,
            ) * 100,
            recent_latency_coverage_basis_points: 0,
            recent_responsiveness_basis_points: responsiveness_score,
            historical_observation_count: 0,
            historical_real_sample_count: 0,
            historical_monitoring_sample_count: 0,
            historical_effective_mass_basis_points: 0,
            historical_success_mass_basis_points: 0,
            historical_failure_mass_basis_points: 0,
            historical_minimum_samples: u64::from(
                policy.reliability_sampling.historical_minimum_samples,
            ),
            historical_reliability_minimum_met: false,
            historical_score: reliability_score,
            historical_weight_basis_points: 10_000,
            historical_responsiveness_weight_basis_points: 10_000,
            historical_latency_sample_count: 0,
            historical_latency_effective_mass_basis_points: 0,
            historical_weighted_latency_ms: policy.reliability_sampling.optimistic_latency_ms,
            historical_latency_minimum_met: false,
            historical_real_latency_sample_count: 0,
            historical_monitoring_latency_sample_count: 0,
            historical_real_weighted_latency_ms: policy.reliability_sampling.optimistic_latency_ms,
            historical_monitoring_weighted_latency_ms: policy
                .reliability_sampling
                .optimistic_latency_ms,
            historical_real_latency_minimum_met: false,
            historical_monitoring_latency_minimum_met: false,
            historical_latency_coverage_basis_points: 0,
            historical_responsiveness_basis_points: responsiveness_score,
            historical_age_window_days: 30,
            historical_half_life_days: 1,
            monitoring_source_status: RoutingMonitoringSourceStatus::NoEvidence,
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
    pub(crate) status: RoutingCandidateCapacityStatus,
    pub(crate) max_concurrency: i64,
    pub(crate) in_flight: Option<i64>,
    pub(crate) acquired: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoutingCandidateCapacityStatus {
    Available,
    Exhausted,
    StateUnavailable,
    Unknown,
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
    policy_config: RoutingPolicyConfigV3,
    max_rate_multiplier: Option<f64>,
    routing_group_filter: RoutingGroupFilter,
    candidates: Vec<(CanonicalRoutingCandidate, Option<ResolvedPricingContext>)>,
    scores: &BTreeMap<String, RoutingCandidateScoreSnapshot>,
    score_statuses: &BTreeMap<String, RoutingScoreStatus>,
    planner_exclusion_codes: &BTreeMap<String, Vec<String>>,
    assessment_provenance: &BTreeMap<String, (String, u64, String)>,
    planner_evaluation: RoutingPlannerEvaluationStatus,
    planner_evaluation_code: Option<String>,
    quality_summaries: &BTreeMap<String, QualitySummary>,
    plan_diagnostics: &BTreeMap<String, RoutingCandidatePlanDiagnostics>,
    attempt_diagnostics: &BTreeMap<String, RoutingAttemptCountDiagnostics>,
    circuit_statuses: &[StationKeyCircuitStatus],
    revisions: RoutingWorkspaceRevisionSnapshot,
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
            let score_status = score_statuses
                .get(&candidate.station_key_id)
                .copied()
                .unwrap_or(RoutingScoreStatus::Unavailable);
            let exclusion_codes = planner_exclusion_codes
                .get(&candidate.station_key_id)
                .cloned()
                .unwrap_or_default();
            let provenance = assessment_provenance.get(&candidate.station_key_id);
            candidate_from_canonical(
                candidate,
                pricing,
                &policy_config,
                request,
                generated_at_ms,
                score_details,
                score_status,
                exclusion_codes,
                provenance,
                quality_summaries,
                plan_diagnostics,
                attempt_diagnostics,
                circuit_statuses,
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
    let availability_status = routing_availability_status(&ordered_candidates);
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
        planner_evaluation,
        planner_evaluation_code,
        availability_status,
        runtime_generation_id: revisions.runtime_generation_id,
        policy_revision: revisions.policy_revision,
        quality_revision: revisions.quality_revision,
        health_revision: revisions.health_revision,
        quality_projection_backlog: revisions.quality_projection_backlog,
        quality_projection_lag_seconds: revisions.quality_projection_lag_seconds,
        quality_stale: revisions.quality_stale,
    }
}

fn routing_availability_status(
    candidates: &[RoutingWorkspaceCandidate],
) -> RoutingAvailabilityStatus {
    let potentially_admissible = candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.score_status,
                RoutingScoreStatus::Scored | RoutingScoreStatus::ProbeDiscovery
            )
        })
        .collect::<Vec<_>>();
    if potentially_admissible.iter().any(|candidate| {
        !matches!(
            candidate.capacity.status,
            RoutingCandidateCapacityStatus::Exhausted
                | RoutingCandidateCapacityStatus::StateUnavailable
        )
    }) {
        return RoutingAvailabilityStatus::Available;
    }
    if !potentially_admissible.is_empty()
        && potentially_admissible
            .iter()
            .all(|candidate| candidate.capacity.status == RoutingCandidateCapacityStatus::Exhausted)
    {
        return RoutingAvailabilityStatus::CapacityExhausted;
    }
    if candidates.iter().any(|candidate| {
        candidate.capacity.status == RoutingCandidateCapacityStatus::StateUnavailable
    }) {
        return RoutingAvailabilityStatus::CapacityStateUnavailable;
    }
    RoutingAvailabilityStatus::AllKeysUnavailable
}

fn depleted_rank(value: Option<f64>, status: Option<&str>) -> u8 {
    if crate::models::routing::balance_is_depleted(value, status) {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{
        candidate_matches_group_scope, circuit_diagnostics, depleted_rank,
        RoutingCandidateGroupSnapshot, RoutingCandidatePlanDiagnostics,
        RoutingCandidateScoreGateStatus, RoutingCandidateScoreWindowSnapshot,
    };
    use crate::application::quality_projection::{
        rebuild_quality_summary_v3_at, QualityProjectionConfig, QUALITY_RECENT_WINDOW_MS,
    };
    use crate::application::routing_engine::request::{
        CanonicalRouteRequest, GroupFilterMode, OrderingProfile, RouteKind, RouteRequestClassifier,
        ValidatedLocalRouteSettings,
    };
    use crate::models::{
        proxy::UpstreamApiFormat,
        routing::{CanonicalRoutingCandidate, StationKeyCapabilities},
        routing_observation::{
            EventTimeStatus, FailureAttribution, ObservationOrder, ObservationOutcome,
            ObservationRetryDisposition, ObservationScope, ObservationSource, RecoveryOrigin,
            ResponseOrigin, RoutingObservation, TrafficEquivalence,
        },
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
    fn low_positive_balance_stays_in_the_normal_display_tier() {
        assert_eq!(depleted_rank(Some(4.71), Some("low")), 0);
    }

    #[test]
    fn optimistic_score_details_keep_recent_and_historical_windows_visible() {
        let policy = crate::models::routing_policy::RoutingPolicyConfigV3::default();
        let details = RoutingCandidateScoreWindowSnapshot::optimistic(&policy, 9_500, 9_791);

        assert_eq!(details.recent_observation_count, 0);
        assert_eq!(details.historical_observation_count, 0);
        assert_eq!(
            details.recent_minimum_samples,
            u64::from(policy.reliability_sampling.recent_minimum_samples)
        );
        assert_eq!(
            details.historical_minimum_samples,
            u64::from(policy.reliability_sampling.historical_minimum_samples)
        );
        assert!(!details.recent_reliability_minimum_met);
        assert!(!details.historical_reliability_minimum_met);
        assert_eq!(details.recent_score, 9_500);
        assert_eq!(details.historical_score, 9_500);
        assert_eq!(details.recent_weight_basis_points, 0);
        assert_eq!(details.historical_weight_basis_points, 10_000);
        assert_eq!(
            details.recent_weighted_latency_ms,
            policy.reliability_sampling.optimistic_latency_ms
        );
        assert_eq!(
            details.historical_weighted_latency_ms,
            policy.reliability_sampling.optimistic_latency_ms
        );
    }

    #[test]
    fn score_window_keeps_real_and_monitoring_samples_separate() {
        let now_ms = QUALITY_RECENT_WINDOW_MS;
        let comparability_key = format!("cmp:v1:{}", "a".repeat(64));
        let real = quality_observation(
            "real",
            ObservationSource::RealRequest,
            TrafficEquivalence::ExactRequest,
            Some(comparability_key.clone()),
            now_ms,
        );
        let monitoring = quality_observation(
            "monitoring",
            ObservationSource::ActiveProbe,
            TrafficEquivalence::SameModelShape,
            Some(comparability_key),
            now_ms,
        );
        let summary = rebuild_quality_summary_v3_at(
            "station_key:key-1",
            &[real, monitoring],
            QualityProjectionConfig::default(),
            2,
            now_ms,
        );
        let details: RoutingCandidateScoreWindowSnapshot = (&summary).into();

        assert_eq!(details.recent_real_sample_count, 1);
        assert_eq!(details.recent_monitoring_sample_count, 1);
        assert_eq!(details.recent_observation_count, 2);
        assert_eq!(
            details.monitoring_source_status,
            super::RoutingMonitoringSourceStatus::Comparable
        );
    }

    fn quality_observation(
        id: &str,
        source: ObservationSource,
        traffic_equivalence: TrafficEquivalence,
        comparability_key: Option<String>,
        event_at_ms: i64,
    ) -> RoutingObservation {
        RoutingObservation {
            id: id.to_string(),
            order: ObservationOrder {
                producer_id: "routing-workspace-test".to_string(),
                producer_sequence: 1,
                event_at_ms,
                ingested_at_ms: event_at_ms,
            },
            scope: ObservationScope {
                station_id: Some("station-1".to_string()),
                station_key_id: Some("key-1".to_string()),
                model: Some("model-1".to_string()),
                endpoint_revision: Some(1),
            },
            source,
            traffic_equivalence,
            outcome: ObservationOutcome::Success,
            latency_ms: Some(100),
            evidence_mass_basis_points: 10_000,
            comparability_key,
            correlation_id: id.to_string(),
            attempt_index: 0,
            station_key_lifecycle_revision: 1,
            cluster_finalized: true,
            cluster_expected_attempt_count: 1,
            boundary_crossed: true,
            event_time_status: EventTimeStatus::Valid,
            response_origin: ResponseOrigin::Upstream,
            failure_code: None,
            failure_attribution: FailureAttribution::Key,
            recovery_origin: RecoveryOrigin::Normal,
            retry_disposition: ObservationRetryDisposition::End,
            probe_scope: None,
            probe_state_revision: None,
        }
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

    #[test]
    fn circuit_diagnostics_use_same_tier_closed_baseline_without_exposing_lease_identity() {
        use std::collections::BTreeMap;

        use crate::application::station_key_circuit::StationKeyCircuitStatus;
        use crate::application::{
            routing_engine::tiers::AvailabilityTier, station_key_circuit::StationKeyCircuitState,
        };

        let plans = BTreeMap::from([
            (
                "recovering".to_string(),
                RoutingCandidatePlanDiagnostics {
                    effective_score: 9_100,
                    base_score: 9_000,
                    target_rank: 0,
                    tier: AvailabilityTier::Primary,
                    lifecycle_revision: 2,
                },
            ),
            (
                "closed".to_string(),
                RoutingCandidatePlanDiagnostics {
                    effective_score: 8_800,
                    base_score: 8_800,
                    target_rank: 0,
                    tier: AvailabilityTier::Primary,
                    lifecycle_revision: 1,
                },
            ),
        ]);
        let statuses = vec![StationKeyCircuitStatus {
            station_key_id: "recovering".to_string(),
            lifecycle_revision: 2,
            policy_revision: 1,
            lease_policy: None,
            state: StationKeyCircuitState::HalfOpen {
                state_revision: 7,
                lease_id: Some("secret-lease-id".to_string()),
                lease_revision: 4,
                lease_expires_at_ms: Some(20_000),
                recovery_successes: 1,
                reopen_level: 2,
            },
        }];

        let diagnostics = circuit_diagnostics("recovering", 10_000, &plans, &statuses);
        assert_eq!(
            diagnostics.score_gate_status,
            RoutingCandidateScoreGateStatus::Passed
        );
        assert!(diagnostics.half_open_lease_in_flight);
        assert_eq!(diagnostics.best_closed_effective_score, Some(8_800));
        let serialized = serde_json::to_string(&diagnostics).expect("serialize diagnostics");
        assert!(!serialized.contains("secret-lease-id"));
        assert!(!serialized.contains("leaseId"));
    }

    #[test]
    fn circuit_score_gate_requires_strictly_higher_effective_score() {
        use std::collections::BTreeMap;

        use crate::application::station_key_circuit::StationKeyCircuitStatus;
        use crate::application::{
            routing_engine::tiers::AvailabilityTier, station_key_circuit::StationKeyCircuitState,
        };

        let plans = BTreeMap::from([
            (
                "recovering".to_string(),
                RoutingCandidatePlanDiagnostics {
                    effective_score: 8_800,
                    base_score: 8_700,
                    target_rank: 0,
                    tier: AvailabilityTier::Primary,
                    lifecycle_revision: 2,
                },
            ),
            (
                "closed".to_string(),
                RoutingCandidatePlanDiagnostics {
                    effective_score: 8_800,
                    base_score: 8_800,
                    target_rank: 0,
                    tier: AvailabilityTier::Primary,
                    lifecycle_revision: 1,
                },
            ),
        ]);
        let statuses = vec![StationKeyCircuitStatus {
            station_key_id: "recovering".to_string(),
            lifecycle_revision: 2,
            policy_revision: 1,
            lease_policy: None,
            state: StationKeyCircuitState::Open {
                state_revision: 5,
                opened_at_ms: 1_000,
                cooldown_until_ms: 2_000,
                consecutive_failures: 3,
                reopen_level: 1,
            },
        }];

        let diagnostics = circuit_diagnostics("recovering", 10_000, &plans, &statuses);
        assert_eq!(
            diagnostics.score_gate_status,
            RoutingCandidateScoreGateStatus::Denied
        );
    }
}

fn candidate_from_canonical(
    candidate: CanonicalRoutingCandidate,
    pricing: Option<ResolvedPricingContext>,
    policy_config: &RoutingPolicyConfigV3,
    request: &RouteRequestFacts,
    generated_at_ms: i64,
    score_details: Option<RoutingCandidateScoreSnapshot>,
    score_status: RoutingScoreStatus,
    planner_exclusion_codes: Vec<String>,
    assessment_provenance: Option<&(String, u64, String)>,
    quality_summaries: &BTreeMap<String, QualitySummary>,
    plan_diagnostics: &BTreeMap<String, RoutingCandidatePlanDiagnostics>,
    attempt_diagnostics: &BTreeMap<String, RoutingAttemptCountDiagnostics>,
    circuit_statuses: &[StationKeyCircuitStatus],
) -> RoutingWorkspaceCandidate {
    let quality_scope = format!("station_key:{}", candidate.station_key_id);
    let quality_summary = quality_summaries.get(&quality_scope);
    let plan_diagnostic = plan_diagnostics.get(&candidate.station_key_id);
    let attempt_diagnostic = attempt_diagnostics
        .get(&quality_scope)
        .copied()
        .unwrap_or_default();
    let diagnostics = RoutingCandidateDiagnostics {
        effective_score: plan_diagnostic.map(|value| value.effective_score),
        base_score: plan_diagnostic.map(|value| value.base_score),
        quality: quality_summary.map(Into::into),
        attempts: RoutingCandidateAttemptDiagnostics {
            raw_real_attempt_count: attempt_diagnostic.raw_attempt_count,
            deduplicated_real_request_count: attempt_diagnostic.deduplicated_request_count,
        },
        circuit: circuit_diagnostics(
            &candidate.station_key_id,
            generated_at_ms,
            plan_diagnostics,
            circuit_statuses,
        ),
    };
    let source_snapshot_id = assessment_provenance
        .map(|value| value.0.clone())
        .unwrap_or_else(|| format!("workspace-{generated_at_ms}"));
    let source_fact_version_vector = assessment_provenance
        .map(|value| format!("durable_revision:{};request_context:{}", value.1, value.2))
        .unwrap_or_else(|| {
            format!(
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
            )
        });
    let score_details = score_details.map(|mut details| {
        if let Some(summary) = quality_summary {
            let window_details = Some(summary.into());
            details.reliability.window_details = window_details.clone();
            details.responsiveness.window_details = window_details;
            details.reliability.inputs = vec![
                score_input(
                    "真实流量可靠性",
                    format_basis_points(summary.real_reliability_basis_points),
                ),
                score_input(
                    "真实流量采用权重",
                    format_basis_points(summary.real_source_weight_basis_points),
                ),
                score_input(
                    "监控可靠性",
                    format_basis_points(summary.monitoring_reliability_basis_points),
                ),
                score_input(
                    "监控采用权重",
                    format_basis_points(summary.monitoring_source_weight_basis_points),
                ),
                score_input(
                    "样本不足乐观值",
                    format_basis_points(summary.optimistic_reliability_basis_points),
                ),
            ];
            details.responsiveness.inputs = vec![
                score_input(
                    "近24小时加权平均延迟",
                    format_latency_value(Some(summary.latency.recent_weighted_latency_ms)),
                ),
                score_input(
                    "历史加权平均延迟",
                    format_latency_value(Some(summary.latency.historical_weighted_latency_ms)),
                ),
                score_input(
                    "近24小时样本门槛",
                    if summary.latency.recent_minimum_met {
                        "已达到"
                    } else {
                        "未达到，采用乐观值"
                    },
                ),
                score_input(
                    "历史样本门槛",
                    if summary.latency.historical_minimum_met {
                        "已达到"
                    } else {
                        "未达到，采用乐观值"
                    },
                ),
                score_input(
                    "样本不足乐观延迟",
                    format_latency_value(Some(summary.optimistic_latency_ms)),
                ),
                score_input("延迟上限", "120000 ms"),
            ];
        } else {
            let window_details = Some(RoutingCandidateScoreWindowSnapshot::optimistic(
                policy_config,
                details.reliability.score,
                details.responsiveness.score,
            ));
            details.reliability.window_details = window_details.clone();
            details.responsiveness.window_details = window_details;
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
                score_input("样本不足处理", "使用当前策略乐观可靠性"),
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
                score_input("样本不足处理", "使用当前策略乐观响应时间"),
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
    let capacity_status = if planner_exclusion_codes
        .iter()
        .any(|code| code == "capacity_state_unavailable")
    {
        RoutingCandidateCapacityStatus::StateUnavailable
    } else if capacity_limit > 0 && in_flight.is_some_and(|in_flight| in_flight >= capacity_limit) {
        RoutingCandidateCapacityStatus::Exhausted
    } else if capacity_limit <= 0 || in_flight.is_some() {
        RoutingCandidateCapacityStatus::Available
    } else {
        RoutingCandidateCapacityStatus::Unknown
    };
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
        score_status,
        planner_exclusion_codes,
        assessment_snapshot_id: assessment_provenance.map(|value| value.0.clone()),
        assessment_durable_revision: assessment_provenance.map(|value| value.1),
        assessment_request_context_fingerprint: assessment_provenance.map(|value| value.2.clone()),
        score_details,
        diagnostics: Some(diagnostics),
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
            status: capacity_status,
            max_concurrency: capacity_limit,
            in_flight: in_flight,
            acquired: false,
        },
        source_refs: RoutingCandidateSourceRefs {
            station_key_id: candidate.station_key_id,
            station_id: candidate.station_id,
            endpoint_revision: candidate.station_endpoint_revision,
            snapshot_id: source_snapshot_id,
            fact_version_vector: source_fact_version_vector,
            projector_version: "routing_workspace_canonical_v1".to_string(),
        },
        hard_rejection_codes,
    }
}

fn circuit_diagnostics(
    station_key_id: &str,
    generated_at_ms: i64,
    plan_diagnostics: &BTreeMap<String, RoutingCandidatePlanDiagnostics>,
    circuit_statuses: &[StationKeyCircuitStatus],
) -> RoutingCandidateCircuitDiagnostics {
    let plan = plan_diagnostics.get(station_key_id);
    let status = circuit_statuses.iter().find(|status| {
        status.station_key_id == station_key_id
            && plan.is_none_or(|plan| status.lifecycle_revision == plan.lifecycle_revision)
    });
    let now_ms = u64::try_from(generated_at_ms.max(0)).unwrap_or_default();
    let best_closed_effective_score = plan.and_then(|current| {
        plan_diagnostics
            .iter()
            .filter(|(other_key, other)| {
                other_key.as_str() != station_key_id
                    && other.target_rank == current.target_rank
                    && other.tier == current.tier
                    && circuit_statuses
                        .iter()
                        .find(|status| {
                            status.station_key_id == other_key.as_str()
                                && status.lifecycle_revision == other.lifecycle_revision
                        })
                        .is_none_or(|status| {
                            matches!(status.state, StationKeyCircuitState::Closed { .. })
                        })
            })
            .map(|(_, other)| other.effective_score)
            .max()
    });
    let score_gate = || match plan {
        None => (
            RoutingCandidateScoreGateStatus::Unavailable,
            "candidate_score_unavailable",
        ),
        Some(_) if best_closed_effective_score.is_none() => (
            RoutingCandidateScoreGateStatus::Passed,
            "no_closed_candidate_baseline",
        ),
        Some(plan)
            if best_closed_effective_score.is_some_and(|best| plan.effective_score > best) =>
        {
            (
                RoutingCandidateScoreGateStatus::Passed,
                "higher_than_best_closed_candidate",
            )
        }
        Some(_) => (
            RoutingCandidateScoreGateStatus::Denied,
            "not_higher_than_best_closed_candidate",
        ),
    };

    match status.map(|status| &status.state) {
        Some(StationKeyCircuitState::Closed {
            state_revision,
            consecutive_failures,
            reopen_level,
        }) => RoutingCandidateCircuitDiagnostics {
            state: RoutingCandidateCircuitState::Closed,
            state_revision: Some(*state_revision),
            lifecycle_revision: status.map(|value| value.lifecycle_revision),
            consecutive_failures: Some(*consecutive_failures),
            reopen_level: *reopen_level,
            cooldown_until_ms: None,
            cooldown_remaining_ms: None,
            half_open_lease_in_flight: false,
            half_open_lease_expires_at_ms: None,
            recovery_successes: None,
            score_gate_status: RoutingCandidateScoreGateStatus::NotApplicable,
            score_gate_reason: "circuit_closed".to_string(),
            best_closed_effective_score,
        },
        Some(StationKeyCircuitState::Open {
            state_revision,
            cooldown_until_ms,
            consecutive_failures,
            reopen_level,
            ..
        }) => {
            let (score_gate_status, score_gate_reason) = if *cooldown_until_ms > now_ms {
                (
                    RoutingCandidateScoreGateStatus::WaitingCooldown,
                    "cooldown_active",
                )
            } else {
                score_gate()
            };
            RoutingCandidateCircuitDiagnostics {
                state: RoutingCandidateCircuitState::Open,
                state_revision: Some(*state_revision),
                lifecycle_revision: status.map(|value| value.lifecycle_revision),
                consecutive_failures: Some(*consecutive_failures),
                reopen_level: *reopen_level,
                cooldown_until_ms: Some(*cooldown_until_ms),
                cooldown_remaining_ms: Some(cooldown_until_ms.saturating_sub(now_ms)),
                half_open_lease_in_flight: false,
                half_open_lease_expires_at_ms: None,
                recovery_successes: None,
                score_gate_status,
                score_gate_reason: score_gate_reason.to_string(),
                best_closed_effective_score,
            }
        }
        Some(StationKeyCircuitState::HalfOpen {
            state_revision,
            lease_id,
            lease_expires_at_ms,
            recovery_successes,
            reopen_level,
            ..
        }) => {
            let (score_gate_status, score_gate_reason) = if lease_id.is_some() {
                (
                    RoutingCandidateScoreGateStatus::Passed,
                    "half_open_lease_in_flight",
                )
            } else {
                score_gate()
            };
            RoutingCandidateCircuitDiagnostics {
                state: RoutingCandidateCircuitState::HalfOpen,
                state_revision: Some(*state_revision),
                lifecycle_revision: status.map(|value| value.lifecycle_revision),
                consecutive_failures: None,
                reopen_level: *reopen_level,
                cooldown_until_ms: None,
                cooldown_remaining_ms: None,
                half_open_lease_in_flight: lease_id.is_some(),
                half_open_lease_expires_at_ms: *lease_expires_at_ms,
                recovery_successes: Some(*recovery_successes),
                score_gate_status,
                score_gate_reason: score_gate_reason.to_string(),
                best_closed_effective_score,
            }
        }
        None => RoutingCandidateCircuitDiagnostics {
            state: RoutingCandidateCircuitState::Closed,
            state_revision: None,
            lifecycle_revision: plan.map(|value| value.lifecycle_revision),
            consecutive_failures: Some(0),
            reopen_level: 0,
            cooldown_until_ms: None,
            cooldown_remaining_ms: None,
            half_open_lease_in_flight: false,
            half_open_lease_expires_at_ms: None,
            recovery_successes: None,
            score_gate_status: RoutingCandidateScoreGateStatus::NotApplicable,
            score_gate_reason: "default_closed_state".to_string(),
            best_closed_effective_score,
        },
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
