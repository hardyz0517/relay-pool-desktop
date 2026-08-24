use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::models::routing_observation::{ObservationOutcome, RoutingObservation};

pub(crate) const QUALITY_PROJECTOR_VERSION: &str = "routing_quality_v3";
const RECENT_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;
const HISTORY_WINDOW_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
const HISTORY_HALF_LIFE_MS: f64 = 7.0 * 24.0 * 60.0 * 60.0 * 1_000.0;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BetaPrior {
    pub(crate) alpha_basis_points: u64,
    pub(crate) beta_basis_points: u64,
    pub(crate) minimum_effective_mass_basis_points: u64,
}

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
        projector_version: QUALITY_PROJECTOR_VERSION.to_string(),
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
    }
}

#[derive(Debug, Default)]
struct WindowAccumulator {
    observation_count: u64,
    quality_observation_count: u64,
    success_mass: u64,
    failure_mass: u64,
    latency_samples: Vec<(u32, u64)>,
    latency_sample_count: u64,
    latency_mass: u64,
}

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

fn conservative(value: u16, effective_mass: u64, prior: BetaPrior) -> u16 {
    if effective_mass < prior.minimum_effective_mass_basis_points {
        value.min(7_500)
    } else {
        value
    }
}

fn recent_weight(count: u64) -> u16 {
    if count == 0 {
        0
    } else {
        ((0.25_f64 + 0.45_f64 * (count as f64 / 10.0).min(1.0)) * 10_000.0).round() as u16
    }
}

fn latency_weight(count: u64) -> u16 {
    if count == 0 {
        0
    } else {
        ((0.25_f64 + 0.45_f64 * (count as f64 / 10.0).min(1.0)) * 10_000.0).round() as u16
    }
}

fn blend(recent: u16, historical: u16, recent_weight: u16, historical_weight: u16) -> u16 {
    ((u64::from(recent) * u64::from(recent_weight)
        + u64::from(historical) * u64::from(historical_weight))
        / 10_000)
        .min(10_000) as u16
}

fn responsiveness_score(p95: Option<u32>, cap_ms: u32) -> u16 {
    let Some(latency) = p95 else {
        return 5_000;
    };
    if cap_ms == 0 {
        return 5_000;
    }
    ((u64::from(cap_ms.saturating_sub(latency.min(cap_ms))) * 10_000) / u64::from(cap_ms)) as u16
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::routing_observation::{
        ObservationOrder, ObservationOutcome, ObservationScope, ObservationSource,
        TrafficEquivalence,
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
}
