use crate::{
    models::health::{
        HealthObservation, HealthObservationOutcome, HealthWritebackMode, StationKeyHealthSnapshot,
        TrafficEquivalence,
    },
    persistence::{
        error::PersistenceError, stores::health_observation_store::HealthObservationStore,
        WriteSession,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HealthTransitionAck {
    pub(crate) observation_inserted: bool,
    pub(crate) health_applied: bool,
    pub(crate) writeback_decision: HealthWritebackDecision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HealthWritebackDecision {
    NotApplicable,
    ObserveOnly,
    Write,
    Suppressed,
}

impl HealthWritebackDecision {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NotApplicable => "not_applicable",
            Self::ObserveOnly => "observe_only",
            Self::Write => "write",
            Self::Suppressed => "suppressed",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HealthTransitionService {
    store: HealthObservationStore,
}

impl HealthTransitionService {
    pub(crate) fn new() -> Self {
        Self {
            store: HealthObservationStore,
        }
    }

    pub(crate) async fn record_observation(
        &self,
        write: &mut WriteSession,
        observation: HealthObservation,
    ) -> Result<HealthTransitionAck, PersistenceError> {
        self.store
            .assert_station_key_revision(
                write.connection(),
                &observation.station_key_id,
                observation.endpoint_revision,
            )
            .await?;
        let decision = writeback_decision(&observation);
        let inserted = self
            .store
            .insert_observation_once(write.connection(), &observation, decision.as_str())
            .await?;
        if !inserted || decision != HealthWritebackDecision::Write {
            return Ok(HealthTransitionAck {
                observation_inserted: inserted,
                health_applied: false,
                writeback_decision: decision,
            });
        }

        let mut current = self
            .store
            .load_station_key_health(
                write.connection(),
                &observation.station_key_id,
                observation.observed_at_ms,
            )
            .await?;
        if current.endpoint_revision != observation.endpoint_revision {
            current = StationKeyHealthSnapshot::empty(
                observation.station_key_id.clone(),
                observation.endpoint_revision,
                observation.observed_at_ms,
            );
        }
        let next = reduce_health(current, &observation);
        self.store
            .upsert_station_key_health(write.connection(), &next)
            .await?;

        Ok(HealthTransitionAck {
            observation_inserted: true,
            health_applied: true,
            writeback_decision: decision,
        })
    }
}

pub(crate) fn writeback_decision(observation: &HealthObservation) -> HealthWritebackDecision {
    if matches!(
        observation.outcome,
        HealthObservationOutcome::Neutral | HealthObservationOutcome::Skipped
    ) {
        return HealthWritebackDecision::NotApplicable;
    }
    match observation.writeback_mode {
        HealthWritebackMode::Disabled => HealthWritebackDecision::Suppressed,
        HealthWritebackMode::ObserveOnly => HealthWritebackDecision::ObserveOnly,
        HealthWritebackMode::Authoritative => match observation.traffic_equivalence {
            TrafficEquivalence::RealUserTraffic | TrafficEquivalence::SyntheticStandard => {
                HealthWritebackDecision::Write
            }
            TrafficEquivalence::SyntheticCliCompat | TrafficEquivalence::Diagnostic => {
                HealthWritebackDecision::Suppressed
            }
        },
    }
}

pub(crate) fn reduce_health(
    current: StationKeyHealthSnapshot,
    observation: &HealthObservation,
) -> StationKeyHealthSnapshot {
    match observation.outcome {
        HealthObservationOutcome::Success => success(current, observation),
        HealthObservationOutcome::ObserveFailure => observe_failure(current, observation),
        HealthObservationOutcome::Cooldown => cooldown(current, observation),
        HealthObservationOutcome::HardFail => hard_fail(current, observation),
        HealthObservationOutcome::Skipped | HealthObservationOutcome::Neutral => current,
    }
}

fn success(
    mut current: StationKeyHealthSnapshot,
    observation: &HealthObservation,
) -> StationKeyHealthSnapshot {
    let latency_ms = observation.latency_ms.unwrap_or(0).max(0);
    current.endpoint_revision = observation.endpoint_revision;
    current.last_success_at = Some(observation.observed_at_ms.to_string());
    current.consecutive_failures = 0;
    current.success_count = current.success_count.saturating_add(1);
    current.total_duration_ms = current.total_duration_ms.saturating_add(latency_ms);
    current.avg_latency_ms = Some(current.total_duration_ms / current.success_count.max(1));
    current.last_error_summary = None;
    current.cooldown_until = None;
    current.updated_at = observation.observed_at_ms.to_string();
    current
}

fn observe_failure(
    mut current: StationKeyHealthSnapshot,
    observation: &HealthObservation,
) -> StationKeyHealthSnapshot {
    current.endpoint_revision = observation.endpoint_revision;
    current.last_failure_at = Some(observation.observed_at_ms.to_string());
    current.consecutive_failures = current.consecutive_failures.saturating_add(1);
    current.failure_count = current.failure_count.saturating_add(1);
    current.last_error_summary = observation.error_summary.clone().or_else(|| {
        observation
            .failure_kind
            .as_ref()
            .map(|failure_kind| trim_error_summary(failure_kind))
    });
    current.cooldown_until =
        threshold_cooldown_until(current.consecutive_failures, observation.observed_at_ms);
    current.updated_at = observation.observed_at_ms.to_string();
    current
}

fn cooldown(
    mut current: StationKeyHealthSnapshot,
    observation: &HealthObservation,
) -> StationKeyHealthSnapshot {
    current.endpoint_revision = observation.endpoint_revision;
    current.last_failure_at = Some(observation.observed_at_ms.to_string());
    current.consecutive_failures = current.consecutive_failures.saturating_add(1);
    current.failure_count = current.failure_count.saturating_add(1);
    current.last_error_summary = observation.error_summary.clone().or_else(|| {
        observation
            .failure_kind
            .as_ref()
            .map(|failure_kind| trim_error_summary(failure_kind))
    });
    current.cooldown_until = Some(
        observation
            .observed_at_ms
            .saturating_add(observation.retry_after_ms.unwrap_or(5 * 60 * 1000).max(0))
            .to_string(),
    );
    current.updated_at = observation.observed_at_ms.to_string();
    current
}

fn hard_fail(
    mut current: StationKeyHealthSnapshot,
    observation: &HealthObservation,
) -> StationKeyHealthSnapshot {
    current.endpoint_revision = observation.endpoint_revision;
    current.last_failure_at = Some(observation.observed_at_ms.to_string());
    current.consecutive_failures = current.consecutive_failures.saturating_add(1);
    current.failure_count = current.failure_count.saturating_add(1);
    current.last_error_summary = observation.error_summary.clone().or_else(|| {
        observation
            .failure_kind
            .as_ref()
            .map(|failure_kind| trim_error_summary(failure_kind))
    });
    current.cooldown_until = Some(
        observation
            .observed_at_ms
            .saturating_add(15 * 60 * 1000)
            .to_string(),
    );
    current.updated_at = observation.observed_at_ms.to_string();
    current
}

fn threshold_cooldown_until(consecutive_failures: i64, now_ms: i64) -> Option<String> {
    let duration_ms = match consecutive_failures {
        failures if failures < 3 => return None,
        3 => 2 * 60 * 1000,
        4 => 5 * 60 * 1000,
        _ => 15 * 60 * 1000,
    };
    Some(now_ms.saturating_add(duration_ms).to_string())
}

fn trim_error_summary(value: &str) -> String {
    let mut chars = value.trim().chars();
    let mut summary = chars.by_ref().take(160).collect::<String>();
    if chars.next().is_some() {
        summary.push_str("...");
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::{reduce_health, writeback_decision, HealthWritebackDecision};
    use crate::models::health::{
        HealthObservation, HealthObservationOutcome, HealthObservationSource, HealthWritebackMode,
        StationKeyHealthSnapshot, TrafficEquivalence,
    };

    #[test]
    fn reducer_honors_retry_after_and_endpoint_revision_reset() {
        let current = StationKeyHealthSnapshot {
            station_key_id: "key-1".to_string(),
            endpoint_revision: 1,
            last_success_at: Some("1".to_string()),
            last_failure_at: None,
            consecutive_failures: 9,
            success_count: 4,
            failure_count: 9,
            total_duration_ms: 400,
            avg_latency_ms: Some(100),
            last_error_summary: None,
            cooldown_until: None,
            updated_at: "1".to_string(),
        };
        let mut observation = observation(HealthObservationOutcome::Cooldown);
        observation.endpoint_revision = 2;
        observation.retry_after_ms = Some(12_345);
        let reset = StationKeyHealthSnapshot::empty(
            observation.station_key_id.clone(),
            observation.endpoint_revision,
            observation.observed_at_ms,
        );

        let next = reduce_health(reset, &observation);

        assert_eq!(next.endpoint_revision, 2);
        assert_eq!(next.success_count, 0);
        assert_eq!(next.failure_count, 1);
        assert_eq!(next.cooldown_until, Some("22345".to_string()));
        assert_eq!(current.endpoint_revision, 1);
    }

    #[test]
    fn writeback_matrix_suppresses_observe_only_diagnostic_and_cli_compat() {
        let mut observation = observation(HealthObservationOutcome::HardFail);
        observation.writeback_mode = HealthWritebackMode::ObserveOnly;
        assert_eq!(
            writeback_decision(&observation),
            HealthWritebackDecision::ObserveOnly
        );

        observation.writeback_mode = HealthWritebackMode::Authoritative;
        observation.traffic_equivalence = TrafficEquivalence::Diagnostic;
        assert_eq!(
            writeback_decision(&observation),
            HealthWritebackDecision::Suppressed
        );

        observation.traffic_equivalence = TrafficEquivalence::SyntheticCliCompat;
        assert_eq!(
            writeback_decision(&observation),
            HealthWritebackDecision::Suppressed
        );

        observation.traffic_equivalence = TrafficEquivalence::RealUserTraffic;
        assert_eq!(
            writeback_decision(&observation),
            HealthWritebackDecision::Write
        );
    }

    fn observation(outcome: HealthObservationOutcome) -> HealthObservation {
        HealthObservation {
            id: "obs-1".to_string(),
            station_key_id: "key-1".to_string(),
            target_result_id: Some("target-1".to_string()),
            source: HealthObservationSource::ProxyRequest,
            source_event_id: "event-1".to_string(),
            observed_at_ms: 10_000,
            endpoint_revision: 1,
            outcome,
            failure_kind: Some("rate_limit".to_string()),
            latency_ms: Some(100),
            retry_after_ms: None,
            error_summary: None,
            writeback_mode: HealthWritebackMode::Authoritative,
            traffic_equivalence: TrafficEquivalence::RealUserTraffic,
        }
    }
}
