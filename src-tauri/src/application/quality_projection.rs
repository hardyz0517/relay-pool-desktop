use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::models::routing_observation::{
    EventTimeStatus, ObservationOutcome, ObservationSource, RoutingObservation,
};

pub(crate) const QUALITY_PROJECTOR_VERSION: &str = "routing_quality_v3";
#[cfg(test)]
const LEGACY_QUALITY_PROJECTOR_VERSION: &str = "routing_quality_legacy_v2";
#[cfg(test)]
const RECENT_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;
#[cfg(test)]
const HISTORY_WINDOW_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
// Legacy v2 compatibility projector only; v3 uses the fixed-point 24-hour
// historical half-life in `quality_weight_fixed`.
#[cfg(test)]
const HISTORY_HALF_LIFE_MS: f64 = 7.0 * 24.0 * 60.0 * 60.0 * 1_000.0;

/// Fixed-point scales used by the v3 projector.  No floating point operation
/// is used by the production v3 path; the old test-only projector below is
/// retained solely for replaying pre-v3 fixtures.
pub(crate) const QUALITY_WEIGHT_SCALE: u64 = 1_000_000;
/// Recent/history mixing uses a finer scale than the persisted score fields;
/// diagnostics are converted to basis points only after the calculation.
pub(crate) const QUALITY_RATIO_SCALE: u64 = 1_000_000;
pub(crate) const QUALITY_LATENCY_SCALE: u64 = 1_000;
const BASIS_POINTS_SCALE: u64 = 10_000;
pub(crate) const RESPONSIVENESS_SCORE_CAP_MS: u32 = 120_000;
pub(crate) const QUALITY_RECENT_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;
pub(crate) const QUALITY_HISTORY_WINDOW_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QualityProjectionConfig {
    /// Revision of the staged quality policy used for this projection.  This
    /// is part of the read-model identity and must not be inferred from the
    /// observation cursor.
    pub(crate) quality_policy_revision: u64,
    pub(crate) recent_minimum_samples: u64,
    pub(crate) historical_minimum_samples: u64,
    pub(crate) optimistic_reliability_basis_points: u16,
    pub(crate) optimistic_latency_ms: u32,
    pub(crate) real_traffic_weight_basis_points: u16,
    pub(crate) monitoring_weight_basis_points: u16,
    /// Eligibility is a fact of the request/probe shape, not of sample count.
    /// The default keeps key quality sortable before its first observation.
    pub(crate) real_source_eligible: bool,
    pub(crate) monitoring_source_eligible: bool,
    /// When supplied, observations from a previous key binding are excluded.
    pub(crate) current_lifecycle_revision: Option<u64>,
}

impl Default for QualityProjectionConfig {
    fn default() -> Self {
        Self {
            quality_policy_revision: 1,
            recent_minimum_samples: 5,
            historical_minimum_samples: 15,
            optimistic_reliability_basis_points: 9_500,
            optimistic_latency_ms: 2_500,
            real_traffic_weight_basis_points: 7_000,
            monitoring_weight_basis_points: 3_000,
            real_source_eligible: true,
            monitoring_source_eligible: true,
            current_lifecycle_revision: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct QualitySourceWindowSummary {
    pub(crate) sample_count: u64,
    pub(crate) effective_weight: u64,
    pub(crate) success_weight: u64,
    pub(crate) failure_weight: u64,
    pub(crate) reliability_basis_points: u16,
    pub(crate) minimum_met: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct QualitySourceSummary {
    pub(crate) eligible: bool,
    pub(crate) effective_weight_basis_points: u16,
    pub(crate) recent: QualitySourceWindowSummary,
    pub(crate) historical: QualitySourceWindowSummary,
    pub(crate) blended_reliability_basis_points: u16,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct QualityLatencySourceSummary {
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct QualityLatencySummary {
    pub(crate) recent_sample_count: u64,
    pub(crate) recent_effective_weight: u64,
    pub(crate) recent_weighted_latency_ms: u32,
    pub(crate) recent_minimum_met: bool,
    pub(crate) historical_sample_count: u64,
    pub(crate) historical_effective_weight: u64,
    pub(crate) historical_weighted_latency_ms: u32,
    pub(crate) historical_minimum_met: bool,
    pub(crate) blended_weighted_latency_ms: u32,
    pub(crate) real_source_weight_basis_points: u16,
    pub(crate) monitoring_source_weight_basis_points: u16,
    pub(crate) real_source: QualityLatencySourceSummary,
    pub(crate) monitoring_source: QualityLatencySourceSummary,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MonitoringSourceStatus {
    Comparable,
    #[default]
    NoEvidence,
    Incomparable,
    WeightZero,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct QualitySummary {
    pub(crate) scope: String,
    pub(crate) projector_version: String,
    pub(crate) observation_count: u64,
    pub(crate) effective_mass_basis_points: u64,
    pub(crate) success_mass_basis_points: u64,
    pub(crate) failure_mass_basis_points: u64,
    pub(crate) reliability_basis_points: u16,
    pub(crate) latency_coverage_basis_points: u16,
    pub(crate) p95_latency_ms: Option<u32>,
    #[serde(default)]
    pub(crate) responsiveness_basis_points: u16,
    #[serde(default)]
    pub(crate) recent_observation_count: u64,
    #[serde(default)]
    pub(crate) recent_effective_mass_basis_points: u64,
    #[serde(default)]
    pub(crate) recent_success_mass_basis_points: u64,
    #[serde(default)]
    pub(crate) recent_failure_mass_basis_points: u64,
    #[serde(default)]
    pub(crate) recent_reliability_basis_points: u16,
    #[serde(default)]
    pub(crate) recent_reliability_weight_basis_points: u16,
    #[serde(default)]
    pub(crate) recent_responsiveness_weight_basis_points: u16,
    #[serde(default)]
    pub(crate) recent_p95_latency_ms: Option<u32>,
    #[serde(default)]
    pub(crate) recent_latency_coverage_basis_points: u16,
    #[serde(default)]
    pub(crate) recent_responsiveness_basis_points: u16,
    #[serde(default)]
    pub(crate) historical_observation_count: u64,
    #[serde(default)]
    pub(crate) historical_effective_mass_basis_points: u64,
    #[serde(default)]
    pub(crate) historical_success_mass_basis_points: u64,
    #[serde(default)]
    pub(crate) historical_failure_mass_basis_points: u64,
    #[serde(default)]
    pub(crate) historical_reliability_basis_points: u16,
    #[serde(default)]
    pub(crate) historical_reliability_weight_basis_points: u16,
    #[serde(default)]
    pub(crate) historical_responsiveness_weight_basis_points: u16,
    #[serde(default)]
    pub(crate) historical_p95_latency_ms: Option<u32>,
    #[serde(default)]
    pub(crate) historical_latency_coverage_basis_points: u16,
    #[serde(default)]
    pub(crate) historical_responsiveness_basis_points: u16,
    #[serde(default)]
    pub(crate) historical_age_window_days: u16,
    #[serde(default)]
    pub(crate) historical_half_life_days: u16,
    pub(crate) last_event_at_ms: Option<i64>,
    pub(crate) checkpoint_sequence: u64,
    #[serde(default)]
    pub(crate) quality_policy_revision: u64,
    #[serde(default)]
    pub(crate) algorithm_version: String,
    #[serde(default)]
    pub(crate) weight_scale: u64,
    #[serde(default)]
    pub(crate) ratio_scale: u64,
    #[serde(default)]
    pub(crate) recent_minimum_samples: u64,
    #[serde(default)]
    pub(crate) historical_minimum_samples: u64,
    #[serde(default)]
    pub(crate) optimistic_reliability_basis_points: u16,
    #[serde(default)]
    pub(crate) optimistic_latency_ms: u32,
    #[serde(default)]
    pub(crate) real_reliability_basis_points: u16,
    #[serde(default)]
    pub(crate) monitoring_reliability_basis_points: u16,
    #[serde(default)]
    pub(crate) real_source_weight_basis_points: u16,
    #[serde(default)]
    pub(crate) monitoring_source_weight_basis_points: u16,
    #[serde(default)]
    pub(crate) real_source_eligible: bool,
    #[serde(default)]
    pub(crate) monitoring_source_eligible: bool,
    #[serde(default)]
    pub(crate) monitoring_source_status: MonitoringSourceStatus,
    #[serde(default)]
    pub(crate) real_source: QualitySourceSummary,
    #[serde(default)]
    pub(crate) monitoring_source: QualitySourceSummary,
    #[serde(default)]
    pub(crate) latency: QualityLatencySummary,
    #[serde(default)]
    pub(crate) quality_unavailable: bool,
    #[serde(default)]
    pub(crate) quality_basis: String,
    #[serde(default)]
    pub(crate) last_real_route_sample_at_ms: Option<i64>,
    #[serde(default)]
    /// Three-valued freshness result: `true`, `false` or `unknown`.
    pub(crate) idle_real_route_sample: String,
    #[serde(default)]
    pub(crate) event_time_diagnostic: Option<String>,
    #[serde(default)]
    pub(crate) selected_observation_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct BetaPrior {
    pub(crate) alpha_basis_points: u64,
    pub(crate) beta_basis_points: u64,
    pub(crate) minimum_effective_mass_basis_points: u64,
}

#[cfg(test)]
impl Default for BetaPrior {
    fn default() -> Self {
        Self {
            alpha_basis_points: 2_000,
            beta_basis_points: 2_000,
            minimum_effective_mass_basis_points: 10_000,
        }
    }
}

#[cfg(test)]
pub(crate) fn rebuild_quality_summary(
    scope: &str,
    observations: &[RoutingObservation],
    prior: BetaPrior,
) -> QualitySummary {
    let checkpoint_sequence = observations
        .iter()
        .filter(|observation| observation_scope(observation) == scope)
        .map(|observation| observation.order.producer_sequence)
        .max()
        .unwrap_or(0);
    let now_ms = observations
        .iter()
        .filter(|observation| observation_scope(observation) == scope)
        .map(|observation| observation.order.event_at_ms)
        .max()
        .unwrap_or(0);
    rebuild_quality_summary_at(scope, observations, prior, checkpoint_sequence, now_ms)
}

#[cfg(test)]
pub(crate) fn rebuild_quality_summary_with_checkpoint(
    scope: &str,
    observations: &[RoutingObservation],
    prior: BetaPrior,
    checkpoint_sequence: u64,
) -> QualitySummary {
    let now_ms = observations
        .iter()
        .filter(|observation| observation_scope(observation) == scope)
        .map(|observation| observation.order.event_at_ms)
        .max()
        .unwrap_or(0);
    rebuild_quality_summary_at(scope, observations, prior, checkpoint_sequence, now_ms)
}

#[cfg(test)]
pub(crate) fn rebuild_quality_summary_at(
    scope: &str,
    observations: &[RoutingObservation],
    prior: BetaPrior,
    checkpoint_sequence: u64,
    now_ms: i64,
) -> QualitySummary {
    let mut ordered = observations
        .iter()
        .filter(|observation| observation_scope(observation) == scope)
        .collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.order
            .event_at_ms
            .cmp(&right.order.event_at_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut seen = BTreeSet::new();
    ordered.retain(|observation| seen.insert(observation.id.as_str()));

    let mut recent = WindowAccumulator::default();
    let mut historical = WindowAccumulator::default();
    let mut last_event_at_ms: Option<i64> = None;
    for observation in &ordered {
        let age_ms = now_ms.saturating_sub(observation.order.event_at_ms).max(0);
        let accumulator = if age_ms <= RECENT_WINDOW_MS {
            Some((&mut recent, 1.0))
        } else if age_ms <= HISTORY_WINDOW_MS {
            Some((
                &mut historical,
                2_f64.powf(-(age_ms as f64) / HISTORY_HALF_LIFE_MS),
            ))
        } else {
            None
        };
        if let Some((accumulator, decay)) = accumulator {
            accumulator.add(observation, decay);
            if is_quality_outcome(&observation.outcome) {
                last_event_at_ms = Some(
                    last_event_at_ms.map_or(observation.order.event_at_ms, |value| {
                        value.max(observation.order.event_at_ms)
                    }),
                );
            }
        }
    }
    let recent_reliability = posterior(&recent, prior);
    let historical_reliability = posterior(&historical, prior);
    let recent_weight = recent_weight(recent.quality_observation_count);
    let historical_weight = 10_000_u16.saturating_sub(recent_weight);
    let reliability = blend(
        recent_reliability,
        historical_reliability,
        recent_weight,
        historical_weight,
    );
    let recent_responsiveness = responsiveness_score(recent.p95_latency_ms(), 120_000);
    let historical_responsiveness = responsiveness_score(historical.p95_latency_ms(), 120_000);
    let recent_responsiveness_weight = latency_weight(recent.latency_sample_count);
    let historical_responsiveness_weight = 10_000_u16.saturating_sub(recent_responsiveness_weight);
    let responsiveness = blend(
        recent_responsiveness,
        historical_responsiveness,
        recent_responsiveness_weight,
        historical_responsiveness_weight,
    );
    let effective_mass = recent
        .effective_mass()
        .saturating_add(historical.effective_mass());
    let success_mass = recent.success_mass.saturating_add(historical.success_mass);
    let failure_mass = recent.failure_mass.saturating_add(historical.failure_mass);
    let p95_latency_ms = recent
        .p95_latency_ms()
        .or_else(|| historical.p95_latency_ms());
    QualitySummary {
        scope: scope.to_string(),
        // Compatibility projector output must never be accepted by the v3
        // quality store or planner read path.
        projector_version: LEGACY_QUALITY_PROJECTOR_VERSION.to_string(),
        observation_count: recent
            .observation_count
            .saturating_add(historical.observation_count),
        effective_mass_basis_points: effective_mass,
        success_mass_basis_points: success_mass,
        failure_mass_basis_points: failure_mass,
        reliability_basis_points: conservative(reliability, effective_mass, prior),
        latency_coverage_basis_points: coverage(&recent, &historical),
        p95_latency_ms,
        responsiveness_basis_points: responsiveness,
        recent_observation_count: recent.observation_count,
        recent_effective_mass_basis_points: recent.effective_mass(),
        recent_success_mass_basis_points: recent.success_mass,
        recent_failure_mass_basis_points: recent.failure_mass,
        recent_reliability_basis_points: conservative(
            recent_reliability,
            recent.effective_mass(),
            prior,
        ),
        recent_reliability_weight_basis_points: recent_weight,
        recent_responsiveness_weight_basis_points: recent_responsiveness_weight,
        recent_p95_latency_ms: recent.p95_latency_ms(),
        recent_latency_coverage_basis_points: recent.coverage(),
        recent_responsiveness_basis_points: recent_responsiveness,
        historical_observation_count: historical.observation_count,
        historical_effective_mass_basis_points: historical.effective_mass(),
        historical_success_mass_basis_points: historical.success_mass,
        historical_failure_mass_basis_points: historical.failure_mass,
        historical_reliability_basis_points: conservative(
            historical_reliability,
            historical.effective_mass(),
            prior,
        ),
        historical_reliability_weight_basis_points: historical_weight,
        historical_responsiveness_weight_basis_points: historical_responsiveness_weight,
        historical_p95_latency_ms: historical.p95_latency_ms(),
        historical_latency_coverage_basis_points: historical.coverage(),
        historical_responsiveness_basis_points: historical_responsiveness,
        historical_age_window_days: 30,
        historical_half_life_days: 7,
        last_event_at_ms,
        checkpoint_sequence,
        quality_policy_revision: 0,
        algorithm_version: "legacy_quality_v2".to_string(),
        weight_scale: 0,
        ratio_scale: 0,
        recent_minimum_samples: 0,
        historical_minimum_samples: 0,
        optimistic_reliability_basis_points: 0,
        optimistic_latency_ms: 0,
        real_reliability_basis_points: 0,
        monitoring_reliability_basis_points: 0,
        real_source_weight_basis_points: 0,
        monitoring_source_weight_basis_points: 0,
        real_source_eligible: false,
        monitoring_source_eligible: false,
        monitoring_source_status: MonitoringSourceStatus::Disabled,
        real_source: QualitySourceSummary::default(),
        monitoring_source: QualitySourceSummary::default(),
        latency: QualityLatencySummary::default(),
        quality_unavailable: false,
        quality_basis: "legacy".to_string(),
        last_real_route_sample_at_ms: last_event_at_ms,
        idle_real_route_sample: "unknown".to_string(),
        event_time_diagnostic: None,
        selected_observation_ids: Vec::new(),
    }
}

#[derive(Debug, Default)]
#[cfg(test)]
struct WindowAccumulator {
    observation_count: u64,
    quality_observation_count: u64,
    success_mass: u64,
    failure_mass: u64,
    latency_samples: Vec<(u32, u64)>,
    latency_sample_count: u64,
    latency_mass: u64,
}

#[cfg(test)]
impl WindowAccumulator {
    fn add(&mut self, observation: &RoutingObservation, decay: f64) {
        self.observation_count = self.observation_count.saturating_add(1);
        if !is_quality_outcome(&observation.outcome) {
            return;
        }
        self.quality_observation_count = self.quality_observation_count.saturating_add(1);
        let mass = ((u64::from(observation.evidence_mass_basis_points) as f64) * decay)
            .round()
            .max(1.0) as u64;
        match observation.outcome {
            ObservationOutcome::Success => {
                self.success_mass = self.success_mass.saturating_add(mass)
            }
            ObservationOutcome::CredentialFailure
            | ObservationOutcome::EndpointFailure
            | ObservationOutcome::ModelFailure
            | ObservationOutcome::RateLimited
            | ObservationOutcome::Timeout => {
                self.failure_mass = self.failure_mass.saturating_add(mass)
            }
            ObservationOutcome::Cancelled | ObservationOutcome::Unknown => {}
        }
        if let Some(latency) = observation.latency_ms {
            self.latency_samples.push((latency, mass));
            self.latency_sample_count = self.latency_sample_count.saturating_add(1);
            self.latency_mass = self.latency_mass.saturating_add(mass);
        }
    }

    fn effective_mass(&self) -> u64 {
        self.success_mass.saturating_add(self.failure_mass)
    }

    fn coverage(&self) -> u16 {
        if self.effective_mass() == 0 {
            0
        } else {
            (self.latency_mass.saturating_mul(10_000) / self.effective_mass()).min(10_000) as u16
        }
    }

    fn p95_latency_ms(&self) -> Option<u32> {
        let mut samples = self.latency_samples.clone();
        samples.sort_unstable_by_key(|(latency, _)| *latency);
        percentile95(&samples)
    }
}

fn is_quality_outcome(outcome: &ObservationOutcome) -> bool {
    matches!(
        outcome,
        ObservationOutcome::Success
            | ObservationOutcome::CredentialFailure
            | ObservationOutcome::EndpointFailure
            | ObservationOutcome::ModelFailure
            | ObservationOutcome::RateLimited
            | ObservationOutcome::Timeout
    )
}

#[cfg(test)]
fn posterior(window: &WindowAccumulator, prior: BetaPrior) -> u16 {
    let denominator = prior
        .alpha_basis_points
        .saturating_add(prior.beta_basis_points)
        .saturating_add(window.effective_mass());
    if denominator == 0 {
        return 5_000;
    }
    ((window.success_mass.saturating_add(prior.alpha_basis_points)).saturating_mul(10_000)
        / denominator)
        .min(10_000) as u16
}

#[cfg(test)]
fn conservative(value: u16, effective_mass: u64, prior: BetaPrior) -> u16 {
    if effective_mass < prior.minimum_effective_mass_basis_points {
        value.min(7_500)
    } else {
        value
    }
}

#[cfg(test)]
fn recent_weight(count: u64) -> u16 {
    if count == 0 {
        0
    } else {
        ((0.25_f64 + 0.45_f64 * (count as f64 / 10.0).min(1.0)) * 10_000.0).round() as u16
    }
}

#[cfg(test)]
fn latency_weight(count: u64) -> u16 {
    if count == 0 {
        0
    } else {
        ((0.25_f64 + 0.45_f64 * (count as f64 / 10.0).min(1.0)) * 10_000.0).round() as u16
    }
}

#[cfg(test)]
fn blend(recent: u16, historical: u16, recent_weight: u16, historical_weight: u16) -> u16 {
    ((u64::from(recent) * u64::from(recent_weight)
        + u64::from(historical) * u64::from(historical_weight))
        / 10_000)
        .min(10_000) as u16
}

#[cfg(test)]
fn responsiveness_score(p95: Option<u32>, cap_ms: u32) -> u16 {
    let Some(latency) = p95 else {
        return 5_000;
    };
    if cap_ms == 0 {
        return 5_000;
    }
    ((u64::from(cap_ms.saturating_sub(latency.min(cap_ms))) * 10_000) / u64::from(cap_ms)) as u16
}

#[cfg(test)]
fn coverage(recent: &WindowAccumulator, historical: &WindowAccumulator) -> u16 {
    let effective = recent
        .effective_mass()
        .saturating_add(historical.effective_mass());
    if effective == 0 {
        0
    } else {
        recent
            .latency_mass
            .saturating_add(historical.latency_mass)
            .saturating_mul(10_000)
            .checked_div(effective)
            .unwrap_or(0)
            .min(10_000) as u16
    }
}

fn observation_scope(observation: &RoutingObservation) -> String {
    observation
        .scope
        .station_key_id
        .as_deref()
        .map(|id| format!("station_key:{id}"))
        .or_else(|| {
            observation
                .scope
                .station_id
                .as_deref()
                .map(|id| format!("station:{id}"))
        })
        .or_else(|| {
            observation
                .scope
                .model
                .as_deref()
                .map(|model| format!("model:{model}"))
        })
        .unwrap_or_else(|| "global".into())
}

#[cfg(test)]
fn percentile95(values: &[(u32, u64)]) -> Option<u32> {
    if values.is_empty() {
        return None;
    }
    let total_mass = values.iter().map(|(_, mass)| *mass).sum::<u64>();
    if total_mass == 0 {
        return None;
    }
    let target = total_mass.saturating_mul(95).div_ceil(100);
    let mut cumulative = 0_u64;
    values.iter().find_map(|(latency, mass)| {
        cumulative = cumulative.saturating_add(*mass);
        (cumulative >= target).then_some(*latency)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct V3WindowResult {
    sample_count: u64,
    effective_weight: u64,
    success_weight: u64,
    failure_weight: u64,
    latency_sample_count: u64,
    latency_weight: u64,
    latency_sum: u128,
}

impl Default for V3WindowResult {
    fn default() -> Self {
        Self {
            sample_count: 0,
            effective_weight: 0,
            success_weight: 0,
            failure_weight: 0,
            latency_sample_count: 0,
            latency_weight: 0,
            latency_sum: 0,
        }
    }
}

impl V3WindowResult {
    fn add(&mut self, observation: &RoutingObservation, weight: u64) {
        if !is_quality_outcome(&observation.outcome) || !observation.boundary_crossed {
            return;
        }
        self.sample_count = self.sample_count.saturating_add(1);
        self.effective_weight = self.effective_weight.saturating_add(weight);
        match observation.outcome {
            ObservationOutcome::Success => {
                self.success_weight = self.success_weight.saturating_add(weight);
                if let Some(latency) = observation.latency_ms {
                    self.latency_sample_count = self.latency_sample_count.saturating_add(1);
                    self.latency_weight = self.latency_weight.saturating_add(weight);
                    self.latency_sum = self
                        .latency_sum
                        .saturating_add(u128::from(latency) * u128::from(weight));
                }
            }
            ObservationOutcome::CredentialFailure
            | ObservationOutcome::EndpointFailure
            | ObservationOutcome::ModelFailure
            | ObservationOutcome::RateLimited
            | ObservationOutcome::Timeout => {
                self.failure_weight = self.failure_weight.saturating_add(weight);
            }
            ObservationOutcome::Cancelled | ObservationOutcome::Unknown => {}
        }
    }

    fn reliability(&self, minimum_samples: u64, optimistic: u16) -> u16 {
        if self.sample_count < minimum_samples || self.effective_weight == 0 {
            optimistic
        } else {
            ratio_bp(self.success_weight, self.effective_weight)
        }
    }

    fn latency_scaled(&self, minimum_samples: u64, optimistic: u32) -> u64 {
        if self.latency_sample_count < minimum_samples || self.latency_weight == 0 {
            return u64::from(optimistic).saturating_mul(QUALITY_LATENCY_SCALE);
        }
        let numerator = self
            .latency_sum
            .saturating_mul(u128::from(QUALITY_LATENCY_SCALE));
        let value =
            (numerator + u128::from(self.latency_weight / 2)) / u128::from(self.latency_weight);
        u64::try_from(value.min(u128::from(u64::MAX))).unwrap_or(u64::MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct V3SourceResult {
    recent: V3WindowResult,
    historical: V3WindowResult,
    last_valid_event_at_ms: Option<i64>,
    had_invalid_event_time: bool,
    selected_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LatencyResult {
    recent_scaled: u64,
    historical_scaled: u64,
    blended_scaled: u64,
    recent_ratio: u64,
    recent_ready: bool,
    historical_ready: bool,
}

fn source_latency(source: &V3SourceResult, config: &QualityProjectionConfig) -> LatencyResult {
    let recent_scaled = source
        .recent
        .latency_scaled(config.recent_minimum_samples, config.optimistic_latency_ms);
    let historical_scaled = source.historical.latency_scaled(
        config.historical_minimum_samples,
        config.optimistic_latency_ms,
    );
    let recent_ratio = recent_ratio_fixed(source.recent.latency_sample_count);
    LatencyResult {
        recent_scaled,
        historical_scaled,
        blended_scaled: blend_scaled_u64(recent_scaled, historical_scaled, recent_ratio),
        recent_ratio,
        recent_ready: source.recent.latency_sample_count >= config.recent_minimum_samples,
        historical_ready: source.historical.latency_sample_count
            >= config.historical_minimum_samples,
    }
}

fn mix_latency(
    real: LatencyResult,
    monitoring: LatencyResult,
    real_weight_basis_points: u16,
    monitoring_weight_basis_points: u16,
    optimistic_latency_ms: u32,
) -> LatencyResult {
    let total = u32::from(real_weight_basis_points)
        .saturating_add(u32::from(monitoring_weight_basis_points));
    if total == 0 {
        let optimistic = u64::from(optimistic_latency_ms).saturating_mul(QUALITY_LATENCY_SCALE);
        return LatencyResult {
            recent_scaled: optimistic,
            historical_scaled: optimistic,
            blended_scaled: optimistic,
            recent_ratio: 0,
            recent_ready: false,
            historical_ready: false,
        };
    }

    let effective_real = u16::try_from(
        (u32::from(real_weight_basis_points) * u32::from(BASIS_POINTS_SCALE as u16) + total / 2)
            / total,
    )
    .unwrap_or(0);
    let source_ratio = u64::from(effective_real).saturating_mul(QUALITY_RATIO_SCALE / 10_000);
    let recent_ratio = blend_scaled_u64(real.recent_ratio, monitoring.recent_ratio, source_ratio);

    LatencyResult {
        recent_scaled: blend_scaled_u64(real.recent_scaled, monitoring.recent_scaled, source_ratio),
        historical_scaled: blend_scaled_u64(
            real.historical_scaled,
            monitoring.historical_scaled,
            source_ratio,
        ),
        blended_scaled: blend_scaled_u64(
            blend_scaled_u64(real.recent_scaled, monitoring.recent_scaled, source_ratio),
            blend_scaled_u64(
                real.historical_scaled,
                monitoring.historical_scaled,
                source_ratio,
            ),
            recent_ratio,
        ),
        recent_ratio,
        recent_ready: (effective_real == 0 || real.recent_ready)
            && (effective_real == 10_000 || monitoring.recent_ready),
        historical_ready: (effective_real == 0 || real.historical_ready)
            && (effective_real == 10_000 || monitoring.historical_ready),
    }
}

fn quality_latency_source_summary(
    source: &V3SourceResult,
    latency: LatencyResult,
) -> QualityLatencySourceSummary {
    QualityLatencySourceSummary {
        recent_sample_count: source.recent.latency_sample_count,
        recent_effective_weight: source.recent.latency_weight,
        recent_weighted_latency_ms: scaled_latency_ms(latency.recent_scaled),
        recent_minimum_met: latency.recent_ready,
        historical_sample_count: source.historical.latency_sample_count,
        historical_effective_weight: source.historical.latency_weight,
        historical_weighted_latency_ms: scaled_latency_ms(latency.historical_scaled),
        historical_minimum_met: latency.historical_ready,
        blended_weighted_latency_ms: scaled_latency_ms(latency.blended_scaled),
    }
}

fn combined_latency_coverage(recent: &V3WindowResult, historical: &V3WindowResult) -> u16 {
    let effective_weight = recent
        .effective_weight
        .saturating_add(historical.effective_weight);
    let latency_weight = recent
        .latency_weight
        .saturating_add(historical.latency_weight);
    if effective_weight == 0 {
        0
    } else {
        u16::try_from(
            (u128::from(latency_weight) * u128::from(BASIS_POINTS_SCALE)
                / u128::from(effective_weight))
            .min(u128::from(BASIS_POINTS_SCALE)),
        )
        .unwrap_or(10_000)
    }
}

impl Default for V3SourceResult {
    fn default() -> Self {
        Self {
            recent: V3WindowResult::default(),
            historical: V3WindowResult::default(),
            last_valid_event_at_ms: None,
            had_invalid_event_time: false,
            selected_count: 0,
        }
    }
}

/// Rebuild one key's v3 quality summary from immutable observations.  This is
/// the sole production owner of the reliability/latency math.  Callers pass a
/// frozen evaluation timestamp so retries, replay and read-model refreshes
/// receive the same result.
pub(crate) fn rebuild_quality_summary_v3_at(
    scope: &str,
    observations: &[RoutingObservation],
    config: QualityProjectionConfig,
    checkpoint_sequence: u64,
    evaluation_at_ms: i64,
) -> QualitySummary {
    let evaluation_at_ms = evaluation_at_ms.max(0);
    let current_revision = config.current_lifecycle_revision;
    let in_current_scope = |observation: &RoutingObservation| {
        observation_scope(observation) == scope
            && observation.scope.station_key_id.is_some()
            && current_revision
                .is_none_or(|revision| observation.station_key_lifecycle_revision == revision)
    };
    let monitoring_has_observation = observations.iter().any(|observation| {
        in_current_scope(observation)
            && matches!(observation.source, ObservationSource::ActiveProbe)
            && !matches!(
                observation.traffic_equivalence,
                crate::models::routing_observation::TrafficEquivalence::Anonymous
            )
            && observation.boundary_crossed
            && is_quality_outcome(&observation.outcome)
            && matches!(observation.event_time_status, EventTimeStatus::Valid)
            && observation.order.event_at_ms <= evaluation_at_ms
            && !observation.correlation_id.is_empty()
    });
    let monitoring_source_status = if !config.monitoring_source_eligible {
        MonitoringSourceStatus::Disabled
    } else if config.monitoring_weight_basis_points == 0 {
        MonitoringSourceStatus::WeightZero
    } else if !monitoring_has_observation {
        MonitoringSourceStatus::NoEvidence
    } else {
        MonitoringSourceStatus::Comparable
    };
    // Reliability is scored at station-key scope. A key can serve multiple
    // models, so model/request comparability commitments are diagnostic only;
    // they must not prevent a valid monitoring result from contributing to the
    // monitoring source. A configured source with no evidence still keeps its
    // configured weight and uses the optimistic value.
    let monitoring_source_eligible = config.monitoring_source_eligible;
    let mut clusters: BTreeMap<(String, String, u64, String), Vec<&RoutingObservation>> =
        BTreeMap::new();
    let mut had_invalid_real_event_time = false;
    for observation in observations.iter().filter(|observation| {
        in_current_scope(observation)
            && !matches!(observation.source, ObservationSource::Administrative)
            && !matches!(
                observation.traffic_equivalence,
                crate::models::routing_observation::TrafficEquivalence::Anonymous
            )
    }) {
        if matches!(observation.source, ObservationSource::RealRequest)
            && !matches!(observation.event_time_status, EventTimeStatus::Valid)
        {
            had_invalid_real_event_time = true;
        }
        // Empty correlation IDs are legacy/unclustered rows.  Keeping them in
        // audit is safe, but treating each retry as an independent sample is
        // precisely the over-counting bug v3 is designed to remove.
        if observation.correlation_id.is_empty() {
            continue;
        }
        let source = match observation.source {
            ObservationSource::RealRequest => "real_request",
            ObservationSource::ActiveProbe => "active_probe",
            ObservationSource::Administrative => continue,
        };
        let key = (
            source.to_string(),
            observation.scope.station_key_id.clone().unwrap_or_default(),
            observation.station_key_lifecycle_revision,
            observation.correlation_id.clone(),
        );
        clusters.entry(key).or_default().push(observation);
    }

    let mut real = V3SourceResult::default();
    let mut monitoring = V3SourceResult::default();
    let mut selected_observation_ids = Vec::new();
    let mut recent_canonical_count = 0_u64;
    let mut historical_canonical_count = 0_u64;
    for (key, mut cluster) in clusters {
        cluster.sort_by(|left, right| {
            left.attempt_index
                .cmp(&right.attempt_index)
                .then_with(|| left.order.event_at_ms.cmp(&right.order.event_at_ms))
                .then_with(|| left.id.cmp(&right.id))
        });
        // A correlation cluster is scoped to one station key. Attempt
        // ordinals are request-global and may legitimately have gaps when
        // another key handled an earlier retry, so contiguity cannot be used
        // as a finalization test. The durable request finalizer owns the
        // complete ledger check; the projection still validates the metadata
        // copied onto the canonical observation so a partial/corrupt cluster
        // can never become a quality sample.
        let expected_attempt_count = cluster
            .first()
            .map(|observation| observation.cluster_expected_attempt_count)
            .unwrap_or(0);
        let finalized = expected_attempt_count > 0
            && cluster.iter().all(|observation| {
                observation.cluster_finalized
                    && observation.cluster_expected_attempt_count == expected_attempt_count
            });
        if !finalized {
            continue;
        }
        let selected = cluster
            .iter()
            .max_by(|left, right| {
                left.attempt_index
                    .cmp(&right.attempt_index)
                    .then_with(|| left.id.cmp(&right.id))
            })
            .copied();
        let Some(selected) = selected else { continue };
        selected_observation_ids.push(selected.id.clone());
        if !matches!(selected.event_time_status, EventTimeStatus::Valid)
            || !selected.boundary_crossed
            || !is_quality_outcome(&selected.outcome)
        {
            if matches!(selected.source, ObservationSource::RealRequest)
                && !matches!(selected.event_time_status, EventTimeStatus::Valid)
            {
                had_invalid_real_event_time = true;
            }
            continue;
        }
        if selected.order.event_at_ms > evaluation_at_ms {
            continue;
        }
        let age_ms = evaluation_at_ms.saturating_sub(selected.order.event_at_ms);
        let (window, weight) = if age_ms <= QUALITY_RECENT_WINDOW_MS {
            (0_u8, quality_weight_fixed(age_ms))
        } else if age_ms < QUALITY_HISTORY_WINDOW_MS {
            (1_u8, quality_weight_fixed(age_ms))
        } else {
            continue;
        };
        let target = if key.0 == "real_request" {
            &mut real
        } else {
            &mut monitoring
        };
        target.selected_count = target.selected_count.saturating_add(1);
        target.last_valid_event_at_ms = Some(
            target
                .last_valid_event_at_ms
                .map_or(selected.order.event_at_ms, |value| {
                    value.max(selected.order.event_at_ms)
                }),
        );
        if window == 0 {
            recent_canonical_count = recent_canonical_count.saturating_add(1);
            target.recent.add(selected, weight);
        } else {
            historical_canonical_count = historical_canonical_count.saturating_add(1);
            target.historical.add(selected, weight);
        }
    }
    selected_observation_ids.sort();

    let real_reliability = source_reliability(&real, &config);
    let monitoring_reliability = source_reliability(&monitoring, &config);
    let (reliability, real_weight, monitoring_weight, quality_unavailable) = mix_reliability(
        real_reliability,
        monitoring_reliability,
        config.real_traffic_weight_basis_points,
        config.monitoring_weight_basis_points,
        config.real_source_eligible,
        monitoring_source_eligible,
    );

    // Latency follows the same source/window structure as reliability. Each
    // source gets its own sample gate and recent/history blend first; only
    // then are the source-level latencies mixed by the effective source
    // weights. Sparse monitoring therefore contributes its configured share
    // without changing the real-traffic sample gate.
    let real_latency = source_latency(&real, &config);
    let monitoring_latency = source_latency(&monitoring, &config);
    let mixed_latency = mix_latency(
        real_latency,
        monitoring_latency,
        real_weight,
        monitoring_weight,
        config.optimistic_latency_ms,
    );
    let real_latency_ms = scaled_latency_ms(mixed_latency.blended_scaled);
    let recent_latency = scaled_latency_ms(mixed_latency.recent_scaled);
    let historical_latency = scaled_latency_ms(mixed_latency.historical_scaled);
    let latency_ratio = mixed_latency.recent_ratio;
    let responsiveness = responsiveness_score_v3(real_latency_ms);
    let real_latency_summary = quality_latency_source_summary(&real, real_latency);
    let monitoring_latency_summary =
        quality_latency_source_summary(&monitoring, monitoring_latency);
    let last_real_route_sample_at_ms = real.last_valid_event_at_ms;
    let idle_real_route_sample = if had_invalid_real_event_time || real.had_invalid_event_time {
        "unknown".to_string()
    } else {
        let idle = last_real_route_sample_at_ms.map_or(true, |last| {
            evaluation_at_ms.saturating_sub(last) >= QUALITY_RECENT_WINDOW_MS
        });
        idle.to_string()
    };
    let quality_basis = if quality_unavailable {
        "QualityUnavailable"
    } else if real_reliability.ready || monitoring_reliability.ready {
        "Observed"
    } else {
        "OptimisticInsufficientSamples"
    };
    let effective_mass = real
        .recent
        .effective_weight
        .saturating_add(real.historical.effective_weight)
        .saturating_add(monitoring.recent.effective_weight)
        .saturating_add(monitoring.historical.effective_weight);
    let success_mass = real
        .recent
        .success_weight
        .saturating_add(real.historical.success_weight)
        .saturating_add(monitoring.recent.success_weight)
        .saturating_add(monitoring.historical.success_weight);
    let failure_mass = real
        .recent
        .failure_weight
        .saturating_add(real.historical.failure_weight)
        .saturating_add(monitoring.recent.failure_weight)
        .saturating_add(monitoring.historical.failure_weight);
    QualitySummary {
        scope: scope.to_string(),
        projector_version: QUALITY_PROJECTOR_VERSION.to_string(),
        observation_count: real
            .selected_count
            .saturating_add(monitoring.selected_count),
        effective_mass_basis_points: effective_mass,
        success_mass_basis_points: success_mass,
        failure_mass_basis_points: failure_mass,
        reliability_basis_points: reliability,
        latency_coverage_basis_points: combined_latency_coverage(&real.recent, &real.historical)
            .max(combined_latency_coverage(
                &monitoring.recent,
                &monitoring.historical,
            )),
        p95_latency_ms: None,
        responsiveness_basis_points: responsiveness,
        recent_observation_count: recent_canonical_count,
        recent_effective_mass_basis_points: real
            .recent
            .effective_weight
            .saturating_add(monitoring.recent.effective_weight),
        recent_success_mass_basis_points: real
            .recent
            .success_weight
            .saturating_add(monitoring.recent.success_weight),
        recent_failure_mass_basis_points: real
            .recent
            .failure_weight
            .saturating_add(monitoring.recent.failure_weight),
        recent_reliability_basis_points: real_reliability.recent,
        recent_reliability_weight_basis_points: recent_ratio_basis_points(recent_canonical_count),
        recent_responsiveness_weight_basis_points: ratio_to_basis_points(latency_ratio),
        recent_p95_latency_ms: None,
        recent_latency_coverage_basis_points: combined_latency_coverage(
            &real.recent,
            &monitoring.recent,
        ),
        recent_responsiveness_basis_points: responsiveness_score_v3(recent_latency),
        historical_observation_count: historical_canonical_count,
        historical_effective_mass_basis_points: real
            .historical
            .effective_weight
            .saturating_add(monitoring.historical.effective_weight),
        historical_success_mass_basis_points: real
            .historical
            .success_weight
            .saturating_add(monitoring.historical.success_weight),
        historical_failure_mass_basis_points: real
            .historical
            .failure_weight
            .saturating_add(monitoring.historical.failure_weight),
        historical_reliability_basis_points: real_reliability.historical,
        historical_reliability_weight_basis_points: 10_000_u16
            .saturating_sub(recent_ratio_basis_points(recent_canonical_count)),
        historical_responsiveness_weight_basis_points: 10_000_u16
            .saturating_sub(ratio_to_basis_points(latency_ratio)),
        historical_p95_latency_ms: None,
        historical_latency_coverage_basis_points: combined_latency_coverage(
            &real.historical,
            &monitoring.historical,
        ),
        historical_responsiveness_basis_points: responsiveness_score_v3(historical_latency),
        historical_age_window_days: 30,
        historical_half_life_days: 1,
        last_event_at_ms: real
            .last_valid_event_at_ms
            .or(monitoring.last_valid_event_at_ms),
        checkpoint_sequence,
        quality_policy_revision: config.quality_policy_revision,
        algorithm_version: QUALITY_PROJECTOR_VERSION.to_string(),
        weight_scale: QUALITY_WEIGHT_SCALE,
        ratio_scale: QUALITY_RATIO_SCALE,
        recent_minimum_samples: config.recent_minimum_samples,
        historical_minimum_samples: config.historical_minimum_samples,
        optimistic_reliability_basis_points: config.optimistic_reliability_basis_points,
        optimistic_latency_ms: config.optimistic_latency_ms,
        real_reliability_basis_points: real_reliability.blended,
        monitoring_reliability_basis_points: monitoring_reliability.blended,
        real_source_weight_basis_points: real_weight,
        monitoring_source_weight_basis_points: monitoring_weight,
        real_source_eligible: config.real_source_eligible,
        monitoring_source_eligible,
        monitoring_source_status,
        real_source: quality_source_summary(
            &real,
            real_reliability,
            config.real_source_eligible,
            real_weight,
            &config,
        ),
        monitoring_source: quality_source_summary(
            &monitoring,
            monitoring_reliability,
            monitoring_source_eligible,
            monitoring_weight,
            &config,
        ),
        latency: QualityLatencySummary {
            recent_sample_count: real
                .recent
                .latency_sample_count
                .saturating_add(monitoring.recent.latency_sample_count),
            recent_effective_weight: real
                .recent
                .latency_weight
                .saturating_add(monitoring.recent.latency_weight),
            recent_weighted_latency_ms: recent_latency,
            recent_minimum_met: mixed_latency.recent_ready,
            historical_sample_count: real
                .historical
                .latency_sample_count
                .saturating_add(monitoring.historical.latency_sample_count),
            historical_effective_weight: real
                .historical
                .latency_weight
                .saturating_add(monitoring.historical.latency_weight),
            historical_weighted_latency_ms: historical_latency,
            historical_minimum_met: mixed_latency.historical_ready,
            blended_weighted_latency_ms: real_latency_ms,
            real_source_weight_basis_points: real_weight,
            monitoring_source_weight_basis_points: monitoring_weight,
            real_source: real_latency_summary,
            monitoring_source: monitoring_latency_summary,
        },
        quality_unavailable,
        quality_basis: quality_basis.to_string(),
        last_real_route_sample_at_ms,
        idle_real_route_sample,
        event_time_diagnostic: had_invalid_real_event_time
            .then(|| "event_time_missing_or_invalid".to_string()),
        selected_observation_ids,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReliabilityResult {
    recent: u16,
    historical: u16,
    blended: u16,
    ready: bool,
}

fn source_reliability(
    source: &V3SourceResult,
    config: &QualityProjectionConfig,
) -> ReliabilityResult {
    let recent_ready = source.recent.sample_count >= config.recent_minimum_samples;
    let history_ready = source.historical.sample_count >= config.historical_minimum_samples;
    let recent = source.recent.reliability(
        config.recent_minimum_samples,
        config.optimistic_reliability_basis_points,
    );
    let historical = source.historical.reliability(
        config.historical_minimum_samples,
        config.optimistic_reliability_basis_points,
    );
    let c = recent_ratio_fixed(source.recent.sample_count);
    ReliabilityResult {
        recent,
        historical,
        blended: blend_ratio_u16(recent, historical, c),
        ready: recent_ready || history_ready,
    }
}

fn quality_source_summary(
    source: &V3SourceResult,
    reliability: ReliabilityResult,
    eligible: bool,
    effective_weight_basis_points: u16,
    config: &QualityProjectionConfig,
) -> QualitySourceSummary {
    QualitySourceSummary {
        eligible,
        effective_weight_basis_points,
        recent: QualitySourceWindowSummary {
            sample_count: source.recent.sample_count,
            effective_weight: source.recent.effective_weight,
            success_weight: source.recent.success_weight,
            failure_weight: source.recent.failure_weight,
            reliability_basis_points: reliability.recent,
            minimum_met: source.recent.sample_count >= config.recent_minimum_samples,
        },
        historical: QualitySourceWindowSummary {
            sample_count: source.historical.sample_count,
            effective_weight: source.historical.effective_weight,
            success_weight: source.historical.success_weight,
            failure_weight: source.historical.failure_weight,
            reliability_basis_points: reliability.historical,
            minimum_met: source.historical.sample_count >= config.historical_minimum_samples,
        },
        blended_reliability_basis_points: reliability.blended,
    }
}

fn mix_reliability(
    real: ReliabilityResult,
    monitoring: ReliabilityResult,
    configured_real: u16,
    configured_monitoring: u16,
    real_eligible: bool,
    monitoring_eligible: bool,
) -> (u16, u16, u16, bool) {
    let real_weight = if real_eligible { configured_real } else { 0 };
    let monitoring_weight = if monitoring_eligible {
        configured_monitoring
    } else {
        0
    };
    let total = u32::from(real_weight) + u32::from(monitoring_weight);
    if total == 0 {
        return (0, 0, 0, true);
    }
    let effective_real =
        u16::try_from((u32::from(real_weight) * 10_000 + total / 2) / total).unwrap_or(0);
    let effective_monitoring = 10_000_u16.saturating_sub(effective_real);
    (
        blend_basis_points(real.blended, monitoring.blended, effective_real),
        effective_real,
        effective_monitoring,
        false,
    )
}

fn blend_basis_points(left: u16, right: u16, left_weight: u16) -> u16 {
    let denominator = BASIS_POINTS_SCALE;
    let sum = u64::from(left) * u64::from(left_weight)
        + u64::from(right) * denominator.saturating_sub(u64::from(left_weight));
    u16::try_from(((sum + denominator / 2) / denominator).min(10_000)).unwrap_or(10_000)
}

fn blend_ratio_u16(left: u16, right: u16, left_weight: u64) -> u16 {
    let denominator = u128::from(QUALITY_RATIO_SCALE);
    let sum = u128::from(left) * u128::from(left_weight)
        + u128::from(right)
            * denominator.saturating_sub(u128::from(left_weight.min(QUALITY_RATIO_SCALE)));
    u16::try_from(((sum + denominator / 2) / denominator).min(10_000)).unwrap_or(10_000)
}

fn blend_scaled_u64(left: u64, right: u64, left_weight: u64) -> u64 {
    let denominator = u128::from(QUALITY_RATIO_SCALE);
    let sum = u128::from(left) * u128::from(left_weight)
        + u128::from(right)
            * denominator.saturating_sub(u128::from(left_weight.min(QUALITY_RATIO_SCALE)));
    u64::try_from((sum + denominator / 2) / denominator).unwrap_or(u64::MAX)
}

fn scaled_latency_ms(value: u64) -> u32 {
    let rounded = value.saturating_add(QUALITY_LATENCY_SCALE / 2) / QUALITY_LATENCY_SCALE;
    u32::try_from(rounded.min(u64::from(u32::MAX))).unwrap_or(u32::MAX)
}

fn ratio_bp(numerator: u64, denominator: u64) -> u16 {
    if denominator == 0 {
        return 0;
    }
    u16::try_from(
        ((u128::from(numerator) * 10_000 + u128::from(denominator / 2)) / u128::from(denominator))
            .min(10_000),
    )
    .unwrap_or(10_000)
}

fn recent_ratio_basis_points(count: u64) -> u16 {
    ratio_to_basis_points(recent_ratio_fixed(count))
}

fn recent_ratio_fixed(count: u64) -> u64 {
    if count == 0 {
        return 0;
    }
    let denominator = u128::from(count).saturating_add(20);
    let value =
        (u128::from(QUALITY_RATIO_SCALE) * u128::from(count) + denominator / 2) / denominator;
    value.min(u128::from(QUALITY_RATIO_SCALE * 9 / 10)) as u64
}

fn ratio_to_basis_points(value: u64) -> u16 {
    let numerator =
        u128::from(value.min(QUALITY_RATIO_SCALE)).saturating_mul(u128::from(BASIS_POINTS_SCALE));
    u16::try_from(
        ((numerator + u128::from(QUALITY_RATIO_SCALE / 2)) / u128::from(QUALITY_RATIO_SCALE))
            .min(u128::from(BASIS_POINTS_SCALE)),
    )
    .unwrap_or(10_000)
}

fn responsiveness_score_v3(latency_ms: u32) -> u16 {
    let latency = latency_ms.min(RESPONSIVENESS_SCORE_CAP_MS);
    let remaining = u64::from(RESPONSIVENESS_SCORE_CAP_MS - latency);
    u16::try_from((remaining * 10_000) / u64::from(RESPONSIVENESS_SCORE_CAP_MS)).unwrap_or(0)
}

/// Returns the quantized `w(a)` from the v3 specification. The public helper
/// accepts the exact integer-millisecond age and applies the two half-life
/// segments internally. Fractional exponents use Q32 binary decomposition so
/// the production path is deterministic and does not depend on floating point.
pub(crate) fn quality_weight_fixed(age_ms: i64) -> u64 {
    let age_ms = age_ms.max(0) as u64;
    let recent_ms = QUALITY_RECENT_WINDOW_MS as u64;
    let q32 = if age_ms <= recent_ms {
        exp2_neg_ratio_q32(age_ms, 72 * 60 * 60 * 1_000)
    } else {
        let boundary = exp2_neg_ratio_q32(recent_ms, 72 * 60 * 60 * 1_000);
        let history = exp2_neg_ratio_q32(age_ms - recent_ms, 24 * 60 * 60 * 1_000);
        mul_q32(boundary, history)
    };
    let quantized = ((u128::from(q32) * u128::from(QUALITY_WEIGHT_SCALE)) + (1_u128 << 31)) >> 32;
    if q32 == 0 {
        0
    } else {
        u64::try_from(quantized.max(1).min(u128::from(QUALITY_WEIGHT_SCALE)))
            .unwrap_or(QUALITY_WEIGHT_SCALE)
    }
}

const Q32: u128 = 1_u128 << 32;
const Q32_MASK: u128 = Q32 - 1;
// 2^(-1/2^k) in Q32, k = 0..31. These constants are the versioned fixed-point
// representation of the exponential and must change only with the algorithm version.
const EXP2_NEG_BINARY_Q32: [u128; 33] = [
    2_147_483_648,
    3_037_000_500,
    3_611_622_603,
    3_938_502_376,
    4_112_874_773,
    4_202_935_003,
    4_248_701_965,
    4_271_771_996,
    4_283_353_945,
    4_289_156_690,
    4_292_061_010,
    4_293_513_907,
    4_294_240_540,
    4_294_603_903,
    4_294_785_595,
    4_294_876_445,
    4_294_921_870,
    4_294_944_583,
    4_294_955_939,
    4_294_961_618,
    4_294_964_457,
    4_294_965_876,
    4_294_966_586,
    4_294_966_941,
    4_294_967_119,
    4_294_967_207,
    4_294_967_252,
    4_294_967_274,
    4_294_967_285,
    4_294_967_290,
    4_294_967_293,
    4_294_967_295,
    4_294_967_295,
];

fn exp2_neg_ratio_q32(age_ms: u64, half_life_ms: u64) -> u128 {
    if half_life_ms == 0 {
        return 0;
    }
    let ratio_q32 = (u128::from(age_ms) << 32) / u128::from(half_life_ms);
    let integer = ratio_q32 >> 32;
    if integer >= 128 {
        return 0;
    }
    let mut result = Q32 >> integer;
    let fraction = ratio_q32 & Q32_MASK;
    // The table starts at 2^-1 (the integer bit).  A binary fraction's first
    // bit is 2^-1 as well, so skip the integer entry and align bit 31 with
    // table entry 1.  The final 2^-32 factor rounds to the same Q32 value as
    // 2^-31, hence the explicit extra table entry above.
    for (index, factor) in EXP2_NEG_BINARY_Q32.iter().skip(1).enumerate() {
        if (fraction & (1_u128 << (31 - index))) != 0 {
            result = (result.saturating_mul(*factor) + (Q32 / 2)) / Q32;
        }
    }
    result
}

fn mul_q32(left: u128, right: u128) -> u128 {
    (left.saturating_mul(right) + (Q32 / 2)) / Q32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::routing_observation::{
        FailureAttribution, ObservationOrder, ObservationOutcome, ObservationRetryDisposition,
        ObservationScope, ObservationSource, RecoveryOrigin, ResponseOrigin, TrafficEquivalence,
    };

    fn observation(
        id: &str,
        sequence: u64,
        event_at_ms: i64,
        outcome: ObservationOutcome,
    ) -> RoutingObservation {
        RoutingObservation {
            id: id.into(),
            order: ObservationOrder {
                producer_id: "test".into(),
                producer_sequence: sequence,
                event_at_ms,
                ingested_at_ms: event_at_ms,
            },
            scope: ObservationScope {
                station_id: None,
                station_key_id: Some("key-1".into()),
                model: Some("model".into()),
                endpoint_revision: Some(1),
            },
            source: ObservationSource::RealRequest,
            traffic_equivalence: TrafficEquivalence::ExactRequest,
            outcome,
            latency_ms: Some(100),
            evidence_mass_basis_points: 10_000,
            comparability_key: None,
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
    fn duplicate_and_out_of_order_rebuild_is_deterministic() {
        let first = observation("b", 2, 20, ObservationOutcome::Success);
        let second = observation("a", 1, 10, ObservationOutcome::Timeout);
        let duplicate = first.clone();
        let left = rebuild_quality_summary(
            "station_key:key-1",
            &[first, second.clone(), duplicate],
            BetaPrior::default(),
        );
        let right = rebuild_quality_summary(
            "station_key:key-1",
            &[second, observation("b", 2, 20, ObservationOutcome::Success)],
            BetaPrior::default(),
        );
        assert_eq!(left, right);
        assert_eq!(left.observation_count, 2);
        assert_eq!(left.p95_latency_ms, Some(100));
    }

    #[test]
    fn persisted_revision_is_supplied_by_the_ingestion_cursor() {
        let summary = rebuild_quality_summary_with_checkpoint(
            "station_key:key-1",
            &[
                observation("first", 9_000, 10, ObservationOutcome::Success),
                observation("second", 1, 20, ObservationOutcome::Timeout),
            ],
            BetaPrior::default(),
            42,
        );

        assert_eq!(summary.checkpoint_sequence, 42);
    }

    #[test]
    fn recent_failures_pull_down_a_strong_historical_baseline() {
        let now_ms = 1_800_000_000_000_i64;
        let mut observations = Vec::new();
        for index in 0..10 {
            observations.push(observation(
                &format!("history-{index}"),
                index,
                now_ms - 10 * 24 * 60 * 60 * 1_000 + index as i64,
                ObservationOutcome::Success,
            ));
        }
        for index in 0..10 {
            observations.push(observation(
                &format!("recent-{index}"),
                100 + index,
                now_ms - index as i64,
                ObservationOutcome::Timeout,
            ));
        }
        let summary = rebuild_quality_summary_at(
            "station_key:key-1",
            &observations,
            BetaPrior::default(),
            99,
            now_ms,
        );
        assert_eq!(summary.recent_observation_count, 10);
        assert_eq!(summary.historical_observation_count, 10);
        assert_eq!(summary.recent_reliability_weight_basis_points, 7_000);
        assert!(
            summary.recent_reliability_basis_points < summary.historical_reliability_basis_points
        );
        assert!(summary.reliability_basis_points < summary.historical_reliability_basis_points);
    }

    #[test]
    fn observations_older_than_history_window_do_not_keep_score_alive() {
        let now_ms = 1_800_000_000_000_i64;
        let old = observation(
            "old",
            1,
            now_ms - 31 * 24 * 60 * 60 * 1_000,
            ObservationOutcome::Success,
        );
        let summary = rebuild_quality_summary_at(
            "station_key:key-1",
            &[old],
            BetaPrior::default(),
            1,
            now_ms,
        );
        assert_eq!(summary.observation_count, 0);
        assert_eq!(summary.reliability_basis_points, 5_000);
        assert_eq!(summary.responsiveness_basis_points, 5_000);
    }

    #[test]
    fn non_terminal_events_do_not_affect_quality_or_latency() {
        let now_ms = 1_800_000_000_000_i64;
        let mut cancelled = observation("cancelled", 1, now_ms, ObservationOutcome::Cancelled);
        cancelled.latency_ms = Some(120_000);
        let mut success = observation("success", 2, now_ms - 1, ObservationOutcome::Success);
        success.latency_ms = Some(100);
        let summary = rebuild_quality_summary_at(
            "station_key:key-1",
            &[cancelled, success],
            BetaPrior::default(),
            2,
            now_ms,
        );

        assert_eq!(summary.recent_observation_count, 2);
        assert_eq!(summary.recent_effective_mass_basis_points, 10_000);
        assert_eq!(summary.recent_reliability_weight_basis_points, 2_950);
        assert_eq!(summary.recent_p95_latency_ms, Some(100));
        assert_eq!(summary.recent_responsiveness_weight_basis_points, 2_950);
    }

    #[test]
    fn recent_ratio_uses_the_frozen_millionth_scale_then_formats_basis_points() {
        assert_eq!(recent_ratio_fixed(0), 0);
        assert_eq!(recent_ratio_fixed(1), 47_619);
        assert_eq!(recent_ratio_fixed(5), 200_000);
        assert_eq!(recent_ratio_fixed(20), 500_000);
        assert_eq!(recent_ratio_fixed(100), 833_333);
        assert_eq!(recent_ratio_fixed(1_000), 900_000);
        assert_eq!(recent_ratio_basis_points(1), 476);
        assert_eq!(recent_ratio_basis_points(5), 2_000);
        assert_eq!(recent_ratio_basis_points(20), 5_000);
        assert_eq!(recent_ratio_basis_points(100), 8_333);
        assert_eq!(recent_ratio_basis_points(1_000), 9_000);
    }

    #[test]
    fn key_level_scoring_accepts_nonanonymous_probe_without_model_commitment() {
        let now_ms = QUALITY_RECENT_WINDOW_MS;
        let comparability_key = format!("cmp:v1:{}", "a".repeat(64));
        let mut real = observation("real-request", 0, now_ms, ObservationOutcome::Success);
        real.comparability_key = Some(comparability_key.clone());
        let mut invalid = observation("invalid-probe", 1, now_ms, ObservationOutcome::Success);
        invalid.source = ObservationSource::ActiveProbe;
        invalid.traffic_equivalence = TrafficEquivalence::SameModelShape;

        let mut valid = observation("valid-probe", 2, now_ms, ObservationOutcome::Success);
        valid.source = ObservationSource::ActiveProbe;
        valid.traffic_equivalence = TrafficEquivalence::SameModelShape;
        valid.comparability_key = Some(comparability_key);

        let summary = rebuild_quality_summary_v3_at(
            "station_key:key-1",
            &[real, invalid, valid],
            QualityProjectionConfig::default(),
            2,
            now_ms,
        );

        assert_eq!(summary.observation_count, 3);
        assert_eq!(
            summary.selected_observation_ids,
            vec!["invalid-probe", "real-request", "valid-probe"]
        );
    }

    #[test]
    fn active_probe_with_a_different_model_commitment_still_uses_key_source_weight() {
        let now_ms = QUALITY_RECENT_WINDOW_MS;
        let mut real = observation("real-request", 1, now_ms, ObservationOutcome::Success);
        real.comparability_key = Some(format!("cmp:v1:{}", "a".repeat(64)));
        let mut probe = observation("probe", 2, now_ms, ObservationOutcome::Success);
        probe.source = ObservationSource::ActiveProbe;
        probe.traffic_equivalence = TrafficEquivalence::SameModelShape;
        probe.comparability_key = Some(format!("cmp:v1:{}", "b".repeat(64)));

        let summary = rebuild_quality_summary_v3_at(
            "station_key:key-1",
            &[real, probe],
            QualityProjectionConfig::default(),
            2,
            now_ms,
        );

        assert_eq!(summary.observation_count, 2);
        assert_eq!(
            summary.selected_observation_ids,
            vec!["probe", "real-request"]
        );
        assert!(summary.monitoring_source_eligible);
        assert_eq!(
            summary.monitoring_source_status,
            MonitoringSourceStatus::Comparable
        );
        assert_eq!(summary.real_source_weight_basis_points, 7_000);
        assert_eq!(summary.monitoring_source_weight_basis_points, 3_000);
        assert_eq!(summary.monitoring_source.recent.sample_count, 1);
    }

    #[test]
    fn future_probe_does_not_disable_an_otherwise_eligible_monitoring_source() {
        let now_ms = QUALITY_RECENT_WINDOW_MS;
        let mut probe = observation(
            "future-probe",
            1,
            now_ms + 1_000,
            ObservationOutcome::Success,
        );
        probe.source = ObservationSource::ActiveProbe;
        probe.traffic_equivalence = TrafficEquivalence::SameModelShape;
        probe.comparability_key = Some(format!("cmp:v1:{}", "b".repeat(64)));
        let config = QualityProjectionConfig {
            real_traffic_weight_basis_points: 0,
            monitoring_weight_basis_points: 10_000,
            ..QualityProjectionConfig::default()
        };

        let summary =
            rebuild_quality_summary_v3_at("station_key:key-1", &[probe], config, 1, now_ms);

        assert!(summary.monitoring_source_eligible);
        assert_eq!(
            summary.monitoring_source_status,
            MonitoringSourceStatus::NoEvidence
        );
        assert!(!summary.quality_unavailable);
        assert_eq!(summary.observation_count, 0);
        assert_eq!(summary.reliability_basis_points, 9_500);
    }

    #[test]
    fn configured_zero_monitoring_weight_has_a_distinct_diagnostic() {
        let config = QualityProjectionConfig {
            real_traffic_weight_basis_points: 10_000,
            monitoring_weight_basis_points: 0,
            ..QualityProjectionConfig::default()
        };
        let summary = rebuild_quality_summary_v3_at(
            "station_key:key-1",
            &[],
            config,
            0,
            QUALITY_RECENT_WINDOW_MS,
        );

        assert_eq!(
            summary.monitoring_source_status,
            MonitoringSourceStatus::WeightZero
        );
        assert_eq!(summary.monitoring_source_weight_basis_points, 0);
    }

    #[test]
    fn key_rebind_excludes_observations_from_the_previous_lifecycle() {
        let now_ms = QUALITY_RECENT_WINDOW_MS;
        let old = observation("old-binding", 1, now_ms, ObservationOutcome::Success);
        let mut current = observation("current-binding", 2, now_ms, ObservationOutcome::Timeout);
        current.station_key_lifecycle_revision = 2;
        let mut config = QualityProjectionConfig::default();
        config.current_lifecycle_revision = Some(2);

        let summary =
            rebuild_quality_summary_v3_at("station_key:key-1", &[old, current], config, 2, now_ms);

        assert_eq!(summary.observation_count, 1);
        assert_eq!(summary.selected_observation_ids, vec!["current-binding"]);
    }

    #[test]
    fn finalized_cluster_requires_a_nonzero_consistent_expected_attempt_count() {
        let now_ms = QUALITY_RECENT_WINDOW_MS;
        let mut zero = observation("zero", 1, now_ms, ObservationOutcome::Success);
        zero.cluster_expected_attempt_count = 0;
        let mut inconsistent_first =
            observation("inconsistent-a", 2, now_ms, ObservationOutcome::Success);
        inconsistent_first.correlation_id = "inconsistent".to_string();
        inconsistent_first.cluster_expected_attempt_count = 2;
        let mut inconsistent_second =
            observation("inconsistent-b", 3, now_ms, ObservationOutcome::Timeout);
        inconsistent_second.correlation_id = "inconsistent".to_string();
        inconsistent_second.attempt_index = 1;
        inconsistent_second.cluster_expected_attempt_count = 3;

        let summary = rebuild_quality_summary_v3_at(
            "station_key:key-1",
            &[zero, inconsistent_first, inconsistent_second],
            QualityProjectionConfig::default(),
            3,
            now_ms,
        );

        assert_eq!(summary.observation_count, 0);
        assert!(summary.selected_observation_ids.is_empty());
    }

    #[test]
    fn unfinalized_cluster_never_enters_quality() {
        let now_ms = QUALITY_RECENT_WINDOW_MS;
        let mut pending = observation("pending", 1, now_ms, ObservationOutcome::Success);
        pending.cluster_finalized = false;

        let summary = rebuild_quality_summary_v3_at(
            "station_key:key-1",
            &[pending],
            QualityProjectionConfig::default(),
            1,
            now_ms,
        );

        assert_eq!(summary.observation_count, 0);
        assert!(summary.selected_observation_ids.is_empty());
    }

    #[test]
    fn canonical_key_observation_accepts_a_complete_multi_key_request_ledger() {
        let now_ms = QUALITY_RECENT_WINDOW_MS;
        let mut canonical = observation("key-b-final", 2, now_ms, ObservationOutcome::Timeout);
        canonical.correlation_id = "multi-key-request".to_string();
        canonical.attempt_index = 2;
        canonical.cluster_expected_attempt_count = 3;

        let summary = rebuild_quality_summary_v3_at(
            "station_key:key-1",
            &[canonical],
            QualityProjectionConfig::default(),
            2,
            now_ms,
        );

        assert_eq!(summary.observation_count, 1);
        assert_eq!(summary.selected_observation_ids, vec!["key-b-final"]);
    }

    #[test]
    fn recent_and_historical_diagnostics_are_window_values_not_source_blends() {
        let now_ms = 1_800_000_000_000_i64;
        let mut recent = observation("recent-failure", 1, now_ms, ObservationOutcome::Timeout);
        recent.latency_ms = None;
        let history = observation(
            "historical-success",
            2,
            now_ms - QUALITY_RECENT_WINDOW_MS - 1,
            ObservationOutcome::Success,
        );
        let config = QualityProjectionConfig {
            recent_minimum_samples: 1,
            historical_minimum_samples: 1,
            real_traffic_weight_basis_points: 10_000,
            monitoring_weight_basis_points: 0,
            monitoring_source_eligible: false,
            ..QualityProjectionConfig::default()
        };

        let summary = rebuild_quality_summary_v3_at(
            "station_key:key-1",
            &[recent, history],
            config,
            2,
            now_ms,
        );

        assert_eq!(summary.recent_reliability_basis_points, 0);
        assert_eq!(summary.historical_reliability_basis_points, 10_000);
        assert_eq!(summary.real_reliability_basis_points, 9_524);
    }

    #[test]
    fn fixed_point_weight_matches_the_piecewise_half_lives() {
        let hour = 60 * 60 * 1_000;
        assert_eq!(quality_weight_fixed(0), QUALITY_WEIGHT_SCALE);
        assert_eq!(quality_weight_fixed(24 * hour), 793_701);
        assert_eq!(quality_weight_fixed(48 * hour), 396_850);
        assert_eq!(quality_weight_fixed(72 * hour), 198_425);
        assert!(quality_weight_fixed(720 * hour) > 0);
        assert!(quality_weight_fixed(720 * hour) < quality_weight_fixed(48 * hour));
    }

    #[test]
    fn v3_latency_uses_optimistic_value_until_recent_minimum_is_met() {
        let now_ms = 1_800_000_000_000_i64;
        let summary = rebuild_quality_summary_v3_at(
            "station_key:key-1",
            &[observation(
                "single",
                1,
                now_ms,
                ObservationOutcome::Success,
            )],
            QualityProjectionConfig::default(),
            1,
            now_ms,
        );

        // The default recent minimum is five.  One 100 ms response must not
        // replace the 2.5 s optimistic latency assumption.
        assert_eq!(summary.optimistic_latency_ms, 2_500);
        assert_eq!(summary.recent_responsiveness_basis_points, 9_791);
        assert_eq!(summary.responsiveness_basis_points, 9_791);
    }

    #[test]
    fn v3_latency_uses_weighted_observed_value_after_minimum_is_met() {
        let now_ms = 1_800_000_000_000_i64;
        let observations = (0..5)
            .map(|index| {
                observation(
                    &format!("sample-{index}"),
                    index,
                    now_ms - index as i64,
                    ObservationOutcome::Success,
                )
            })
            .collect::<Vec<_>>();
        let summary = rebuild_quality_summary_v3_at(
            "station_key:key-1",
            &observations,
            QualityProjectionConfig::default(),
            1,
            now_ms,
        );

        // Recent c=5/(5+20)=0.2; history has no samples and therefore keeps
        // the 2.5 s optimistic latency.  The empty monitoring source keeps
        // its 30% share at the optimistic value, so the blended latency is
        // about 2.265 s.
        assert_eq!(summary.recent_responsiveness_basis_points, 9_931);
        assert_eq!(summary.responsiveness_basis_points, 9_811);
    }

    #[test]
    fn monitoring_latency_contributes_by_configured_source_weight() {
        let now_ms = 1_800_000_000_000_i64;
        let comparability_key = format!("cmp:v1:{}", "a".repeat(64));
        let real = (0..5)
            .map(|index| {
                let mut value = observation(
                    &format!("real-{index}"),
                    index,
                    now_ms - index as i64,
                    ObservationOutcome::Success,
                );
                value.latency_ms = Some(100);
                value
            })
            .collect::<Vec<_>>();
        let monitoring = (0..5)
            .map(|index| {
                let mut value = observation(
                    &format!("monitoring-{index}"),
                    10 + index,
                    now_ms - index as i64,
                    ObservationOutcome::Success,
                );
                value.source = ObservationSource::ActiveProbe;
                value.traffic_equivalence = TrafficEquivalence::SameModelShape;
                value.comparability_key = Some(comparability_key.clone());
                value.latency_ms = Some(10_000);
                value
            })
            .collect::<Vec<_>>();

        let real_only = rebuild_quality_summary_v3_at(
            "station_key:key-1",
            &real,
            QualityProjectionConfig::default(),
            1,
            now_ms,
        );
        let summary = rebuild_quality_summary_v3_at(
            "station_key:key-1",
            &real
                .iter()
                .chain(monitoring.iter())
                .cloned()
                .collect::<Vec<_>>(),
            QualityProjectionConfig::default(),
            2,
            now_ms,
        );

        assert_eq!(summary.latency.recent_sample_count, 10);
        assert_eq!(summary.latency.real_source.recent_sample_count, 5);
        assert_eq!(summary.latency.monitoring_source.recent_sample_count, 5);
        assert_eq!(summary.latency.real_source.recent_weighted_latency_ms, 100);
        assert_eq!(
            summary.latency.monitoring_source.recent_weighted_latency_ms,
            10_000
        );
        assert_eq!(summary.latency.recent_weighted_latency_ms, 3_070);
        assert_eq!(summary.latency.real_source_weight_basis_points, 7_000);
        assert_eq!(summary.latency.monitoring_source_weight_basis_points, 3_000);
        assert!(summary.responsiveness_basis_points < real_only.responsiveness_basis_points);
    }
}
