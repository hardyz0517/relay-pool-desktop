#![allow(dead_code)]

#[path = "../src/models/proxy.rs"]
mod proxy_model;
#[path = "../src/models/routing.rs"]
mod routing_model;

mod models {
    pub(crate) mod routing {
        pub(crate) use crate::routing_model::*;
    }

    pub(crate) mod proxy {
        pub(crate) use crate::proxy_model::*;
    }
}

mod application {
    pub(crate) mod routing_engine {
        pub(crate) mod request {
            #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
            #[serde(rename_all = "snake_case")]
            pub(crate) enum RouteKind {
                Inference,
                ModelCatalog,
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum GroupFilterMode {
                Any,
                Required,
            }
        }
    }

    pub(crate) mod operational_facts {
        pub(crate) mod balance_projector {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum BalanceProjectionStatus {
                Healthy,
                Missing,
                DepletedEmergency,
            }
        }

        pub(crate) mod capability_projector {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum CapabilityDecision {
                Allow,
                Reject,
                RequireStrictConfirmation,
            }
        }

        pub(crate) mod health_projector {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum HealthAdmission {
                Admit,
                AdmitDegraded,
                SuppressOrdinaryRuntime,
                SuppressDurableCooldown,
                HardReject,
                Unknown,
            }
        }

        pub(crate) mod multiplier_projector {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum MultiplierResolutionStatus {
                Resolved,
                Missing,
                Disabled,
                Stale,
                Ambiguous,
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
            use crate::application::{
                operational_facts::{
                    balance_projector::BalanceProjectionStatus,
                    capability_projector::CapabilityDecision, health_projector::HealthAdmission,
                    multiplier_projector::MultiplierResolutionStatus,
                    pricing_projector::RoutingCostBasis,
                },
                routing_engine::request::{GroupFilterMode, RouteKind},
            };

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct RouteCandidateProjection {
                pub(crate) identity: CandidateIdentityProjection,
                pub(crate) priority: i64,
                pub(crate) route_kind: RouteKind,
                pub(crate) requested_model: Option<String>,
                pub(crate) resolved_model: Option<String>,
                pub(crate) policy: CandidatePolicyProjection,
                pub(crate) group: Option<CandidateGroupProjection>,
                pub(crate) multiplier: CandidateMultiplierProjection,
                pub(crate) pricing: CandidatePricingProjection,
                pub(crate) balance: CandidateBalanceProjection,
                pub(crate) capability: CandidateCapabilityProjection,
                pub(crate) health: CandidateHealthProjection,
                pub(crate) capacity: CapacityProjection,
                pub(crate) provenance: CandidateProvenanceProjection,
                pub(crate) hard_rejection_codes: Vec<&'static str>,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CandidateIdentityProjection {
                pub(crate) station_key_id: String,
                pub(crate) station_id: String,
                pub(crate) endpoint_revision: i64,
                pub(crate) sanitized_origin: String,
                pub(crate) credential_available: bool,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CandidatePolicyProjection {
                pub(crate) group_filter_mode: GroupFilterMode,
                pub(crate) required_group_stable_key: Option<String>,
                pub(crate) group_matches: bool,
                pub(crate) backup_only: bool,
                pub(crate) preferred_model_match: bool,
                pub(crate) tag_filter_match: bool,
                pub(crate) allow_depleted_fallback: bool,
                pub(crate) affinity_eligible: bool,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CandidateGroupProjection {
                pub(crate) stable_key: String,
                pub(crate) display_name: String,
                pub(crate) available: bool,
                pub(crate) reason: &'static str,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CandidateMultiplierProjection {
                pub(crate) status: MultiplierResolutionStatus,
                pub(crate) multiplier: Option<f64>,
                pub(crate) selected_source: Option<&'static str>,
                pub(crate) ceiling_rejected: bool,
                pub(crate) reason: &'static str,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CandidatePricingProjection {
                pub(crate) basis: RoutingCostBasis,
                pub(crate) comparison_value: Option<f64>,
                pub(crate) reason: Option<&'static str>,
                pub(crate) currency: Option<String>,
                pub(crate) unit: Option<String>,
                pub(crate) source_chain: Vec<String>,
                pub(crate) observed_at: Option<String>,
                pub(crate) confidence: Option<f64>,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CandidateBalanceProjection {
                pub(crate) status: BalanceProjectionStatus,
                pub(crate) selected_scope: Option<String>,
                pub(crate) reason: &'static str,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CandidateCapabilityProjection {
                pub(crate) protocol: CapabilityDecision,
                pub(crate) model: CapabilityDecision,
                pub(crate) stream: CapabilityDecision,
                pub(crate) tools: CapabilityDecision,
                pub(crate) vision: CapabilityDecision,
                pub(crate) reasoning: CapabilityDecision,
                pub(crate) rejection_subjects: Vec<String>,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CandidateHealthProjection {
                pub(crate) station_key: HealthAdmission,
                pub(crate) station_account: HealthAdmission,
                pub(crate) endpoint: HealthAdmission,
                pub(crate) model: HealthAdmission,
                pub(crate) runtime_overlay_applied: bool,
                pub(crate) reasons: Vec<&'static str>,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CapacityProjection {
                pub(crate) scopes: Vec<CapacityScopeSnapshot>,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CapacityScopeSnapshot {
                pub(crate) scope: CapacityScope,
                pub(crate) limit: Option<u32>,
                pub(crate) in_flight: u32,
                pub(crate) available: bool,
                pub(crate) source_revision: Option<i64>,
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum CapacityScope {
                StationKey,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CandidateProvenanceProjection {
                pub(crate) snapshot_id: String,
                pub(crate) fact_version_vector: String,
                pub(crate) projector_version: &'static str,
                pub(crate) endpoint_revision: i64,
            }
        }
    }
}

#[path = "../src/application/queries/request_decision_trace.rs"]
mod request_decision_trace;
#[path = "../src/application/queries/routing_runtime.rs"]
mod routing_runtime;
#[path = "../src/application/queries/routing_workspace.rs"]
mod routing_workspace;

use application::operational_facts::candidate_projector::{
    CandidateBalanceProjection, CandidateCapabilityProjection, CandidateGroupProjection,
    CandidateHealthProjection, CandidateIdentityProjection, CandidateMultiplierProjection,
    CandidatePolicyProjection, CandidatePricingProjection, CandidateProvenanceProjection,
    CapacityProjection, CapacityScope, CapacityScopeSnapshot, RouteCandidateProjection,
};
use application::operational_facts::{
    balance_projector::BalanceProjectionStatus, capability_projector::CapabilityDecision,
    health_projector::HealthAdmission, multiplier_projector::MultiplierResolutionStatus,
    pricing_projector::RoutingCostBasis,
};
use application::routing_engine::request::{GroupFilterMode, RouteKind};
use models::{
    proxy::RequestLog,
    routing::{
        RouteEndpointKind, RoutingGroupFilter, RoutingPolicy, RuntimeRoutingCandidate,
        RuntimeRoutingSettings, StationKeyCapabilities, StationKeyHealth,
    },
};
use request_decision_trace::{
    decision_trace_from_legacy_log, RequestDecisionTimelineKind, RequestDecisionTimelineStatus,
    RequestDecisionTraceStatus,
};
use routing_runtime::runtime_overlay_from_candidates;
use routing_workspace::{
    simulate_preview_from_candidate_projections, workspace_snapshot_from_projection_candidates,
    workspace_snapshot_from_runtime, RoutePreviewSimulationInput, RoutingCapacityReadMode,
    RoutingWorkspaceProjectionCandidate, RoutingWorkspaceSnapshotInput,
    ROUTING_PREVIEW_POLICY_VERSION,
};

fn capabilities() -> StationKeyCapabilities {
    StationKeyCapabilities {
        station_key_id: "key-1".to_string(),
        supports_chat_completions: true,
        supports_responses: true,
        supports_embeddings: false,
        supports_stream: true,
        supports_tools: false,
        supports_vision: false,
        supports_reasoning: false,
        model_allowlist: vec!["gpt-5-mini".to_string()],
        model_blocklist: Vec::new(),
        preferred_models: vec!["gpt-5-mini".to_string()],
        only_use_as_backup: false,
        routing_tags: vec!["fast".to_string()],
        updated_at: "1000".to_string(),
    }
}

fn candidate(id: &str, load: Option<i64>) -> RuntimeRoutingCandidate {
    RuntimeRoutingCandidate {
        station_key_id: id.to_string(),
        station_id: "station-1".to_string(),
        station_endpoint_revision: 7,
        upstream_base_url: "https://secret.example/v1?token=redacted".to_string(),
        upstream_api_format: models::proxy::UpstreamApiFormat::OpenAiChatCompletions,
        routing_order: Some(1),
        priority: 10,
        max_concurrency: 8,
        load_factor: load,
        schedulable: true,
        collector_proxy_mode: "default".to_string(),
        collector_proxy_url: None,
        station_name: "Station".to_string(),
        key_name: "Key".to_string(),
        capabilities: capabilities(),
        health: Some(StationKeyHealth {
            station_key_id: id.to_string(),
            last_success_at: Some("1000".to_string()),
            last_failure_at: None,
            consecutive_failures: 0,
            success_count: 1,
            failure_count: 0,
            avg_latency_ms: Some(120),
            last_error_summary: None,
            cooldown_until: None,
            updated_at: "1000".to_string(),
        }),
        balance_snapshot: None,
        api_key: Some("sk-secret".to_string()),
        api_key_secret: None,
    }
}

fn legacy_request_log() -> RequestLog {
    RequestLog {
        id: "request-log-1".to_string(),
        request_id: Some("request-1".to_string()),
        started_at: "1700000000000".to_string(),
        finished_at: Some("1700000000100".to_string()),
        duration_ms: Some(100),
        method: "POST".to_string(),
        path: "/v1/chat/completions".to_string(),
        model: Some("gpt-5-mini".to_string()),
        stream: true,
        status: "success".to_string(),
        lifecycle_status: Some("completed".to_string()),
        station_key_id: Some("key-1".to_string()),
        station_id: Some("station-1".to_string()),
        upstream_base_url: None,
        fallback_count: 1,
        error_message: None,
        route_policy: Some("cost_stable_first".to_string()),
        route_reason: Some("selected".to_string()),
        rejected_candidates_json: None,
        body_bytes: Some(512),
        attempt_count: Some(2),
        route_wait_ms: Some(7),
        upstream_headers_ms: Some(20),
        failure_source: Some("upstream".to_string()),
        attempts_json: None,
        completion_source: Some("stream_eof".to_string()),
        prompt_tokens: Some(10),
        completion_tokens: Some(20),
        total_tokens: Some(30),
        cache_creation_tokens: None,
        cache_read_tokens: None,
        reasoning_effort: None,
        first_token_ms: Some(40),
        billing_mode: Some("estimated".to_string()),
        estimated_input_cost: Some(0.001),
        estimated_output_cost: Some(0.002),
        estimated_total_cost: Some(0.003),
        base_input_cost: Some(0.001),
        base_output_cost: Some(0.002),
        base_fixed_cost: None,
        base_total_cost: Some(0.003),
        cost_currency: Some("USD".to_string()),
        pricing_rule_id: Some("rule-1".to_string()),
        pricing_source: Some("fixture".to_string()),
        cost_status: Some("estimated".to_string()),
        group_binding_id: Some("group-1".to_string()),
        normalization_status: Some("exact".to_string()),
        balance_scope: Some("key".to_string()),
        economic_context_json: None,
        created_at: "1700000000000".to_string(),
    }
}

fn settings() -> RuntimeRoutingSettings {
    RuntimeRoutingSettings {
        policy: RoutingPolicy::CostStableFirst,
        max_rate_multiplier: Some(2.0),
        routing_group_filter: RoutingGroupFilter::AllGroups,
        scheduler_advanced_settings: Default::default(),
        allow_depleted_fallback: false,
    }
}

fn projection(
    id: &str,
    station_id: &str,
    hard_rejection_codes: Vec<&'static str>,
) -> RouteCandidateProjection {
    RouteCandidateProjection {
        identity: CandidateIdentityProjection {
            station_key_id: id.to_string(),
            station_id: station_id.to_string(),
            endpoint_revision: 1,
            sanitized_origin: "https://redacted.example".to_string(),
            credential_available: true,
        },
        priority: 1,
        route_kind: RouteKind::Inference,
        requested_model: Some("gpt-5-mini".to_string()),
        resolved_model: Some("gpt-5-mini".to_string()),
        policy: CandidatePolicyProjection {
            group_filter_mode: GroupFilterMode::Any,
            required_group_stable_key: None,
            group_matches: true,
            backup_only: false,
            preferred_model_match: false,
            tag_filter_match: true,
            allow_depleted_fallback: false,
            affinity_eligible: true,
        },
        group: Some(CandidateGroupProjection {
            stable_key: "binding:group-1".to_string(),
            display_name: "Group 1".to_string(),
            available: true,
            reason: "bound_group",
        }),
        multiplier: CandidateMultiplierProjection {
            status: MultiplierResolutionStatus::Missing,
            multiplier: None,
            selected_source: None,
            ceiling_rejected: false,
            reason: "multiplier_missing",
        },
        pricing: CandidatePricingProjection {
            basis: RoutingCostBasis::Unpriced,
            comparison_value: None,
            reason: Some("pricing_context_missing"),
            currency: None,
            unit: None,
            source_chain: vec!["pricing_projector".to_string()],
            observed_at: None,
            confidence: None,
        },
        balance: CandidateBalanceProjection {
            status: BalanceProjectionStatus::Missing,
            selected_scope: None,
            reason: "balance_missing",
        },
        capability: CandidateCapabilityProjection {
            protocol: CapabilityDecision::Allow,
            model: CapabilityDecision::Allow,
            stream: CapabilityDecision::Allow,
            tools: CapabilityDecision::Reject,
            vision: CapabilityDecision::Reject,
            reasoning: CapabilityDecision::Reject,
            rejection_subjects: Vec::new(),
        },
        health: CandidateHealthProjection {
            station_key: HealthAdmission::Admit,
            station_account: HealthAdmission::Admit,
            endpoint: HealthAdmission::Admit,
            model: HealthAdmission::Admit,
            runtime_overlay_applied: false,
            reasons: Vec::new(),
        },
        capacity: CapacityProjection {
            scopes: vec![CapacityScopeSnapshot {
                scope: CapacityScope::StationKey,
                limit: Some(8),
                in_flight: 1,
                available: true,
                source_revision: Some(1),
            }],
        },
        provenance: CandidateProvenanceProjection {
            snapshot_id: "snapshot-1".to_string(),
            fact_version_vector: "endpoint:1".to_string(),
            projector_version: "route_candidate_projection_v1",
            endpoint_revision: 1,
        },
        hard_rejection_codes,
    }
}

#[test]
fn workspace_snapshot_is_backend_owned_paginated_and_secret_free() {
    let snapshot = workspace_snapshot_from_runtime(
        &settings(),
        vec![candidate("key-1", Some(1)), candidate("key-2", Some(0))],
        RoutingWorkspaceSnapshotInput {
            limit: Some(1),
            cursor: None,
        },
        1234,
    );

    assert_eq!(
        snapshot.preview_policy_version,
        ROUTING_PREVIEW_POLICY_VERSION
    );
    assert_eq!(snapshot.production_policy, RoutingPolicy::CostStableFirst);
    assert_eq!(
        snapshot.capacity_mode,
        RoutingCapacityReadMode::SnapshotOnly
    );
    assert_eq!(snapshot.page.returned, 1);
    assert_eq!(snapshot.page.next_cursor.as_deref(), Some("offset:1"));
    assert_eq!(snapshot.candidates[0].capacity.acquired, false);
    assert_eq!(
        snapshot.candidates[0].capacity.mode,
        RoutingCapacityReadMode::SnapshotOnly
    );

    let json = serde_json::to_string(&snapshot).expect("serialize snapshot");
    assert!(!json.contains("sk-secret"));
    assert!(!json.contains("secret.example"));
    assert!(!json.contains("upstreamBaseUrl"));
}

#[test]
fn runtime_overlay_is_separate_low_cardinality_and_does_not_refresh_workspace_facts() {
    let overlay = runtime_overlay_from_candidates(vec![candidate("key-1", Some(3))], 2000, 42, 32);

    assert_eq!(overlay.overlay_version, "routing_runtime_overlay_v1");
    assert_eq!(overlay.revision, 42);
    assert_eq!(overlay.candidates.len(), 1);
    assert_eq!(overlay.candidates[0].in_flight, Some(3));

    let json = serde_json::to_string(&overlay).expect("serialize overlay");
    assert!(!json.contains("price"));
    assert!(!json.contains("history"));
    assert!(!json.contains("secret.example"));
}

#[test]
fn workspace_snapshot_from_projection_exposes_unified_operational_fields() {
    let snapshot = workspace_snapshot_from_projection_candidates(
        &settings(),
        vec![RoutingWorkspaceProjectionCandidate {
            station_name: "Station".to_string(),
            key_name: "Key".to_string(),
            projection: projection("key-1", "station-1", Vec::new()),
        }],
        RoutingWorkspaceSnapshotInput {
            limit: Some(10),
            cursor: None,
        },
        1234,
    );

    let candidate = &snapshot.candidates[0];
    assert_eq!(
        candidate
            .group
            .as_ref()
            .map(|group| group.stable_key.as_str()),
        Some("binding:group-1")
    );
    assert_eq!(candidate.multiplier.reason, "multiplier_missing");
    assert_eq!(
        candidate.pricing.reason.as_deref(),
        Some("pricing_context_missing")
    );
    assert_eq!(candidate.capability_verdicts.tools, "reject");
    assert_eq!(
        candidate.source_refs.projector_version,
        "route_candidate_projection_v1"
    );
    assert!(candidate.hard_rejection_codes.is_empty());
}

#[test]
fn preview_simulation_uses_projection_input_and_snapshot_only_capacity() {
    let projections = vec![
        projection("key-1", "station-1", vec!["health_hard_reject"]),
        projection("key-2", "station-2", Vec::new()),
    ];

    let simulation = simulate_preview_from_candidate_projections(
        RoutePreviewSimulationInput {
            endpoint: RouteEndpointKind::ChatCompletions,
            model: Some("gpt-5-mini".to_string()),
            stream: true,
        },
        RoutingPolicy::PriorityFallback,
        &projections,
    );

    assert_eq!(
        simulation.preview_policy_version,
        ROUTING_PREVIEW_POLICY_VERSION
    );
    assert_eq!(
        simulation.capacity_mode,
        RoutingCapacityReadMode::SnapshotOnly
    );
    assert_eq!(simulation.selected_station_key_id.as_deref(), Some("key-2"));
    assert_eq!(simulation.rejection_count, 1);
    assert!(!simulation.selected_capacity_acquired);
}

#[test]
fn request_decision_trace_does_not_fake_planning_rounds_before_cutover() {
    let trace = decision_trace_from_legacy_log(None);

    assert_eq!(trace.status, RequestDecisionTraceStatus::TraceUnavailable);
    assert_eq!(trace.reason, "trace_unavailable");
    assert_eq!(trace.timeline.len(), 1);
    assert_eq!(
        trace.timeline[0].kind,
        RequestDecisionTimelineKind::Unavailable
    );
    assert_eq!(
        trace.timeline[0].status,
        RequestDecisionTimelineStatus::Unavailable
    );
    assert!(trace.planning_rounds.is_empty());
}

#[test]
fn legacy_request_decision_trace_exposes_typed_timeline_without_json_ui_contract() {
    let trace = decision_trace_from_legacy_log(Some(legacy_request_log()));

    assert_eq!(trace.status, RequestDecisionTraceStatus::LegacySummary);
    assert!(trace.planning_rounds.is_empty());
    assert_eq!(trace.timeline.len(), 7);
    assert_eq!(
        trace.timeline[0].kind,
        RequestDecisionTimelineKind::LegacySummary
    );
    assert_eq!(
        trace.timeline[1].kind,
        RequestDecisionTimelineKind::PlanningRound
    );
    assert_eq!(
        trace.timeline[1].status,
        RequestDecisionTimelineStatus::Unavailable
    );
    assert_eq!(
        trace.timeline[2].kind,
        RequestDecisionTimelineKind::SlotWait
    );
    assert_eq!(
        trace.timeline[2].status,
        RequestDecisionTimelineStatus::Available
    );
    assert_eq!(trace.timeline[2].duration_ms, Some(7));
    assert_eq!(
        trace.timeline[3].kind,
        RequestDecisionTimelineKind::AttemptProtocol
    );
    assert_eq!(trace.timeline[3].attempt_count, Some(2));
    assert_eq!(
        trace.timeline[4].kind,
        RequestDecisionTimelineKind::Fallback
    );
    assert_eq!(trace.timeline[4].fallback_count, Some(1));
    assert_eq!(
        trace.timeline[5].kind,
        RequestDecisionTimelineKind::DownstreamDelivery
    );
    assert_eq!(
        trace.timeline[6].kind,
        RequestDecisionTimelineKind::CostAggregate
    );
    assert_eq!(trace.timeline[6].cost_status.as_deref(), Some("estimated"));
    assert_eq!(trace.timeline[6].cost_currency.as_deref(), Some("USD"));
}
