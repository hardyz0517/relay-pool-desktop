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
        }
    }

    pub(crate) mod operational_facts {
        pub(crate) mod candidate_projector {
            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct RouteCandidateProjection {
                pub(crate) identity: CandidateIdentityProjection,
                pub(crate) hard_rejection_codes: Vec<&'static str>,
            }

            #[derive(Debug, Clone, PartialEq)]
            pub(crate) struct CandidateIdentityProjection {
                pub(crate) station_key_id: String,
                pub(crate) station_id: String,
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
    CandidateIdentityProjection, RouteCandidateProjection,
};
use models::routing::{
    RouteEndpointKind, RoutingGroupFilter, RoutingPolicy, RuntimeRoutingCandidate,
    RuntimeRoutingSettings, StationKeyCapabilities, StationKeyHealth,
};
use request_decision_trace::{decision_trace_from_legacy_log, RequestDecisionTraceStatus};
use routing_runtime::runtime_overlay_from_candidates;
use routing_workspace::{
    simulate_preview_from_candidate_projections, workspace_snapshot_from_runtime,
    RoutePreviewSimulationInput, RoutingCapacityReadMode, RoutingWorkspaceSnapshotInput,
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

fn settings() -> RuntimeRoutingSettings {
    RuntimeRoutingSettings {
        policy: RoutingPolicy::CostStableFirst,
        max_rate_multiplier: Some(2.0),
        routing_group_filter: RoutingGroupFilter::AllGroups,
        scheduler_advanced_settings: Default::default(),
        allow_depleted_fallback: false,
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
fn preview_simulation_uses_projection_input_and_snapshot_only_capacity() {
    let projections = vec![
        RouteCandidateProjection {
            identity: CandidateIdentityProjection {
                station_key_id: "key-1".to_string(),
                station_id: "station-1".to_string(),
                endpoint_revision: 1,
            },
            hard_rejection_codes: vec!["health_hard_reject"],
        },
        RouteCandidateProjection {
            identity: CandidateIdentityProjection {
                station_key_id: "key-2".to_string(),
                station_id: "station-2".to_string(),
                endpoint_revision: 1,
            },
            hard_rejection_codes: Vec::new(),
        },
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
    assert!(trace.planning_rounds.is_empty());
}
