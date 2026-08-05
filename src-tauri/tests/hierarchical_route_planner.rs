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
        pub(crate) mod request {
            use std::collections::BTreeSet;

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum OrderingProfile {
                PriorityFirst,
                CostFirst,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct RouteRequestFacts {
                ordering_profile: OrderingProfile,
            }

            impl RouteRequestFacts {
                pub(crate) fn new(ordering_profile: OrderingProfile) -> Self {
                    Self { ordering_profile }
                }

                pub(crate) fn ordering_profile(&self) -> OrderingProfile {
                    self.ordering_profile
                }
            }

            #[derive(Debug, Clone, PartialEq, Eq)]
            pub(crate) struct RouteProgressView {
                pub(crate) actual_attempt_exclusions: BTreeSet<String>,
                pub(crate) runtime_rebuild_count: u32,
            }

            impl RouteProgressView {
                pub(crate) fn excludes_station_key(&self, station_key_id: &str) -> bool {
                    self.actual_attempt_exclusions.contains(station_key_id)
                }
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct PlanningRoundContext {
                pub(crate) request: RouteRequestFacts,
                pub(crate) progress: RouteProgressView,
                pub(crate) snapshot_id: String,
                pub(crate) runtime_overlay_revision: u64,
            }
        }

        pub(crate) mod eligibility {
            pub(crate) use crate::eligibility::*;
        }
        pub(crate) mod selector {
            pub(crate) use crate::selector::*;
        }
    }
}

#[path = "../src/application/routing_engine/eligibility.rs"]
mod eligibility;
#[path = "../src/application/routing_engine/planner_legacy.rs"]
mod planner_legacy;
#[path = "../src/application/routing_engine/selector.rs"]
mod selector;

use std::collections::BTreeSet;

use application::{
    operational_facts::{
        balance_projector::BalanceProjectionStatus,
        candidate_projector::{
            CandidateBalanceProjection, CandidateIdentityProjection, CandidatePolicyProjection,
            CandidatePricingProjection, CandidateProvenanceProjection, RouteCandidateProjection,
        },
        pricing_projector::RoutingCostBasis,
    },
    routing_engine::request::{
        OrderingProfile, PlanningRoundContext, RouteProgressView, RouteRequestFacts,
    },
};
use planner_legacy::{ordered_plan_candidates, plan_candidate_count, plan_route, PlanningInput};
use selector::{AvailabilityTier, RoutePlannerError};

fn context(profile: OrderingProfile) -> PlanningRoundContext {
    PlanningRoundContext {
        request: RouteRequestFacts::new(profile),
        progress: RouteProgressView {
            actual_attempt_exclusions: BTreeSet::new(),
            runtime_rebuild_count: 0,
        },
        snapshot_id: "snapshot-a".to_string(),
        runtime_overlay_revision: 7,
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

#[test]
fn fixed_gate_order_and_progress_exclusion_are_reported_without_mutating_progress() {
    let mut rejected = candidate("bad", 1);
    rejected.hard_rejection_codes =
        vec!["tag_mismatch", "credential_missing", "health_hard_reject"];
    let mut excluded = candidate("attempted", 2);
    excluded.hard_rejection_codes = Vec::new();
    let mut context = context(OrderingProfile::PriorityFirst);
    context
        .progress
        .actual_attempt_exclusions
        .insert("attempted".to_string());

    let plan = plan_route(PlanningInput {
        context: &context,
        candidates: &[rejected, excluded],
        affinity_station_key_id: None,
    })
    .expect("plan");

    assert_eq!(context.progress.runtime_rebuild_count, 0);
    assert_eq!(plan.rejections[0].station_key_id, "bad");
    assert_eq!(plan.rejections[0].code, "credential_missing");
    assert_eq!(plan.rejections[1].station_key_id, "attempted");
    assert_eq!(plan.rejections[1].code, "actual_attempt_excluded");
    assert_eq!(plan.selected_station_key_id, None);
}

#[test]
fn priority_first_strictly_layers_primary_backup_and_depleted_emergency() {
    let mut primary = candidate("primary", 10);
    primary.pricing.comparison_value = Some(2.0);
    let mut backup = candidate("backup", 0);
    backup.policy.backup_only = true;
    backup.pricing.comparison_value = Some(0.5);
    let mut depleted = candidate("depleted", 0);
    depleted.balance.status = BalanceProjectionStatus::DepletedEmergency;
    depleted.pricing.comparison_value = Some(0.1);

    let plan = plan_route(PlanningInput {
        context: &context(OrderingProfile::PriorityFirst),
        candidates: &[depleted, backup, primary],
        affinity_station_key_id: None,
    })
    .expect("plan");

    assert_eq!(plan.strata.len(), 3);
    assert_eq!(plan.strata[0].tier, AvailabilityTier::Primary);
    assert_eq!(plan.strata[0].candidate_ids(), vec!["primary"]);
    assert_eq!(plan.strata[1].tier, AvailabilityTier::ConfiguredBackup);
    assert_eq!(plan.strata[1].candidate_ids(), vec!["backup"]);
    assert_eq!(plan.strata[2].tier, AvailabilityTier::DepletedEmergency);
    assert_eq!(plan.strata[2].candidate_ids(), vec!["depleted"]);
}

#[test]
fn cost_first_uses_exact_comparable_basis_before_priority_and_does_not_cross_currency() {
    let mut expensive_priority = candidate("expensive-priority", 0);
    expensive_priority.pricing.comparison_value = Some(2.0);
    let mut cheap_same_basis = candidate("cheap-same-basis", 10);
    cheap_same_basis.pricing.comparison_value = Some(1.0);
    let mut different_currency = candidate("different-currency", 0);
    different_currency.pricing.comparison_value = Some(0.1);
    different_currency.pricing.currency = Some("EUR".to_string());

    let plan = plan_route(PlanningInput {
        context: &context(OrderingProfile::CostFirst),
        candidates: &[different_currency, expensive_priority, cheap_same_basis],
        affinity_station_key_id: None,
    })
    .expect("plan");

    assert_eq!(
        plan.strata[0].candidate_ids(),
        vec![
            "cheap-same-basis",
            "expensive-priority",
            "different-currency"
        ]
    );
    assert_eq!(
        plan.selected_station_key_id.as_deref(),
        Some("cheap-same-basis")
    );
    assert_eq!(plan_candidate_count(&plan), 3);
    assert_eq!(
        ordered_plan_candidates(&plan)
            .into_iter()
            .map(|candidate| candidate.station_key_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "cheap-same-basis",
            "expensive-priority",
            "different-currency"
        ]
    );
}

#[test]
fn planner_fixture_pricing_basis_contract_covers_unpriced_and_not_applicable() {
    assert_eq!(format!("{:?}", RoutingCostBasis::Unpriced), "Unpriced");
    assert_eq!(
        format!("{:?}", RoutingCostBasis::NotApplicable),
        "NotApplicable"
    );
}

#[test]
fn affinity_cannot_restore_rejected_candidates_or_cross_cost_first_band() {
    let mut cheap = candidate("cheap", 10);
    cheap.pricing.comparison_value = Some(1.0);
    let mut sticky = candidate("sticky", 0);
    sticky.pricing.comparison_value = Some(1.20);
    let mut rejected_sticky = candidate("rejected-sticky", 0);
    rejected_sticky.hard_rejection_codes = vec!["health_hard_reject"];

    let plan = plan_route(PlanningInput {
        context: &context(OrderingProfile::CostFirst),
        candidates: &[sticky.clone(), cheap.clone(), rejected_sticky],
        affinity_station_key_id: Some("rejected-sticky"),
    })
    .expect("plan");

    assert_eq!(plan.selected_station_key_id.as_deref(), Some("cheap"));

    let plan = plan_route(PlanningInput {
        context: &context(OrderingProfile::CostFirst),
        candidates: &[sticky, cheap],
        affinity_station_key_id: Some("sticky"),
    })
    .expect("plan");

    assert_eq!(plan.selected_station_key_id.as_deref(), Some("cheap"));
}

#[test]
fn candidate_limit_is_hard_bounded() {
    let candidates = (0..1025)
        .map(|index| candidate(&format!("key-{index}"), index))
        .collect::<Vec<_>>();

    let error = plan_route(PlanningInput {
        context: &context(OrderingProfile::PriorityFirst),
        candidates: &candidates,
        affinity_station_key_id: None,
    })
    .expect_err("candidate limit");

    assert_eq!(
        error,
        RoutePlannerError::CandidateLimitExceeded {
            actual: 1025,
            limit: 1024
        }
    );
}
