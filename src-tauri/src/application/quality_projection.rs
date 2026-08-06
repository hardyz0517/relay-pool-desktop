use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::models::routing_observation::{ObservationOutcome, RoutingObservation};

pub(crate) const QUALITY_PROJECTOR_VERSION: &str = "routing_quality_v2";

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
    rebuild_quality_summary_with_checkpoint(scope, observations, prior, checkpoint_sequence)
}

pub(crate) fn rebuild_quality_summary_with_checkpoint(
    scope: &str,
    observations: &[RoutingObservation],
    prior: BetaPrior,
    checkpoint_sequence: u64,
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

    let mut success_mass = 0_u64;
    let mut failure_mass = 0_u64;
    let mut latency_samples = Vec::new();
    let mut latency_mass = 0_u64;
    let mut last_event_at_ms = None;
    for observation in &ordered {
        let mass = u64::from(observation.evidence_mass_basis_points);
        match observation.outcome {
            ObservationOutcome::Success => success_mass = success_mass.saturating_add(mass),
            ObservationOutcome::CredentialFailure
            | ObservationOutcome::EndpointFailure
            | ObservationOutcome::ModelFailure
            | ObservationOutcome::RateLimited
            | ObservationOutcome::Timeout => failure_mass = failure_mass.saturating_add(mass),
            ObservationOutcome::Cancelled | ObservationOutcome::Unknown => continue,
        }
        if let Some(latency) = observation.latency_ms {
            latency_samples.push((latency, mass));
            latency_mass = latency_mass.saturating_add(mass);
        }
        last_event_at_ms = Some(observation.order.event_at_ms);
    }
    latency_samples.sort_unstable_by_key(|(latency, _)| *latency);
    let effective_mass = success_mass.saturating_add(failure_mass);
    let denominator = prior
        .alpha_basis_points
        .saturating_add(prior.beta_basis_points)
        .saturating_add(effective_mass);
    let reliability = if denominator == 0 {
        0
    } else {
        ((success_mass.saturating_add(prior.alpha_basis_points)).saturating_mul(10_000)
            / denominator)
            .min(10_000) as u16
    };
    let coverage = if effective_mass == 0 {
        0
    } else {
        (latency_mass.saturating_mul(10_000) / effective_mass).min(10_000) as u16
    };
    let p95_latency_ms = percentile95(&latency_samples);
    QualitySummary {
        scope: scope.to_string(),
        projector_version: QUALITY_PROJECTOR_VERSION.to_string(),
        observation_count: ordered.len() as u64,
        effective_mass_basis_points: effective_mass,
        success_mass_basis_points: success_mass,
        failure_mass_basis_points: failure_mass,
        reliability_basis_points: if effective_mass < prior.minimum_effective_mass_basis_points {
            // Prior remains visible, but insufficient evidence cannot claim a
            // strong quality verdict; keep the posterior conservative.
            reliability.min(7_500)
        } else {
            reliability
        },
        latency_coverage_basis_points: coverage,
        p95_latency_ms,
        last_event_at_ms,
        checkpoint_sequence,
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
}
