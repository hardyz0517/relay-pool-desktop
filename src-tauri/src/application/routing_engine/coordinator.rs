use std::collections::BTreeSet;

use super::{
    intelligent_planner::{plan_snapshot, PlannerError, RoutePlan},
    planning_snapshot::PlanningSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CoordinatorFailure {
    NoCandidate,
    Planning(PlannerError),
    TargetStale {
        station_key_id: String,
        expected_revision: i64,
        actual_revision: i64,
    },
    Execution {
        station_key_id: String,
        retryable: bool,
        failure_domain: Option<String>,
    },
    ReplanLimit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecisionTraceRound {
    pub(crate) round: u32,
    pub(crate) plan: RoutePlan,
    pub(crate) attempted_station_key_id: String,
    pub(crate) outcome: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoordinatorResult {
    pub(crate) selected_station_key_id: String,
    pub(crate) rounds: Vec<DecisionTraceRound>,
}

pub(crate) trait TargetFence {
    fn current_endpoint_revision(&self, station_key_id: &str) -> Option<i64>;
}

pub(crate) trait AttemptExecutor {
    fn execute(&mut self, station_key_id: &str) -> Result<(), ExecutionFailure>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionFailure {
    pub(crate) retryable: bool,
    pub(crate) failure_domain: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CoordinatorConfig {
    pub(crate) max_rounds: u32,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self { max_rounds: 3 }
    }
}

/// Owns planning rounds and fallback. Transport and streaming remain behind
/// AttemptExecutor, so a failed body cannot accidentally mutate planner state.
pub(crate) fn coordinate<T, E>(
    snapshot: &PlanningSnapshot,
    root_seed: &[u8],
    config: CoordinatorConfig,
    fence: &T,
    executor: &mut E,
) -> Result<CoordinatorResult, CoordinatorFailure>
where
    T: TargetFence,
    E: AttemptExecutor,
{
    let mut working = snapshot.clone();
    let mut excluded = BTreeSet::new();
    let mut rounds = Vec::new();
    for round in 0..config.max_rounds.max(1) {
        working
            .candidates
            .retain(|candidate| !excluded.contains(&candidate.station_key_id));
        let plan = plan_snapshot(&working, root_seed, u64::from(round + 1)).map_err(|error| {
            if matches!(error, PlannerError::NoEligibleCandidate) {
                CoordinatorFailure::NoCandidate
            } else {
                CoordinatorFailure::Planning(error)
            }
        })?;
        let selected = plan.selected_station_key_id.clone();
        let candidate = working
            .candidates
            .iter()
            .find(|candidate| candidate.station_key_id == selected)
            .ok_or(CoordinatorFailure::NoCandidate)?;
        let actual =
            fence
                .current_endpoint_revision(&selected)
                .ok_or(CoordinatorFailure::TargetStale {
                    station_key_id: selected.clone(),
                    expected_revision: candidate.endpoint_revision,
                    actual_revision: -1,
                })?;
        if actual != candidate.endpoint_revision {
            excluded.insert(selected.clone());
            rounds.push(DecisionTraceRound {
                round,
                plan,
                attempted_station_key_id: selected.clone(),
                outcome: "stale_target",
            });
            continue;
        }
        match executor.execute(&selected) {
            Ok(()) => {
                rounds.push(DecisionTraceRound {
                    round,
                    plan,
                    attempted_station_key_id: selected.clone(),
                    outcome: "success",
                });
                return Ok(CoordinatorResult {
                    selected_station_key_id: selected,
                    rounds,
                });
            }
            Err(failure) => {
                let outcome = if failure.retryable {
                    "retryable_failure"
                } else {
                    "terminal_failure"
                };
                rounds.push(DecisionTraceRound {
                    round,
                    plan,
                    attempted_station_key_id: selected.clone(),
                    outcome,
                });
                if !failure.retryable {
                    return Err(CoordinatorFailure::Execution {
                        station_key_id: selected,
                        retryable: false,
                        failure_domain: failure.failure_domain,
                    });
                }
                excluded.insert(selected);
            }
        }
    }
    Err(CoordinatorFailure::ReplanLimit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::routing_engine::{
        algorithm_profile::DispatchAlgorithmProfile,
        planning_snapshot::{CandidateSnapshot, RuntimeOverlaySnapshot},
    };
    use crate::models::routing_policy::RoutingPolicyConfigV1;

    struct Fence;
    impl TargetFence for Fence {
        fn current_endpoint_revision(&self, _: &str) -> Option<i64> {
            Some(1)
        }
    }
    struct Exec {
        calls: u32,
    }
    impl AttemptExecutor for Exec {
        fn execute(&mut self, _: &str) -> Result<(), ExecutionFailure> {
            self.calls += 1;
            if self.calls == 1 {
                Err(ExecutionFailure {
                    retryable: true,
                    failure_domain: None,
                })
            } else {
                Ok(())
            }
        }
    }

    fn snapshot() -> PlanningSnapshot {
        PlanningSnapshot {
            snapshot_id: "s".into(),
            durable_revision: 1,
            policy: RoutingPolicyConfigV1::default(),
            profile: DispatchAlgorithmProfile::default(),
            candidates: vec![CandidateSnapshot {
                station_key_id: "a".into(),
                station_id: "st".into(),
                endpoint_revision: 1,
                credential_revision: 1,
                credential_available: true,
                hard_eligible: true,
                backup_only: false,
                depleted: false,
                capability_basis_points: 10_000,
                reliability_basis_points: 8_000,
                responsiveness_basis_points: 8_000,
                cost_basis_points: Some(8_000),
                preference_basis_points: 5_000,
                failure_domains: vec!["st".into()],
            }],
            runtime: RuntimeOverlaySnapshot {
                runtime_instance_id: "r".into(),
                runtime_revision: 1,
                candidate_set_revision: 1,
                in_flight: 0,
                max_concurrency: 1,
                affinity_station_key_id: None,
            },
        }
    }

    #[test]
    fn coordinator_records_retry_and_is_bounded() {
        let mut exec = Exec { calls: 0 };
        let result = coordinate(
            &snapshot(),
            b"seed",
            CoordinatorConfig { max_rounds: 2 },
            &Fence,
            &mut exec,
        );
        assert!(result.is_err());
        assert_eq!(exec.calls, 1);
    }
}
