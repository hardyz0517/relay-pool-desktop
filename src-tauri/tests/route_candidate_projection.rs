#![allow(dead_code)]

#[path = "../src/models/operational/mod.rs"]
mod operational_model;

mod models {
    pub(crate) mod operational {
        pub(crate) use crate::operational_model::*;
    }
}

#[path = "../src/application/operational_facts/group_projector.rs"]
mod group_projector;
#[path = "../src/application/operational_facts/multiplier_projector.rs"]
mod multiplier_projector;
#[path = "../src/application/routing_engine/request.rs"]
mod request;
mod pricing_projector {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum PricingRouteKind {
        Inference,
        ModelCatalog,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum RoutingCostBasis {
        ExactPrice,
        MultiplierProxy,
        Unpriced,
        NotApplicable,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub(crate) struct RequestCostComparisonContext {
        pub(crate) route_kind: PricingRouteKind,
        pub(crate) basis: RoutingCostBasis,
        pub(crate) comparison_value: Option<f64>,
        pub(crate) reason: Option<&'static str>,
        pub(crate) currency: Option<String>,
        pub(crate) unit: Option<String>,
        pub(crate) source_chain: Vec<String>,
        pub(crate) observed_at: Option<String>,
        pub(crate) confidence: Option<f64>,
    }
}
#[path = "../src/application/operational_facts/balance_projector.rs"]
mod balance_projector;
#[path = "../src/application/operational_facts/capability_projector.rs"]
mod capability_projector;
#[path = "../src/application/operational_facts/health_projector.rs"]
mod health_projector;

mod application {
    pub(crate) mod routing_engine {
        pub(crate) mod request {
            pub(crate) use crate::request::*;
        }
    }

    pub(crate) mod operational_facts {
        pub(crate) mod balance_projector {
            pub(crate) use crate::balance_projector::*;
        }
        pub(crate) mod capability_projector {
            pub(crate) use crate::capability_projector::*;
        }
        pub(crate) mod group_projector {
            pub(crate) use crate::group_projector::*;
        }
        pub(crate) mod health_projector {
            pub(crate) use crate::health_projector::*;
        }
        pub(crate) mod multiplier_projector {
            pub(crate) use crate::multiplier_projector::*;
        }
        pub(crate) mod pricing_projector {
            pub(crate) use crate::pricing_projector::*;
        }
    }
}

#[path = "../src/application/operational_facts/candidate_projector.rs"]
mod candidate_projector;

use balance_projector::{BalanceProjection, BalanceProjectionStatus};
use candidate_projector::{
    project_route_candidate, CandidateIdentityProjection, CandidateOperationalProjections,
    CapabilityProjectionSet, CapacityProjection, CapacityScope, CapacityScopeSnapshot,
    HealthProjectionSet, ROUTE_CANDIDATE_PROJECTION_VERSION,
};
use capability_projector::{
    CapabilityDecision, CapabilityFeature, CapabilityProjection, CapabilityProtocol,
    CapabilitySubject,
};
use group_projector::{GroupIdentity, GroupProjection, ProjectionTrace};
use health_projector::{EffectiveHealthProjection, HealthAdmission, HealthProjectionTarget};
use models::operational::{
    BalanceScope, EndpointId, EndpointRef, EndpointRevision, PriceConfidence, RateMultiplier,
    RecordRevision, StationId, UnixMillis,
};
use multiplier_projector::{
    MultiplierEvidenceKind, MultiplierProjection, MultiplierResolutionStatus,
};
use pricing_projector::{RequestCostComparisonContext, RoutingCostBasis};
use request::{
    CanonicalRouteRequest, GroupFilterMode, OrderingProfile, RouteKind, RouteProgress,
    RouteRequestClassifier, ValidatedLocalRouteSettings,
};

fn now() -> UnixMillis {
    UnixMillis::new(1_000).expect("time")
}

fn trace(reason: &'static str) -> ProjectionTrace {
    ProjectionTrace::new(
        vec!["test"],
        PriceConfidence::new(1.0).expect("confidence"),
        now(),
        reason,
        vec![RecordRevision::new(1).expect("revision")],
    )
}

fn local_settings(profile: OrderingProfile) -> ValidatedLocalRouteSettings {
    ValidatedLocalRouteSettings {
        ordering_profile: profile,
        max_rate_multiplier: Some(2.0),
        group_filter_mode: GroupFilterMode::Required,
        required_group_stable_key: Some("binding:group-a".to_string()),
        preferred_models: vec!["gpt-5-mini".to_string()],
        required_tags: vec!["fast".to_string()],
        allow_depleted_fallback: false,
        affinity_enabled: true,
    }
}

fn request_facts(route_kind: RouteKind) -> request::RouteRequestFacts {
    RouteRequestClassifier::classify(
        CanonicalRouteRequest {
            route_kind,
            requested_model: Some("gpt-5-mini".to_string()),
            stream: true,
            uses_tools: false,
            uses_vision: false,
            uses_reasoning: false,
            untrusted_headers: vec![(
                "x-relay-ordering-profile".to_string(),
                "cost_first".to_string(),
            )],
        },
        local_settings(OrderingProfile::PriorityFirst),
        10,
    )
}

fn capability(subject: CapabilitySubject, decision: CapabilityDecision) -> CapabilityProjection {
    CapabilityProjection {
        subject,
        truth: models::operational::CapabilityVerdict::Supported,
        decision,
        winner: None,
        overridden: Vec::new(),
        conflict_reason: None,
    }
}

fn capabilities() -> CapabilityProjectionSet {
    CapabilityProjectionSet {
        protocol: capability(
            CapabilitySubject::Protocol(CapabilityProtocol::ChatCompletions),
            CapabilityDecision::Allow,
        ),
        model: capability(
            CapabilitySubject::Model("gpt-5-mini".to_string()),
            CapabilityDecision::Allow,
        ),
        stream: capability(
            CapabilitySubject::Feature(CapabilityFeature::Stream),
            CapabilityDecision::Allow,
        ),
        tools: capability(
            CapabilitySubject::Feature(CapabilityFeature::Tools),
            CapabilityDecision::Reject,
        ),
        vision: capability(
            CapabilitySubject::Feature(CapabilityFeature::Vision),
            CapabilityDecision::Reject,
        ),
        reasoning: capability(
            CapabilitySubject::Feature(CapabilityFeature::Reasoning),
            CapabilityDecision::Reject,
        ),
    }
}

fn health_target() -> HealthProjectionTarget {
    HealthProjectionTarget::Endpoint(EndpointRef::new(
        StationId::new("station-1").expect("station"),
        EndpointId::new("primary").expect("endpoint"),
        EndpointRevision::new(7).expect("revision"),
    ))
}

fn effective_health(admission: HealthAdmission, reason: &'static str) -> EffectiveHealthProjection {
    EffectiveHealthProjection {
        target: health_target(),
        admission,
        reasons: vec![reason],
        runtime_overlay_applied: false,
        stale_runtime_overlay_ignored: false,
    }
}

fn operational_projections(pricing_basis: RoutingCostBasis) -> CandidateOperationalProjections {
    CandidateOperationalProjections {
        identity: CandidateIdentityProjection {
            station_key_id: "key-1".to_string(),
            station_id: "station-1".to_string(),
            endpoint_revision: 7,
            sanitized_origin: "https://relay.example".to_string(),
            credential_available: true,
        },
        priority: 10,
        resolved_model: Some("gpt-5-mini".to_string()),
        group: Some(GroupProjection {
            identity: GroupIdentity::BindingId("group-a".to_string()),
            display_name: "Primary".to_string(),
            available: true,
            trace: trace("group_resolved"),
        }),
        multiplier: MultiplierProjection {
            multiplier: Some(RateMultiplier::new(1.25).expect("multiplier")),
            status: MultiplierResolutionStatus::Resolved,
            selected_kind: Some(MultiplierEvidenceKind::BindingLatestUser),
            trace: trace("multiplier_resolved"),
        },
        pricing: RequestCostComparisonContext {
            route_kind: pricing_projector::PricingRouteKind::Inference,
            basis: pricing_basis,
            comparison_value: Some(1.25),
            reason: None,
            currency: Some("USD".to_string()),
            unit: Some("per_1m_tokens".to_string()),
            source_chain: vec!["pricing_rule:rule-1".to_string()],
            observed_at: Some("1000".to_string()),
            confidence: Some(0.9),
        },
        balance: BalanceProjection {
            status: BalanceProjectionStatus::Healthy,
            selected_scope: Some(BalanceScope::StationKey),
            health_hint: models::operational::HealthState::Unknown,
            trace: trace("balance_healthy"),
        },
        capabilities: capabilities(),
        health: HealthProjectionSet {
            station_key: effective_health(HealthAdmission::Admit, "key_ok"),
            station_account: effective_health(HealthAdmission::Admit, "account_ok"),
            endpoint: effective_health(HealthAdmission::Admit, "endpoint_ok"),
            model: effective_health(HealthAdmission::Admit, "model_ok"),
        },
        capacity: CapacityProjection {
            scopes: vec![
                CapacityScopeSnapshot {
                    scope: CapacityScope::StationKey,
                    limit: Some(8),
                    in_flight: 1,
                    available: true,
                    source_revision: Some(3),
                },
                CapacityScopeSnapshot {
                    scope: CapacityScope::Endpoint,
                    limit: Some(16),
                    in_flight: 2,
                    available: true,
                    source_revision: Some(7),
                },
            ],
        },
        backup_only: false,
        candidate_tags: vec!["fast".to_string(), "cheap".to_string()],
        snapshot_id: "ofs-test".to_string(),
        fact_version_vector: "station=1,key=2,settings=3,alias=4".to_string(),
    }
}

#[test]
fn classifier_freezes_local_policy_and_ignores_untrusted_request_policy_hints() {
    let facts = request_facts(RouteKind::Inference);

    assert_eq!(facts.route_kind(), RouteKind::Inference);
    assert_eq!(facts.requested_model(), Some("gpt-5-mini"));
    assert_eq!(facts.ordering_profile(), OrderingProfile::PriorityFirst);
    assert_eq!(facts.required_group_stable_key(), Some("binding:group-a"));
    assert_eq!(facts.max_rate_multiplier(), Some(2.0));
    assert_eq!(facts.admitted_at_ms(), 10);
}

#[test]
fn route_progress_owns_attempt_exclusions_ordinal_and_monotonic_deadline() {
    let mut progress = RouteProgress::new(10_000);

    progress.record_actual_attempt("key-1");
    progress.record_snapshot_rebuild();
    progress.record_runtime_rebuild();
    assert!(progress.tighten_deadline(9_000));
    assert!(!progress.tighten_deadline(12_000));

    let view = progress.view();
    assert_eq!(view.ordinal, 1);
    assert_eq!(view.attempt_count, 1);
    assert!(view.excludes_station_key("key-1"));
    assert_eq!(view.deadline_ms, 9_000);
    assert_eq!(view.snapshot_rebuild_count, 1);
    assert_eq!(view.runtime_rebuild_count, 1);
}

#[test]
fn candidate_projection_contains_complete_cross_module_facts_without_secrets() {
    let projection = project_route_candidate(
        &request_facts(RouteKind::Inference),
        operational_projections(RoutingCostBasis::ExactPrice),
    );

    assert_eq!(projection.identity.station_key_id, "key-1");
    assert_eq!(projection.priority, 10);
    assert_eq!(
        projection.identity.sanitized_origin,
        "https://relay.example"
    );
    assert_eq!(projection.route_kind, RouteKind::Inference);
    assert_eq!(projection.resolved_model.as_deref(), Some("gpt-5-mini"));
    assert_eq!(projection.policy.group_matches, true);
    assert_eq!(projection.policy.preferred_model_match, true);
    assert_eq!(projection.policy.tag_filter_match, true);
    assert_eq!(
        projection.group.as_ref().expect("group").stable_key,
        "binding:group-a"
    );
    assert_eq!(projection.multiplier.multiplier, Some(1.25));
    assert_eq!(projection.pricing.basis, RoutingCostBasis::ExactPrice);
    assert_eq!(projection.pricing.comparison_value, Some(1.25));
    assert_eq!(projection.pricing.source_chain, vec!["pricing_rule:rule-1"]);
    assert_eq!(projection.balance.status, BalanceProjectionStatus::Healthy);
    assert_eq!(projection.capability.tools, CapabilityDecision::Reject);
    assert!(projection.hard_rejection_codes.is_empty());
    assert!(!projection
        .hard_rejection_codes
        .contains(&"capability_rejected"));
    assert_eq!(projection.health.station_key, HealthAdmission::Admit);
    assert_eq!(projection.capacity.scopes.len(), 2);
    assert_eq!(projection.provenance.snapshot_id, "ofs-test");
    assert_eq!(
        projection.provenance.projector_version,
        ROUTE_CANDIDATE_PROJECTION_VERSION
    );
    assert_eq!(projection.provenance.endpoint_revision, 7);

    let debug = format!("{projection:?}");
    assert!(!debug.contains("sk-"));
    assert!(!debug.contains("?token="));
    assert!(!debug.contains("/v1/chat/completions"));
}

#[test]
fn inference_cannot_use_not_applicable_pricing_but_catalog_does_not_need_cost_or_multiplier_ceiling(
) {
    let inference_projection = project_route_candidate(
        &request_facts(RouteKind::Inference),
        operational_projections(RoutingCostBasis::NotApplicable),
    );
    assert!(inference_projection
        .hard_rejection_codes
        .contains(&"pricing_not_applicable_for_inference"));

    let mut catalog_settings = local_settings(OrderingProfile::CostFirst);
    catalog_settings.max_rate_multiplier = Some(1.0);
    let catalog_request = RouteRequestClassifier::classify(
        CanonicalRouteRequest {
            route_kind: RouteKind::ModelCatalog,
            requested_model: None,
            stream: false,
            uses_tools: false,
            uses_vision: false,
            uses_reasoning: false,
            untrusted_headers: Vec::new(),
        },
        catalog_settings,
        20,
    );
    let catalog_projection = project_route_candidate(
        &catalog_request,
        operational_projections(RoutingCostBasis::NotApplicable),
    );

    assert_eq!(catalog_projection.route_kind, RouteKind::ModelCatalog);
    assert!(!catalog_projection
        .hard_rejection_codes
        .contains(&"pricing_not_applicable_for_inference"));
    assert!(!catalog_projection
        .hard_rejection_codes
        .contains(&"multiplier_ceiling"));
}
