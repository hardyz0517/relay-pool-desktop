#![allow(dead_code)]

mod application {
    pub(crate) mod operational_facts {
        pub(crate) mod balance_projector {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum BalanceProjectionStatus {
                Healthy,
                DepletedEmergency,
            }
        }

        pub(crate) mod pricing_projector {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum RoutingCostBasis {
                ExactPrice,
                MultiplierProxy,
                Unpriced,
                NotApplicable,
            }
        }

        pub(crate) mod candidate_projector {
            use super::{
                balance_projector::BalanceProjectionStatus, pricing_projector::RoutingCostBasis,
            };

            #[derive(Debug, Clone, PartialEq, Eq)]
            pub(crate) struct CandidateIdentityProjection {
                pub(crate) station_key_id: String,
                pub(crate) station_id: String,
                pub(crate) endpoint_revision: i64,
                pub(crate) sanitized_origin: String,
                pub(crate) credential_available: bool,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CandidatePolicyProjection {
                pub(crate) backup_only: bool,
                pub(crate) preferred_model_match: bool,
                pub(crate) affinity_eligible: bool,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CandidatePricingProjection {
                pub(crate) basis: RoutingCostBasis,
                pub(crate) comparison_value: Option<f64>,
                pub(crate) currency: Option<String>,
                pub(crate) unit: Option<String>,
                pub(crate) estimated_input_price: Option<f64>,
                pub(crate) estimated_output_price: Option<f64>,
                pub(crate) estimated_fixed_price: Option<f64>,
                pub(crate) status_label: String,
            }

            #[derive(Debug, Clone, PartialEq, Eq)]
            pub(crate) struct CandidateBalanceProjection {
                pub(crate) status: BalanceProjectionStatus,
            }

            #[derive(Debug, Clone, PartialEq, Eq)]
            pub(crate) struct CandidateProvenanceProjection {
                pub(crate) snapshot_id: String,
                pub(crate) fact_version_vector: String,
                pub(crate) projector_version: &'static str,
                pub(crate) endpoint_revision: i64,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct RouteCandidateProjection {
                pub(crate) identity: CandidateIdentityProjection,
                pub(crate) priority: i64,
                pub(crate) policy: CandidatePolicyProjection,
                pub(crate) pricing: CandidatePricingProjection,
                pub(crate) balance: CandidateBalanceProjection,
                pub(crate) provenance: CandidateProvenanceProjection,
                pub(crate) hard_rejection_codes: Vec<&'static str>,
            }
        }
    }

    pub(crate) mod routing_engine {
        pub(crate) mod capacity {
            pub(crate) use crate::capacity::*;
        }
        pub(crate) mod eligibility {
            pub(crate) use crate::eligibility::*;
        }
        pub(crate) mod planner {
            pub(crate) use crate::planner::*;
        }
        pub(crate) mod request {
            pub(crate) use crate::request::*;
        }
        pub(crate) mod selector {
            pub(crate) use crate::selector::*;
        }
    }
}

#[path = "../src/application/routing_engine/capacity.rs"]
mod capacity;
#[path = "../src/application/routing_engine/controller.rs"]
mod controller;
#[path = "../src/application/routing_engine/eligibility.rs"]
mod eligibility;
#[path = "../src/application/routing_engine/planner.rs"]
mod planner;
#[path = "../src/application/routing_engine/request.rs"]
mod request;
#[path = "../src/application/routing_engine/selector.rs"]
mod selector;

use std::collections::BTreeMap;

use application::operational_facts::{
    balance_projector::BalanceProjectionStatus,
    candidate_projector::{
        CandidateBalanceProjection, CandidateIdentityProjection, CandidatePolicyProjection,
        CandidatePricingProjection, CandidateProvenanceProjection, RouteCandidateProjection,
    },
    pricing_projector::RoutingCostBasis,
};
use capacity::{CapacityConstraintKey, CompositeCapacityRegistry, ProviderAccountConstraint};
use controller::{
    ActualAttemptTerminal, CandidateAdmissionProfile, ControllerDecision, ControllerFailureKind,
    ControllerPlanningInput, ControllerTransition, FallbackPolicy, RouteAdmissionController,
    RouteControllerSettings,
};
use request::{
    CanonicalRouteRequest, GroupFilterMode, OrderingProfile, RouteKind, RouteRequestClassifier,
    RouteRequestFacts, ValidatedLocalRouteSettings,
};
use selector::MAX_ROUTE_PLAN_CANDIDATES;

fn request_facts() -> RouteRequestFacts {
    RouteRequestClassifier::classify(
        CanonicalRouteRequest {
            route_kind: RouteKind::Inference,
            requested_model: Some("gpt-test".to_string()),
            stream: true,
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
            allow_depleted_fallback: true,
            affinity_enabled: false,
        },
        1_000,
    )
}

fn controller(policy: FallbackPolicy) -> RouteAdmissionController {
    RouteAdmissionController::new(
        request_facts(),
        RouteControllerSettings {
            deadline_ms: 10_000,
            initial_snapshot_id: "snapshot-a".to_string(),
            initial_runtime_overlay_revision: 1,
            initial_durable_generation: 1,
            fallback_policy: policy,
        },
        16,
    )
}

fn retry_safe_policy() -> FallbackPolicy {
    FallbackPolicy {
        has_stable_idempotency_key: true,
        non_idempotent: false,
    }
}

fn candidate(id: &str, priority: i64) -> RouteCandidateProjection {
    RouteCandidateProjection {
        identity: CandidateIdentityProjection {
            station_key_id: id.to_string(),
            station_id: format!("station-{id}"),
            endpoint_revision: 3,
            sanitized_origin: "https://relay.example".to_string(),
            credential_available: true,
        },
        priority,
        policy: CandidatePolicyProjection {
            backup_only: false,
            preferred_model_match: false,
            affinity_eligible: true,
        },
        pricing: CandidatePricingProjection {
            basis: RoutingCostBasis::ExactPrice,
            comparison_value: Some(1.0),
            currency: Some("USD".to_string()),
            unit: Some("per_1m_tokens".to_string()),
            estimated_input_price: Some(1.0),
            estimated_output_price: None,
            estimated_fixed_price: None,
            status_label: "priced".to_string(),
        },
        balance: CandidateBalanceProjection {
            status: BalanceProjectionStatus::Healthy,
        },
        provenance: CandidateProvenanceProjection {
            snapshot_id: "snapshot-a".to_string(),
            fact_version_vector: "station=1,key=2,settings=3".to_string(),
            projector_version: "route_candidate_projection_v1",
            endpoint_revision: 3,
        },
        hard_rejection_codes: Vec::new(),
    }
}

fn profile(candidate: &RouteCandidateProjection) -> CandidateAdmissionProfile {
    CandidateAdmissionProfile {
        endpoint_revision: candidate.identity.endpoint_revision,
        expected_credential_revision: 11,
        credential_revision: 11,
        durable_generation: 1,
        global_max_concurrency: 16,
        station_account_max_concurrency: 16,
        station_key_max_concurrency: 1,
        provider_account_constraint: ProviderAccountConstraint::NotApplicable,
        half_open_probe_id: None,
    }
}

fn profiles(
    candidates: &[RouteCandidateProjection],
) -> BTreeMap<String, CandidateAdmissionProfile> {
    candidates
        .iter()
        .map(|candidate| {
            (
                candidate.identity.station_key_id.clone(),
                profile(candidate),
            )
        })
        .collect()
}

fn next_input<'a>(
    candidates: &'a [RouteCandidateProjection],
    profiles: &'a BTreeMap<String, CandidateAdmissionProfile>,
    capacity: &'a CompositeCapacityRegistry,
    runtime_revision: u64,
    now_ms: i64,
) -> ControllerPlanningInput<'a> {
    ControllerPlanningInput {
        candidates,
        affinity_station_key_id: None,
        profiles,
        capacity,
        current_runtime_overlay_revision: runtime_revision,
        now_ms,
        max_waiters_per_constraint: 4,
    }
}

fn selected_id(decision: ControllerDecision) -> String {
    match decision {
        ControllerDecision::Selected(selected) => selected.candidate.station_key_id,
        other => panic!("expected selected route, got {other:?}"),
    }
}

#[test]
fn capacity_miss_continues_plan_without_attempt_progress_or_retry_token() {
    let mut primary = candidate("primary", 10);
    let mut backup = candidate("backup", 1);
    backup.policy.backup_only = true;
    primary.policy.backup_only = false;
    let candidates = vec![primary.clone(), backup.clone()];
    let profiles = profiles(&candidates);
    let capacity = CompositeCapacityRegistry::default();
    let _blocking_primary = capacity
        .try_acquire(
            profiles["primary"].capacity_request(&selector::RoutePlanCandidate {
                station_key_id: "primary".to_string(),
                station_id: "station-primary".to_string(),
                endpoint_revision: 3,
                priority: 10,
                tier: selector::AvailabilityTier::Primary,
                pricing: selector::RoutePlanPricingSnapshot {
                    basis: RoutingCostBasis::ExactPrice,
                    currency: Some("USD".to_string()),
                    unit: Some("per_1m_tokens".to_string()),
                    estimated_input_price: Some(1.0),
                    estimated_output_price: None,
                    estimated_fixed_price: None,
                    status_label: "priced".to_string(),
                },
                evidence: Vec::new(),
            }),
        )
        .expect("blocking primary key lease");

    let mut controller = controller(retry_safe_policy());
    let decision = controller
        .next(next_input(&candidates, &profiles, &capacity, 1, 1_100))
        .expect("backup selected");

    assert_eq!(selected_id(decision), "backup");
    assert_eq!(controller.progress_view().attempt_count, 0);
    assert!(controller
        .progress_view()
        .actual_attempt_exclusions
        .is_empty());
    assert_eq!(
        controller.pass_capacity_state().unavailable_this_pass.len(),
        1
    );
    assert!(controller
        .trace()
        .iter()
        .any(|event| event.transition == ControllerTransition::CapacityMiss));
}

#[test]
fn actual_terminal_adds_monotonic_exclusion_and_prevents_duplicate_key_attempts() {
    let candidates = vec![candidate("a", 1), candidate("b", 2)];
    let profiles = profiles(&candidates);
    let capacity = CompositeCapacityRegistry::default();
    let mut controller = controller(retry_safe_policy());

    let first = match controller
        .next(next_input(&candidates, &profiles, &capacity, 1, 1_100))
        .expect("first selected")
    {
        ControllerDecision::Selected(selected) => selected,
        other => panic!("expected selected route, got {other:?}"),
    };
    assert_eq!(first.candidate.station_key_id, "a");
    controller
        .record_actual_terminal(first, ActualAttemptTerminal::FailedBeforeCommit)
        .expect("retry-safe terminal");

    let second_id = selected_id(
        controller
            .next(next_input(&candidates, &profiles, &capacity, 1, 1_200))
            .expect("second selected"),
    );
    assert_eq!(second_id, "b");
    assert_eq!(controller.progress_view().attempt_count, 1);
    assert!(controller
        .progress_view()
        .actual_attempt_exclusions
        .contains("a"));
}

#[test]
fn wait_wakeup_clears_pass_state_refreshes_overlay_and_allows_unattempted_key() {
    let candidates = vec![candidate("a", 10)];
    let profiles = profiles(&candidates);
    let capacity = CompositeCapacityRegistry::default();
    let blocking = capacity
        .try_acquire(
            profiles["a"].capacity_request(&selector::RoutePlanCandidate {
                station_key_id: "a".to_string(),
                station_id: "station-a".to_string(),
                endpoint_revision: 3,
                priority: 10,
                tier: selector::AvailabilityTier::Primary,
                pricing: selector::RoutePlanPricingSnapshot {
                    basis: RoutingCostBasis::ExactPrice,
                    currency: Some("USD".to_string()),
                    unit: Some("per_1m_tokens".to_string()),
                    estimated_input_price: Some(1.0),
                    estimated_output_price: None,
                    estimated_fixed_price: None,
                    status_label: "priced".to_string(),
                },
                evidence: Vec::new(),
            }),
        )
        .expect("blocking a");
    let mut controller = controller(retry_safe_policy());

    let wait = controller
        .next(next_input(&candidates, &profiles, &capacity, 1, 1_100))
        .expect("wait entered");
    assert!(matches!(
        wait,
        ControllerDecision::Wait {
            constraint: CapacityConstraintKey::StationKey(_),
            ..
        }
    ));
    assert_eq!(
        controller.pass_capacity_state().unavailable_this_pass.len(),
        1
    );

    drop(wait);
    drop(blocking);
    controller.record_wait_wakeup(2);
    assert!(controller
        .pass_capacity_state()
        .unavailable_this_pass
        .is_empty());
    let selected = selected_id(
        controller
            .next(next_input(&candidates, &profiles, &capacity, 2, 1_200))
            .expect("selected after wake"),
    );
    assert_eq!(selected, "a");
    assert!(controller
        .progress_view()
        .actual_attempt_exclusions
        .is_empty());
}

#[test]
fn runtime_generation_replan_is_bounded_to_eight_runtime_only_rebuilds() {
    let candidates = vec![candidate("a", 10)];
    let profiles = profiles(&candidates);
    let capacity = CompositeCapacityRegistry::default();
    let mut controller = controller(retry_safe_policy());

    for revision in 2..=9 {
        assert!(matches!(
            controller
                .next(next_input(
                    &candidates,
                    &profiles,
                    &capacity,
                    revision,
                    1_100
                ))
                .expect("runtime replan"),
            ControllerDecision::Replan {
                reason: ControllerTransition::RuntimeReplan
            }
        ));
    }
    let failure = controller
        .next(next_input(&candidates, &profiles, &capacity, 10, 1_100))
        .expect_err("runtime replan limit");
    assert_eq!(failure.kind, ControllerFailureKind::TemporaryHealth);
}

#[test]
fn config_or_credential_fence_change_allows_one_batch_snapshot_rebuild_then_fails_stably() {
    let candidates = vec![candidate("a", 10)];
    let mut profiles = profiles(&candidates);
    profiles.get_mut("a").expect("profile").endpoint_revision = 4;
    profiles.get_mut("a").expect("profile").credential_revision = 12;
    profiles.get_mut("a").expect("profile").durable_generation = 2;
    let capacity = CompositeCapacityRegistry::default();
    let mut controller = controller(retry_safe_policy());

    assert!(matches!(
        controller
            .next(next_input(&candidates, &profiles, &capacity, 1, 1_100))
            .expect("snapshot rebuild"),
        ControllerDecision::Replan {
            reason: ControllerTransition::SnapshotRebuild
        }
    ));
    assert_eq!(controller.progress_view().snapshot_rebuild_count, 1);

    let failure = controller
        .next(next_input(&candidates, &profiles, &capacity, 1, 1_200))
        .expect_err("second fence change");
    assert_eq!(failure.kind, ControllerFailureKind::ConfigUnstable);
}

#[test]
fn max_attempts_include_initial_and_possibly_accepted_non_idempotent_blocks_retry() {
    let candidates = vec![
        candidate("a", 1),
        candidate("b", 2),
        candidate("c", 3),
        candidate("d", 4),
    ];
    let profiles = profiles(&candidates);
    let capacity = CompositeCapacityRegistry::default();
    let mut bounded = controller(retry_safe_policy());

    for now_ms in [1_100, 1_200, 1_300] {
        let selected = match bounded
            .next(next_input(&candidates, &profiles, &capacity, 1, now_ms))
            .expect("selected within max attempts")
        {
            ControllerDecision::Selected(selected) => selected,
            other => panic!("expected selected route, got {other:?}"),
        };
        bounded
            .record_actual_terminal(selected, ActualAttemptTerminal::FailedBeforeCommit)
            .expect("retry safe");
    }
    let failure = bounded
        .next(next_input(&candidates, &profiles, &capacity, 1, 1_400))
        .expect_err("attempt limit");
    assert_eq!(failure.kind, ControllerFailureKind::AttemptLimit);

    let mut unsafe_retry = controller(FallbackPolicy {
        has_stable_idempotency_key: false,
        non_idempotent: true,
    });
    let first = match unsafe_retry
        .next(next_input(&candidates, &profiles, &capacity, 1, 1_100))
        .expect("selected")
    {
        ControllerDecision::Selected(selected) => selected,
        other => panic!("expected selected route, got {other:?}"),
    };
    let failure = unsafe_retry
        .record_actual_terminal(first, ActualAttemptTerminal::PossiblyAccepted)
        .expect_err("commit uncertain");
    assert_eq!(failure.kind, ControllerFailureKind::CommitUncertain);
}

#[test]
fn typed_failures_cover_deadline_no_eligible_and_candidate_limit() {
    let empty_profiles = BTreeMap::new();
    let capacity = CompositeCapacityRegistry::default();
    let mut deadline = controller(retry_safe_policy());
    let failure = deadline
        .next(next_input(
            &[candidate("a", 10)],
            &empty_profiles,
            &capacity,
            1,
            10_000,
        ))
        .expect_err("deadline");
    assert_eq!(failure.kind, ControllerFailureKind::Deadline);

    let mut no_eligible_candidate = candidate("bad", 1);
    no_eligible_candidate.hard_rejection_codes = vec!["credential_missing"];
    let mut no_eligible = controller(retry_safe_policy());
    let failure = no_eligible
        .next(next_input(
            &[no_eligible_candidate],
            &empty_profiles,
            &capacity,
            1,
            1_100,
        ))
        .expect_err("no eligible");
    assert_eq!(failure.kind, ControllerFailureKind::NoEligible);

    let too_many = (0..=MAX_ROUTE_PLAN_CANDIDATES)
        .map(|index| candidate(&format!("key-{index}"), index as i64))
        .collect::<Vec<_>>();
    let profiles = profiles(&too_many);
    let mut candidate_limit = controller(retry_safe_policy());
    let failure = candidate_limit
        .next(next_input(&too_many, &profiles, &capacity, 1, 1_100))
        .expect_err("candidate limit");
    assert_eq!(failure.kind, ControllerFailureKind::CandidateLimit);
}
