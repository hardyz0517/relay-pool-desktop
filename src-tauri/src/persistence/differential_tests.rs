use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use crate::{
    application::{
        clock::Clock,
        credentials::{
            CredentialError, CredentialService, CredentialVault, EncryptedSecret, SecretBytes,
        },
        ids::IdGenerator,
        operational_facts::{
            candidate_projection::{
                route_projection_from_runtime_candidate_with_pricing, validated_route_settings,
            },
            pricing_projector::RoutingCostBasis,
        },
        request_finalization::RequestFinalizationService,
        request_finalization::{
            effect_planner::classified_attempt_failure_from_canonical,
            failure::{
                failure_from_provider_signal, CapabilityApplicabilitySet,
                ProviderErrorSemanticSignal,
            },
        },
        request_lifecycle::{
            attempt::{AttemptContext, AttemptTerminal, AttemptTerminalRecord},
            delivery::DeliveryTerminal,
            ports::RequestLifecycleStore,
            request::{
                FinalRequestRecord, RequestCompletion, RequestContextSnapshot, RequestLifecycle,
                RequestStartRecord, RequestTerminal,
            },
        },
        routing::RoutingService,
        routing_diagnostics_reader::RoutingDiagnosticsReader,
        routing_engine::{
            planning_snapshot::RuntimeOverlaySnapshot,
            request::{CanonicalRouteRequest, RouteKind, RouteRequestClassifier},
        },
        settings::SettingsService,
        stations::StationService,
    },
    models::{
        remote_keys::{RemoteKeyMatchStatus, RemoteStationKey},
        routing::{RoutingGroupFilter, RuntimeRoutingSettings},
        settings::UpdateSettingsInput,
        station_keys::{CreateStationKeyInput, UpdateStationKeyInput},
        stations::{CreateStationInput, UpdateStationInput},
    },
    persistence::{self, runtime::PersistenceRuntime, schema_compatibility::BinaryCompatibility},
};
use chrono::{TimeZone, Utc};
use semver::Version;
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, Connection, Row};
static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(1);

#[tokio::test]
async fn request_finalization_is_idempotent_in_v2() {
    let fixture = V2Fixture::create().await;
    let runtime = Arc::new(fixture.runtime().await);
    let service = RequestFinalizationService::new(runtime.handle());
    let context = RequestContextSnapshot {
        request_id: "request-finalization-1".to_string(),
        method: "POST".to_string(),
        local_path: "/v1/chat/completions".to_string(),
        endpoint: "chat_completions".to_string(),
        received_at_ms: 1000,
    };
    service
        .start_request(RequestStartRecord {
            context: context.clone(),
        })
        .await
        .expect("request start");

    let mut lifecycle = RequestLifecycle::new(context);
    lifecycle.admit().expect("admit");
    let final_record: FinalRequestRecord = lifecycle
        .terminalize(
            RequestTerminal::Completed(RequestCompletion {
                protocol_completed: true,
                attempt_id: None,
            }),
            DeliveryTerminal::BodyCompleted,
        )
        .expect("terminal");
    let first = service
        .finish_request(final_record.clone())
        .await
        .expect("first finalization");
    let duplicate = service
        .finish_request(final_record)
        .await
        .expect("duplicate finalization");

    assert!(first.finalized);
    assert!(!duplicate.finalized);
    assert_eq!(fixture.count("request_logs").await, 1);
}

#[tokio::test]
async fn proxy_group_subscription_failure_does_not_create_a_routing_exclusion() {
    let fixture = V2Fixture::create().await;
    fixture
        .seed_planning_candidate("key-group-a", "station-a", Some("group-a"), "gpt-upstream")
        .await;
    fixture
        .seed_planning_candidate("key-group-b", "station-b", Some("group-b"), "gpt-upstream")
        .await;
    let routing = RoutingService::new(fixture.runtime().await.handle());
    let request = planning_request("gpt-test");

    assert_eq!(
        planning_ids(&routing, &request).await,
        ["key-group-a", "key-group-b"]
    );

    let finalizer = RequestFinalizationService::new(fixture.runtime().await.handle());
    persist_proxy_failure(
        &finalizer,
        AttemptContext {
            attempt_id: crate::application::request_lifecycle::request::AttemptId::new(
                "req-group",
                0,
            ),
            station_id: "station-a".to_string(),
            station_key_id: "key-group-a".to_string(),
            endpoint_revision: 1,
            credential_revision: 1,
            account_revision: 1,
            group_binding_id: Some("group-a".to_string()),
            group_revision: Some(1),
            resolved_upstream_model: Some("gpt-upstream".to_string()),
            comparability_key: None,
            model_alias_revision: 1,
            started_at_ms: 1,
        },
        failure_from_provider_signal(
            ProviderErrorSemanticSignal::ConfirmedGroupSubscriptionInvalid {
                station_id: "station-a".to_string(),
                group_binding_id: "group-a".to_string(),
            },
            CapabilityApplicabilitySet::ConfirmedModelCatalog,
        ),
    )
    .await;

    // Scoped group/account/endpoint health is retained for diagnostics, but
    // v3 production routing is keyed only by station_key_id. A group-level
    // subscription failure therefore must not remove sibling candidates.
    assert_eq!(
        planning_ids(&routing, &request).await,
        ["key-group-a", "key-group-b"]
    );
    fixture.bump_group_revision("group-a").await;
    assert_eq!(
        planning_ids(&routing, &request).await,
        ["key-group-a", "key-group-b"]
    );
}

#[tokio::test]
async fn proxy_model_not_found_excludes_only_that_key_model_commitment_until_revision_changes() {
    let fixture = V2Fixture::create().await;
    fixture
        .seed_planning_candidate("key-model-a", "station-a", None, "gpt-upstream")
        .await;
    fixture
        .seed_planning_candidate("key-model-b", "station-b", None, "gpt-upstream")
        .await;
    let routing = RoutingService::new(fixture.runtime().await.handle());
    // Legacy model aliases are no longer consumed by the production planner;
    // exercise the native upstream model identity directly.
    let request = planning_request("gpt-upstream");

    assert_eq!(
        planning_ids(&routing, &request).await,
        ["key-model-a", "key-model-b"]
    );

    let finalizer = RequestFinalizationService::new(fixture.runtime().await.handle());
    persist_proxy_failure(
        &finalizer,
        AttemptContext {
            attempt_id: crate::application::request_lifecycle::request::AttemptId::new(
                "req-model",
                0,
            ),
            station_id: "station-a".to_string(),
            station_key_id: "key-model-a".to_string(),
            endpoint_revision: 1,
            credential_revision: 1,
            account_revision: 1,
            group_binding_id: None,
            group_revision: None,
            resolved_upstream_model: Some("gpt-upstream".to_string()),
            comparability_key: None,
            model_alias_revision: 1,
            started_at_ms: 1,
        },
        failure_from_provider_signal(
            ProviderErrorSemanticSignal::ConfirmedModelNotFound {
                station_key_id: "key-model-a".to_string(),
                model: "gpt-upstream".to_string(),
            },
            CapabilityApplicabilitySet::ConfirmedModelCatalog,
        ),
    )
    .await;

    assert_eq!(planning_ids(&routing, &request).await, ["key-model-b"]);
    fixture.bump_key_revision("key-model-a").await;
    assert_eq!(
        planning_ids(&routing, &request).await,
        ["key-model-a", "key-model-b"]
    );
}

async fn persist_proxy_failure(
    finalizer: &RequestFinalizationService,
    context: AttemptContext,
    failure: crate::application::request_finalization::failure::CanonicalFailure,
) {
    finalizer
        .start_request(RequestStartRecord {
            context: RequestContextSnapshot {
                request_id: context.attempt_id.request_id.clone(),
                method: "POST".to_string(),
                local_path: "/v1/chat/completions".to_string(),
                endpoint: "chat_completions".to_string(),
                received_at_ms: context.started_at_ms,
            },
        })
        .await
        .expect("proxy request starts before its attempt terminal");
    finalizer
        .finish_attempt(AttemptTerminalRecord {
            context,
            terminal: AttemptTerminal::Failed(classified_attempt_failure_from_canonical(&failure)),
            output_committed: false,
            terminal_at_ms: 10,
        })
        .await
        .expect("proxy terminal persists");
}

fn planning_request(model: &str) -> crate::application::routing_engine::request::RouteRequestFacts {
    RouteRequestClassifier::classify(
        CanonicalRouteRequest {
            route_kind: RouteKind::Inference,
            requested_model: Some(model.to_string()),
            stream: false,
            uses_tools: false,
            uses_vision: false,
            uses_reasoning: false,
            untrusted_headers: Vec::new(),
        },
        validated_route_settings(&RuntimeRoutingSettings::default()),
        100,
    )
}

async fn planning_ids(
    routing: &RoutingService,
    request: &crate::application::routing_engine::request::RouteRequestFacts,
) -> Vec<&'static str> {
    let snapshot = routing
        .load_intelligent_planning_snapshot(
            request,
            RuntimeOverlaySnapshot {
                runtime_instance_id: "e2e-runtime".to_string(),
                runtime_revision: 1,
                candidate_set_revision: 1,
                in_flight: 0,
                max_concurrency: 1,
                affinity_station_key_id: None,
            },
            crate::application::routing_engine::request::PlanningRequestContext::from_now(
                std::time::Duration::from_secs(5),
            ),
        )
        .await;
    let snapshot = match snapshot {
        Ok(Some(snapshot)) => snapshot,
        Err(error) => {
            let debug = routing
                .load_workspace_candidates_with_request_pricing(request)
                .await;
            panic!("planning snapshot: {error:?}; workspace: {debug:?}")
        }
        Ok(None) => panic!("routing policy unavailable"),
    };
    snapshot
        .candidates
        .iter()
        .map(|candidate| match candidate.station_key_id.as_str() {
            "key-group-a" => "key-group-a",
            "key-group-b" => "key-group-b",
            "key-model-a" => "key-model-a",
            "key-model-b" => "key-model-b",
            other => panic!("unexpected candidate {other}"),
        })
        .collect()
}

#[tokio::test]
async fn station_endpoint_change_is_atomic_and_matches_v1_contract_boundary() {
    let fixture = V2Fixture::create().await;
    let station_service = fixture.station_service(vec!["station-1"]).await;
    let station = station_service
        .create(CreateStationInput {
            name: "Relay".to_string(),
            station_type: "newapi".to_string(),
            website_url: "https://console.example".to_string(),
            api_base_url: "https://api.example/v1".to_string(),
            api_key: "sk-test-station".to_string(),
            collector_proxy_mode: "inherit".to_string(),
            collector_proxy_url: None,
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: Some(10.0),
            collection_interval_minutes: 5,
            note: None,
        })
        .await
        .expect("create station");
    fixture.seed_endpoint_state(&station.id).await;

    let updated = station_service
        .update_station(UpdateStationInput {
            id: station.id.clone(),
            name: "Relay Updated".to_string(),
            station_type: "newapi".to_string(),
            website_url: "https://console-next.example".to_string(),
            api_base_url: "https://api-next.example/v1".to_string(),
            api_key: None,
            collector_proxy_mode: "inherit".to_string(),
            collector_proxy_url: None,
            enabled: true,
            credit_per_cny: 2.0,
            low_balance_threshold_cny: None,
            collection_interval_minutes: 10,
            note: Some("moved".to_string()),
        })
        .await
        .expect("update station endpoint");

    assert_eq!(updated.endpoint_revision, 2);
    assert!(
        !updated.enabled,
        "API origin changes must disable until revalidated"
    );
    assert_eq!(updated.status, "disabled");
    assert_eq!(fixture.endpoint_health_rows().await, 0);
    assert_eq!(fixture.station_key_health_rows().await, 0);
    assert_eq!(fixture.credential_session_source(&station.id).await, "none");
    assert_eq!(fixture.secret_rows().await, 0);
}

#[tokio::test]
async fn station_reorder_and_delete_are_one_bounded_write_session() {
    let fixture = V2Fixture::create().await;
    let service = fixture
        .station_service(vec!["station-a", "station-b"])
        .await;
    let first = service
        .create(station_input("First"))
        .await
        .expect("first station");
    let second = service
        .create(station_input("Second"))
        .await
        .expect("second station");

    let reordered = service
        .reorder(vec![second.id.clone(), first.id.clone()])
        .await
        .expect("reorder");
    assert_eq!(reordered[0].id, second.id);
    assert_eq!(reordered[1].id, first.id);

    service
        .delete(second.id.clone())
        .await
        .expect("delete station");
    let remaining = service.list().await.expect("list stations");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].priority, 0);
}

#[tokio::test]
async fn settings_are_typed_and_unknown_values_do_not_enter_v2() {
    let fixture = V2Fixture::create().await;
    let service = fixture.settings_service().await;

    service
        .import_known_legacy_settings(vec![
            ("local_proxy_port".to_string(), "8787".to_string()),
            ("collector_proxy_mode".to_string(), "direct".to_string()),
            ("retired_secret_setting".to_string(), "canary".to_string()),
        ])
        .await
        .expect("import settings");

    let settings = service.load().await.expect("load settings");
    assert_eq!(settings.local_proxy_port, 8787);
    assert_eq!(settings.collector_proxy_mode, "direct");
    assert!(!fixture.any_setting_contains("canary").await);
}

#[tokio::test]
async fn settings_update_preserves_typed_defaults_and_validates_bounds() {
    let fixture = V2Fixture::create().await;
    let service = fixture.settings_service().await;

    let settings = service
        .update(UpdateSettingsInput {
            local_proxy_port: 8788,
            collector_proxy_mode: "direct".to_string(),
            collector_proxy_url: None,
            low_balance_threshold_cny: 8.0,
            collector_interval_minutes: 15,
            balance_interval_minutes: 5,
            group_rate_interval_minutes: 20,
            published_status_interval_minutes: 5,
            pricing_refresh_interval_minutes: 60,
            collector_timeout_seconds: 15,
            collector_max_concurrency: 2,
            developer_mode_enabled: true,
            show_decision_explanation: true,
            tray_behavior: Some("disabled".to_string()),
        })
        .await
        .expect("update settings");

    assert_eq!(settings.local_proxy_port, 8788);
    // Deprecated routing settings are ignored after the policy aggregate cutover.
    assert_eq!(settings.tray_behavior, "disabled");
    assert_eq!(settings.collector_max_concurrency, 2);

    let error = service
        .update(UpdateSettingsInput {
            collector_max_concurrency: 9,
            ..settings_input()
        })
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "constraint violation");
}

#[tokio::test]
async fn credential_secret_replacement_commits_ciphertext_and_reference_atomically() {
    let fixture = V2Fixture::create().await;
    let station = fixture
        .station_service(vec!["station-1"])
        .await
        .create(station_input("CredentialStation"))
        .await
        .expect("station");
    let vault = Arc::new(DeterministicCredentialVault::new([7; 32]));
    let service = fixture
        .credential_service(
            vault.clone(),
            vec!["key-1", "secret-create", "secret-replace"],
        )
        .await;
    let key = service
        .create_station_key(station_key_input(
            &station.id,
            "Primary",
            "sk-create-canary",
        ))
        .await
        .expect("create key");

    let saved = service
        .replace_station_key_secret(
            &station.id,
            &key.id,
            SecretBytes::from(b"sk-test-canary".to_vec()),
        )
        .await
        .expect("replace secret");

    assert_eq!(saved.secret_ref.owner_id, "key-1");
    assert_eq!(saved.secret_ref.kind, "api_key");
    assert_eq!(vault.last_aad(), "station_key:key-1:api_key:v1");
    assert_eq!(fixture.secret_rows().await, 1);
    assert!(!fixture.any_text_contains("sk-test-canary").await);
    assert!(!fixture.any_blob_contains(b"sk-test-canary").await);
    let listed = service
        .list_station_keys(station.id.clone())
        .await
        .expect("list keys");
    assert_eq!(listed[0].api_key_masked, "sk-***nary");
    assert!(listed[0].api_key_present);
}

#[tokio::test]
async fn credential_blank_secret_update_preserves_ciphertext_reference() {
    let fixture = V2Fixture::create().await;
    let station = fixture
        .station_service(vec!["station-1"])
        .await
        .create(station_input("BlankSecretStation"))
        .await
        .expect("station");
    let vault = Arc::new(DeterministicCredentialVault::new([3; 32]));
    let service = fixture
        .credential_service(vault, vec!["key-1", "secret-1"])
        .await;
    let key = service
        .create_station_key(station_key_input(
            &station.id,
            "Primary",
            "sk-original-canary",
        ))
        .await
        .expect("create key");
    let before = fixture.station_key_secret_id(&key.id).await;

    let updated = service
        .update_station_key(UpdateStationKeyInput {
            id: key.id.clone(),
            station_id: station.id.clone(),
            name: "Renamed".to_string(),
            api_key: Some(String::new()),
            enabled: true,
            priority: 0,
            max_concurrency: 4,
            load_factor: Some(2),
            schedulable: true,
            group_name: Some("Group A".to_string()),
            tier_label: None,
            group_binding_id: Some("binding-a".to_string()),
            group_id_hash: Some("hash-a".to_string()),
            rate_multiplier: Some(1.25),
            manual_rate_multiplier: None,
            rate_source: Some("manual".to_string()),
            balance_scope: Some("key".to_string()),
            status: "healthy".to_string(),
            note: Some("kept".to_string()),
        })
        .await
        .expect("blank secret update");
    let after = fixture.station_key_secret_id(&key.id).await;

    assert_eq!(before, after);
    assert_eq!(updated.name, "Renamed");
    assert_eq!(updated.max_concurrency, 4);
    assert_eq!(fixture.secret_rows().await, 1);
    assert!(!fixture.any_text_contains("sk-original-canary").await);
}

#[tokio::test]
async fn credential_blank_secret_replacement_is_station_scoped_and_returns_existing_secret_ref() {
    let fixture = V2Fixture::create().await;
    let first_station = fixture
        .station_service(vec!["station-1", "station-2"])
        .await
        .create(station_input("FirstCredentialStation"))
        .await
        .expect("first station");
    let second_station = fixture
        .station_service(vec!["unused"])
        .await
        .create(station_input("SecondCredentialStation"))
        .await
        .expect("second station");
    let service = fixture
        .credential_service(
            Arc::new(DeterministicCredentialVault::new([4; 32])),
            vec!["key-1", "secret-1"],
        )
        .await;
    let key = service
        .create_station_key(station_key_input(
            &first_station.id,
            "Primary",
            "sk-original-canary",
        ))
        .await
        .expect("create key");
    let existing_secret_id = fixture
        .station_key_secret_id(&key.id)
        .await
        .expect("secret id");

    let saved = service
        .replace_station_key_secret(&first_station.id, &key.id, SecretBytes::from(Vec::new()))
        .await
        .expect("blank replacement preserves existing secret");
    assert_eq!(saved.secret_ref.id, existing_secret_id);
    assert_eq!(saved.secret_ref.owner_id, key.id);

    let cross_station = service
        .replace_station_key_secret(&second_station.id, &key.id, SecretBytes::from(Vec::new()))
        .await
        .expect_err("cross-station key should be rejected");
    assert_eq!(cross_station.to_string(), "not found");
}

#[tokio::test]
async fn station_key_ordering_is_station_scoped_and_deterministic() {
    let fixture = V2Fixture::create().await;
    let station = fixture
        .station_service(vec!["station-1"])
        .await
        .create(station_input("OrderStation"))
        .await
        .expect("station");
    let service = fixture
        .credential_service(
            Arc::new(DeterministicCredentialVault::new([5; 32])),
            vec!["key-a", "secret-a", "key-b", "secret-b"],
        )
        .await;
    let first = service
        .create_station_key(station_key_input(&station.id, "First", "sk-first"))
        .await
        .expect("first key");
    let second = service
        .create_station_key(station_key_input(&station.id, "Second", "sk-second"))
        .await
        .expect("second key");

    let reordered = service
        .reorder_station_keys(
            station.id.clone(),
            vec![second.id.clone(), first.id.clone()],
        )
        .await
        .expect("reorder keys");

    assert_eq!(reordered[0].id, second.id);
    assert_eq!(reordered[0].priority, 0);
    assert_eq!(reordered[1].id, first.id);
    assert_eq!(reordered[1].priority, 1);
}

#[tokio::test]
async fn remote_key_binding_rejects_cross_station_keys() {
    let fixture = V2Fixture::create().await;
    let station_service = fixture
        .station_service(vec!["station-a", "station-b"])
        .await;
    let first_station = station_service
        .create(station_input("RemoteA"))
        .await
        .expect("first station");
    let second_station = station_service
        .create(station_input("RemoteB"))
        .await
        .expect("second station");
    let service = fixture
        .credential_service(
            Arc::new(DeterministicCredentialVault::new([9; 32])),
            vec!["key-a", "secret-a", "key-b", "secret-b"],
        )
        .await;
    let first_key = service
        .create_station_key(station_key_input(&first_station.id, "First", "sk-a"))
        .await
        .expect("first key");
    let second_key = service
        .create_station_key(station_key_input(&second_station.id, "Second", "sk-b"))
        .await
        .expect("second key");
    let mut remote_row = remote_key_row(&first_station.id, "remote-a");
    remote_row.api_key_fingerprint = crate::models::remote_keys::api_key_fingerprint("sk-a");
    let remote = service
        .upsert_remote_station_key(remote_row)
        .await
        .expect("remote key");

    let error = service
        .bind_remote_station_key(remote.id.clone(), second_key.id)
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "not found");

    let bound = service
        .bind_remote_station_key(remote.id, first_key.id.clone())
        .await
        .expect("same station binding");
    assert_eq!(bound.len(), 1);
    assert_eq!(
        bound[0].matched_station_key_id.as_deref(),
        Some(first_key.id.as_str())
    );
    assert_eq!(bound[0].match_status, RemoteKeyMatchStatus::Matched);
}

#[tokio::test]
async fn remote_key_relationship_follows_secret_identity_and_cannot_be_manually_unbound() {
    let fixture = V2Fixture::create().await;
    let station = fixture
        .station_service(vec!["station-identity"])
        .await
        .create(station_input("RemoteIdentity"))
        .await
        .expect("station");
    let service = fixture
        .credential_service(
            Arc::new(DeterministicCredentialVault::new([10; 32])),
            vec![
                "key-exact",
                "secret-exact",
                "key-other",
                "secret-other",
                "secret-updated",
            ],
        )
        .await;
    let exact_key = service
        .create_station_key(station_key_input(&station.id, "Exact", "sk-exact"))
        .await
        .expect("exact key");
    let other_key = service
        .create_station_key(station_key_input(&station.id, "Other", "sk-other"))
        .await
        .expect("other key");
    let mut remote_row = remote_key_row(&station.id, "remote-identity");
    remote_row.api_key_fingerprint = crate::models::remote_keys::api_key_fingerprint("sk-exact");
    let remote = service
        .upsert_remote_station_key(remote_row)
        .await
        .expect("remote key");

    let mismatch = service
        .bind_remote_station_key(remote.id.clone(), other_key.id)
        .await
        .expect_err("a different secret must not bind");
    assert!(matches!(
        mismatch,
        crate::application::error::ApplicationError::ConstraintViolation
    ));

    service
        .bind_remote_station_key(remote.id.clone(), exact_key.id.clone())
        .await
        .expect("exact fingerprint binding");
    let unbind = service
        .unbind_remote_station_key(remote.id.clone(), station.id.clone())
        .await
        .expect_err("identity relationships cannot be manually unbound");
    assert!(matches!(
        unbind,
        crate::application::error::ApplicationError::ConstraintViolation
    ));
    assert_eq!(
        service
            .list_remote_station_keys(station.id.clone())
            .await
            .expect("bound remote")[0]
            .matched_station_key_id
            .as_deref(),
        Some(exact_key.id.as_str())
    );

    service
        .update_station_key(UpdateStationKeyInput {
            id: exact_key.id,
            station_id: station.id.clone(),
            name: exact_key.name,
            api_key: Some("sk-changed".to_string()),
            enabled: exact_key.enabled,
            priority: exact_key.priority,
            max_concurrency: exact_key.max_concurrency,
            load_factor: exact_key.load_factor,
            schedulable: exact_key.schedulable,
            group_name: exact_key.group_name,
            tier_label: exact_key.tier_label,
            group_binding_id: exact_key.group_binding_id,
            group_id_hash: exact_key.group_id_hash,
            rate_multiplier: exact_key.rate_multiplier,
            manual_rate_multiplier: None,
            rate_source: exact_key.rate_source,
            balance_scope: exact_key.balance_scope,
            status: exact_key.status,
            note: exact_key.note,
        })
        .await
        .expect("replace local secret");
    let remotes = service
        .list_remote_station_keys(station.id)
        .await
        .expect("remote after secret replacement");
    assert_eq!(remotes[0].match_status, RemoteKeyMatchStatus::Unbound);
    assert!(remotes[0].matched_station_key_id.is_none());
}

#[tokio::test]
async fn importing_remote_key_preserves_adapter_discovery_order() {
    let fixture = V2Fixture::create().await;
    let station = fixture
        .station_service(vec!["station-discovery-order"])
        .await
        .create(station_input("RemoteDiscoveryOrder"))
        .await
        .expect("station");
    let service = fixture
        .credential_service(
            Arc::new(DeterministicCredentialVault::new([11; 32])),
            vec!["key-imported", "secret-imported"],
        )
        .await;
    let remote_key = |id: &str, full_key: &str, collected_at: &str| RemoteStationKey {
        id: id.to_string(),
        station_id: station.id.clone(),
        remote_key_id_hash: Some(format!("{id}-hash")),
        remote_key_name: Some(id.to_string()),
        api_key_masked: Some("sk-***mote".to_string()),
        api_key_fingerprint: crate::models::remote_keys::api_key_fingerprint(full_key),
        group_id_hash: None,
        group_name: None,
        tier_label: None,
        rate_multiplier: None,
        rate_source: None,
        created_at: None,
        last_used_at: None,
        raw_source: "collector".to_string(),
        match_status: RemoteKeyMatchStatus::Unbound,
        matched_station_key_id: None,
        match_confidence: 0.0,
        collected_at: collected_at.to_string(),
    };
    let scanned = service
        .replace_remote_station_keys_and_metadata(
            station.id.clone(),
            station.endpoint_revision,
            vec![
                remote_key("remote-first", "sk-first", "1"),
                remote_key("remote-imported", "sk-imported", "2"),
            ],
            Vec::new(),
        )
        .await
        .expect("replace remote discovery snapshot");
    assert_eq!(
        scanned
            .iter()
            .map(|key| key.id.as_str())
            .collect::<Vec<_>>(),
        vec!["remote-first", "remote-imported"]
    );

    service
        .save_remote_station_key_with_local(
            scanned[1].clone(),
            station.endpoint_revision,
            None,
            None,
            "sk-imported".to_string(),
        )
        .await
        .expect("import second discovered key");
    let after_import = service
        .list_remote_station_keys(station.id)
        .await
        .expect("list remote keys after import");

    assert_eq!(
        after_import
            .iter()
            .map(|key| key.id.as_str())
            .collect::<Vec<_>>(),
        vec!["remote-first", "remote-imported"]
    );
    assert_eq!(after_import[1].match_status, RemoteKeyMatchStatus::Matched);
}

#[tokio::test]
async fn routing_service_loads_v2_runtime_candidates_and_workflow_queries() {
    let fixture = V2Fixture::create().await;
    let station_service = fixture.station_service(vec!["station-routing"]).await;
    let station = station_service
        .create(station_input("RoutingStation"))
        .await
        .expect("station");

    let mut connection = fixture.connect().await;
    sqlx::query(
        r#"
        UPDATE settings
           SET value = ?1
         WHERE key = 'collector_proxy_mode'
        "#,
    )
    .bind("manual")
    .execute(&mut connection)
    .await
    .expect("proxy mode");
    sqlx::query(
        r#"
        UPDATE settings
           SET value = ?1
         WHERE key = 'collector_proxy_url'
        "#,
    )
    .bind("http://127.0.0.1:7890")
    .execute(&mut connection)
    .await
    .expect("proxy url");
    sqlx::query(
        r#"
        INSERT INTO secrets (
            id, scope, owner_id, kind, masked_value, ciphertext, nonce, created_at, updated_at
        ) VALUES (
            'secret-routing-key',
            'station_key',
            'routing-key',
            'api_key',
            'sk-***ey',
            x'010203',
            x'0405060708090A0B0C0D0E0F',
            '1',
            '1'
        )
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("secret");
    sqlx::query(
        r#"
        INSERT INTO station_keys (
            id, station_id, name, api_key, api_key_secret_id, enabled, priority,
            max_concurrency, load_factor, schedulable, group_name, tier_label,
            group_binding_id, group_id_hash, rate_multiplier, manual_rate_multiplier,
            manual_rate_updated_at, rate_source, balance_scope, status, note,
            created_at, updated_at
        ) VALUES (
            'routing-key',
            ?1,
            'Routing Key',
            '',
            'secret-routing-key',
            1,
            7,
            3,
            2,
            1,
            'Group A',
            'Tier 1',
            'binding-a',
            'hash-a',
            1.5,
            1.25,
            '123456',
            'manual',
            'station',
            'unchecked',
            NULL,
            '1',
            '1'
        )
        "#,
    )
    .bind(&station.id)
    .execute(&mut connection)
    .await
    .expect("station key");
    sqlx::query(
        r#"
        INSERT INTO station_group_bindings (
            id, station_id, station_key_id, binding_kind, parent_group_binding_id,
            group_key_hash, group_id_hash, group_name, binding_status,
            default_rate_multiplier, user_rate_multiplier, effective_rate_multiplier,
            inferred_group_category, group_category_override, rate_source, confidence,
            last_seen_at, last_checked_at, last_rate_changed_at, raw_json_redacted,
            created_at, updated_at
        ) VALUES (
            'binding-a',
            ?1,
            NULL,
            'station_group',
            NULL,
            'group-key-a',
            'binding-hash-a',
            'Bound Group A',
            'bound',
            1.0,
            0.8,
            0.8,
            'gpt',
            NULL,
            'collector',
            0.92,
            '123450',
            '123455',
            '123455',
            NULL,
            '123450',
            '123455'
        )
        "#,
    )
    .bind(&station.id)
    .execute(&mut connection)
    .await
    .expect("group binding");
    sqlx::query(
        r#"
        INSERT INTO station_key_capabilities (
            station_key_id, supports_chat_completions, supports_responses, supports_embeddings,
            supports_stream, supports_tools, supports_vision, supports_reasoning,
            model_allowlist_json, model_blocklist_json, preferred_models_json,
            only_use_as_backup, routing_tags_json, updated_at
        ) VALUES (
            'routing-key',
            1, 1, 0, 1, 0, 0, 0,
            '["gpt-5"]',
            '[]',
            '["gpt-5.1"]',
            0,
            '["primary"]',
            '1'
        )
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("capabilities");
    sqlx::query(
        r#"
        INSERT INTO routing_health_snapshot (
            station_key_id, endpoint_revision, last_success_at, last_failure_at,
            consecutive_failures, success_count, failure_count, avg_latency_ms,
            last_error_summary, cooldown_until, updated_at
        ) VALUES (
            'routing-key',
            1,
            '111',
            '222',
            2,
            7,
            3,
            88,
            'timeout',
            '333',
            '444'
        )
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("health");
    sqlx::query(
        r#"
        INSERT INTO balance_snapshots (
            id, station_id, station_key_id, scope, value, currency, credit_unit,
            used_value, total_value, today_request_count, total_request_count,
            today_consumption, total_consumption, today_base_consumption,
            total_base_consumption, today_token_count, total_token_count,
            today_input_token_count, today_output_token_count,
            total_input_token_count, total_output_token_count,
            account_concurrency_limit, low_balance_threshold, status, source,
            confidence, collected_at, created_at, updated_at
        ) VALUES (
            'balance-routing',
            ?1,
            'routing-key',
            'station',
            12.5,
            'CNY',
            'credit',
            1.5,
            14.0,
            3,
            9,
            2.5,
            9.9,
            2.0,
            8.8,
            10,
            30,
            12,
            18,
            12,
            18,
            8,
            10.0,
            'healthy',
            'collector',
            0.85,
            '555',
            '1',
            '2'
        )
        "#,
    )
    .bind(&station.id)
    .execute(&mut connection)
    .await
    .expect("balance");
    sqlx::query(
        r#"
        INSERT INTO model_aliases (
            id, client_model, upstream_model, enabled, note, created_at, updated_at
        ) VALUES (
            'alias-routing',
            'gpt-test',
            'gpt-5',
            1,
            'routing alias',
            '1',
            '1'
        )
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("alias");
    connection.close().await.expect("close fixture");

    let service = RoutingService::new(fixture.runtime().await.handle());
    let diagnostics = RoutingDiagnosticsReader::new(fixture.runtime().await.handle());
    let request = RouteRequestClassifier::classify(
        CanonicalRouteRequest {
            route_kind: RouteKind::Inference,
            requested_model: Some("gpt-5".to_string()),
            stream: false,
            uses_tools: false,
            uses_vision: false,
            uses_reasoning: false,
            untrusted_headers: Vec::new(),
        },
        validated_route_settings(&RuntimeRoutingSettings {
            max_rate_multiplier: Some(2.0),
            routing_group_scope: RoutingGroupFilter::AllGroups,
            allow_depleted_fallback: false,
            ..Default::default()
        }),
        123457,
    );
    let priced_candidates = service
        .load_runtime_candidates_with_request_pricing(&request)
        .await
        .expect("request-scoped routing candidates");
    let candidates = priced_candidates
        .iter()
        .map(|row| &row.candidate)
        .collect::<Vec<_>>();
    let alias_pairs = diagnostics
        .list_model_alias_pairs()
        .await
        .expect("alias pairs");
    let balances = diagnostics
        .list_balance_snapshots_for_station(&station.id)
        .await
        .expect("balances");

    assert_eq!(
        alias_pairs,
        vec![("gpt-test".to_string(), "gpt-5".to_string())]
    );
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].scope, "station");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].station_key_id, "routing-key");
    assert_eq!(candidates[0].station_account_concurrency_limit, Some(8));
    assert_eq!(candidates[0].priority, 7);
    assert_eq!(candidates[0].routing_order, None);
    assert_eq!(candidates[0].collector_proxy_mode, "inherit");
    assert_eq!(candidates[0].collector_proxy_url.as_deref(), None);
    assert_eq!(candidates[0].capabilities.preferred_models, vec!["gpt-5.1"]);
    let economics = candidates[0]
        .economic_snapshot
        .as_ref()
        .expect("runtime economic snapshot");
    assert_eq!(economics.group_binding_id.as_deref(), Some("binding-a"));
    assert_eq!(economics.group_key_hash.as_deref(), Some("group-key-a"));
    assert_eq!(economics.group_id_hash.as_deref(), Some("hash-a"));
    assert_eq!(economics.group_name.as_deref(), Some("Bound Group A"));
    assert_eq!(economics.group_status.as_deref(), Some("bound"));
    assert_eq!(economics.group_confidence, Some(0.92));
    assert_eq!(economics.group_checked_at.as_deref(), Some("123455"));
    assert_eq!(economics.rate_multiplier, Some(1.5));
    assert_eq!(economics.manual_rate_multiplier, Some(1.25));
    assert_eq!(economics.manual_rate_updated_at.as_deref(), Some("123456"));
    assert_eq!(economics.rate_source.as_deref(), Some("manual"));
    assert_eq!(priced_candidates.len(), 1);
    let projection = route_projection_from_runtime_candidate_with_pricing(
        &request,
        priced_candidates[0].candidate.clone(),
        priced_candidates[0].pricing_context.as_ref(),
    )
    .expect("priced projection");
    assert_eq!(projection.pricing.basis, RoutingCostBasis::MultiplierProxy);
    assert_eq!(projection.pricing.comparison_value, Some(1.5));
    assert!(candidates[0].api_key_secret.is_some());
    assert!(matches!(
        candidates[0]
            .balance_snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.as_str()),
        Some("healthy")
    ));
}

fn station_input(name: &str) -> CreateStationInput {
    CreateStationInput {
        name: name.to_string(),
        station_type: "newapi".to_string(),
        website_url: format!("https://{name}.example"),
        api_base_url: format!("https://{name}.example/v1"),
        api_key: String::new(),
        collector_proxy_mode: "inherit".to_string(),
        collector_proxy_url: None,
        enabled: true,
        credit_per_cny: 1.0,
        low_balance_threshold_cny: None,
        collection_interval_minutes: 5,
        note: None,
    }
}

fn station_key_input(station_id: &str, name: &str, api_key: &str) -> CreateStationKeyInput {
    CreateStationKeyInput {
        station_id: station_id.to_string(),
        name: name.to_string(),
        api_key: api_key.to_string(),
        enabled: true,
        priority: None,
        max_concurrency: Some(3),
        load_factor: None,
        schedulable: Some(true),
        group_name: None,
        tier_label: None,
        group_binding_id: None,
        group_id_hash: None,
        rate_multiplier: None,
        manual_rate_multiplier: None,
        rate_source: None,
        balance_scope: None,
        note: None,
    }
}

fn remote_key_row(
    station_id: &str,
    id: &str,
) -> persistence::stores::credential_store::NewRemoteStationKeyRow {
    persistence::stores::credential_store::NewRemoteStationKeyRow {
        id: id.to_string(),
        station_id: station_id.to_string(),
        remote_key_id_hash: Some(format!("{id}-hash")),
        remote_key_name: Some(id.to_string()),
        api_key_masked: Some("sk-***mote".to_string()),
        api_key_fingerprint: Some("fingerprint".to_string()),
        group_id_hash: None,
        group_name: None,
        tier_label: None,
        rate_multiplier: None,
        rate_source: None,
        created_at: Some("1".to_string()),
        last_used_at: None,
        raw_source: "collector".to_string(),
        collected_at: "2".to_string(),
        now: "2".to_string(),
    }
}

fn settings_input() -> UpdateSettingsInput {
    UpdateSettingsInput {
        local_proxy_port: 8787,
        collector_proxy_mode: "direct".to_string(),
        collector_proxy_url: None,
        low_balance_threshold_cny: 15.0,
        collector_interval_minutes: 30,
        balance_interval_minutes: 5,
        group_rate_interval_minutes: 20,
        published_status_interval_minutes: 5,
        pricing_refresh_interval_minutes: 60,
        collector_timeout_seconds: 15,
        collector_max_concurrency: 3,
        developer_mode_enabled: false,
        show_decision_explanation: false,
        tray_behavior: None,
    }
}

#[derive(Clone)]
struct FixedClock;

impl Clock for FixedClock {
    fn now_utc(&self) -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).unwrap()
    }
}

#[derive(Default)]
struct SequenceIds {
    ids: Mutex<Vec<String>>,
}

impl SequenceIds {
    fn new(ids: Vec<&str>) -> Self {
        Self {
            ids: Mutex::new(ids.into_iter().map(ToString::to_string).rev().collect()),
        }
    }
}

impl IdGenerator for SequenceIds {
    fn next_id(&self) -> String {
        self.ids
            .lock()
            .expect("ids")
            .pop()
            .expect("deterministic id")
    }
}

struct DeterministicCredentialVault {
    key: [u8; 32],
    last_aad: Mutex<Option<String>>,
}

impl DeterministicCredentialVault {
    fn new(key: [u8; 32]) -> Self {
        Self {
            key,
            last_aad: Mutex::new(None),
        }
    }

    fn last_aad(&self) -> String {
        self.last_aad
            .lock()
            .expect("last aad")
            .clone()
            .expect("aad recorded")
    }
}

impl CredentialVault for DeterministicCredentialVault {
    fn encrypt(
        &self,
        aad: &str,
        plaintext: SecretBytes,
    ) -> Result<EncryptedSecret, CredentialError> {
        *self.last_aad.lock().expect("last aad") = Some(aad.to_string());
        let mut ciphertext = Vec::with_capacity(plaintext.as_bytes().len());
        for (index, byte) in plaintext.as_bytes().iter().enumerate() {
            ciphertext.push(byte ^ self.key[index % self.key.len()]);
        }
        ciphertext.reverse();
        Ok(EncryptedSecret {
            ciphertext,
            nonce: self.key[..12].to_vec(),
            masked_value: mask_secret_bytes(plaintext.as_bytes()),
            key_id: "deterministic-test-key".to_string(),
            encryption_version: crate::services::secrets::CURRENT_SECRET_ENCRYPTION_VERSION,
            value_hash: crate::services::secrets::crypto::hash_secret(&String::from_utf8_lossy(
                plaintext.as_bytes(),
            )),
        })
    }

    fn decrypt(
        &self,
        aad: &str,
        _key_id: &str,
        _encryption_version: u16,
        encrypted: &EncryptedSecret,
    ) -> Result<SecretBytes, CredentialError> {
        *self.last_aad.lock().expect("last aad") = Some(aad.to_string());
        let mut plaintext = encrypted.ciphertext.clone();
        plaintext.reverse();
        for (index, byte) in plaintext.iter_mut().enumerate() {
            *byte ^= self.key[index % self.key.len()];
        }
        Ok(SecretBytes::from(plaintext))
    }
}

fn mask_secret_bytes(secret: &[u8]) -> String {
    if secret.len() <= 7 {
        return "***".to_string();
    }
    let prefix = String::from_utf8_lossy(&secret[..3]);
    let suffix = String::from_utf8_lossy(&secret[secret.len() - 4..]);
    format!("{prefix}***{suffix}")
}

struct V2Fixture {
    path: PathBuf,
}

impl V2Fixture {
    async fn create() -> Self {
        let path = temp_db_path("differential");
        persistence::migrations::initialize_v2_database(&path)
            .await
            .expect("initialize fixture");
        Self { path }
    }

    async fn station_service(&self, ids: Vec<&str>) -> StationService {
        StationService::new(
            self.runtime().await.handle(),
            Arc::new(FixedClock),
            Arc::new(SequenceIds::new(ids)),
        )
    }

    async fn settings_service(&self) -> SettingsService {
        SettingsService::new(
            self.runtime().await.handle(),
            Arc::new(FixedClock),
            Arc::new(SequenceIds::new(vec!["settings-secret-1"])),
            Arc::new(crate::services::secrets::vault::DataKeyVault::for_test(
                [44; 32],
            )),
            "fixture-data-dir".to_string(),
            None,
        )
    }

    async fn credential_service(
        &self,
        vault: Arc<dyn CredentialVault>,
        ids: Vec<&str>,
    ) -> CredentialService {
        CredentialService::new(
            self.runtime().await.handle(),
            vault,
            Arc::new(FixedClock),
            Arc::new(SequenceIds::new(ids)),
        )
    }

    async fn runtime(&self) -> PersistenceRuntime {
        PersistenceRuntime::open(&self.path, current_test_binary())
            .await
            .expect("open runtime")
    }

    async fn seed_planning_candidate(
        &self,
        key_id: &str,
        station_id: &str,
        group_binding_id: Option<&str>,
        upstream_model: &str,
    ) {
        let mut connection = self.connect().await;
        sqlx::query("INSERT INTO stations (id, name, station_type, website_url, api_base_url, api_key, enabled, created_at, updated_at) VALUES (?1, ?1, 'newapi', 'https://fixture.invalid', 'https://fixture.invalid/v1', '', 1, '1', '1')")
            .bind(station_id)
            .execute(&mut connection)
            .await
            .expect("planning station");
        if let Some(group_id) = group_binding_id {
            sqlx::query("INSERT INTO station_group_bindings (id, station_id, station_key_id, binding_kind, group_key_hash, group_name, binding_status, created_at, updated_at) VALUES (?1, ?2, NULL, 'station_group', ?1, ?1, 'bound', '1', '1')")
                .bind(group_id)
                .bind(station_id)
                .execute(&mut connection)
                .await
                .expect("planning group");
        }
        sqlx::query("INSERT INTO station_keys (id, station_id, name, api_key, enabled, priority, group_binding_id, created_at, updated_at) VALUES (?1, ?2, ?1, 'sk-test', 1, 1, ?3, '1', '1')")
            .bind(key_id)
            .bind(station_id)
            .bind(group_binding_id)
            .execute(&mut connection)
            .await
            .expect("planning key");
        sqlx::query("INSERT INTO station_key_capabilities (station_key_id, supports_chat_completions, supports_responses, supports_embeddings, supports_stream, supports_tools, supports_vision, supports_reasoning, model_allowlist_json, model_blocklist_json, preferred_models_json, only_use_as_backup, routing_tags_json, updated_at) VALUES (?1, 1, 1, 0, 1, 1, 1, 1, '[]', '[]', '[]', 0, '[]', '1')")
            .bind(key_id)
            .execute(&mut connection)
            .await
            .expect("planning capabilities");
        sqlx::query("INSERT OR IGNORE INTO model_aliases (id, client_model, upstream_model, enabled, created_at, updated_at) VALUES ('alias-planning', 'gpt-test', ?1, 1, '1', '1')")
            .bind(upstream_model)
            .execute(&mut connection)
            .await
            .expect("planning alias");
        for scope in [
            format!("station:{station_id}"),
            format!("station_key:{key_id}"),
            format!("station_account:{station_id}"),
            "model_alias:alias-planning".to_string(),
        ]
        .into_iter()
        .chain(group_binding_id.map(|id| format!("station_group:{id}")))
        {
            sqlx::query("INSERT OR IGNORE INTO domain_revisions (scope, revision, updated_at_ms, provenance) VALUES (?1, 1, 0, 'baseline_snapshot')")
                .bind(scope)
                .execute(&mut connection)
                .await
                .expect("planning revision");
        }
        let key_revision: Option<i64> =
            sqlx::query_scalar("SELECT revision FROM domain_revisions WHERE scope = ?1")
                .bind(format!("station_key:{key_id}"))
                .fetch_optional(&mut connection)
                .await
                .expect("read planning key revision");
        assert_eq!(key_revision, Some(1));
        let joined_revision: Option<i64> = sqlx::query_scalar(
            "SELECT r.revision FROM station_keys k LEFT JOIN domain_revisions r ON r.scope = 'station_key:' || k.id WHERE k.id = ?1",
        )
        .bind(key_id)
        .fetch_one(&mut connection)
        .await
        .expect("join planning key revision");
        assert_eq!(joined_revision, Some(1));
        connection.close().await.expect("close planning seed");
    }

    async fn bump_group_revision(&self, group_binding_id: &str) {
        let mut connection = self.connect().await;
        sqlx::query("UPDATE station_group_bindings SET updated_at = '2' WHERE id = ?1")
            .bind(group_binding_id)
            .execute(&mut connection)
            .await
            .expect("bump group revision");
        connection.close().await.expect("close group revision");
    }

    async fn bump_key_revision(&self, key_id: &str) {
        let mut connection = self.connect().await;
        sqlx::query("UPDATE station_keys SET api_key = 'sk-test-rotated' WHERE id = ?1")
            .bind(key_id)
            .execute(&mut connection)
            .await
            .expect("bump key revision");
        connection.close().await.expect("close key revision");
    }

    async fn seed_endpoint_state(&self, station_id: &str) {
        let mut connection = self.connect().await;
        sqlx::query("INSERT INTO station_keys (id, station_id) VALUES ('key-1', ?1)")
            .bind(station_id)
            .execute(&mut connection)
            .await
            .expect("station key");
        sqlx::query(
            "INSERT INTO endpoint_health_snapshot (station_id, endpoint_revision) VALUES (?1, 1)",
        )
        .bind(station_id)
        .execute(&mut connection)
        .await
        .expect("endpoint health");
        sqlx::query("INSERT INTO routing_health_snapshot (station_key_id, endpoint_revision) VALUES ('key-1', 1)")
            .execute(&mut connection)
            .await
            .expect("key health");
        sqlx::query(
            r#"
            INSERT INTO secrets (id, scope, owner_id, kind, masked_value, ciphertext, nonce, created_at, updated_at)
            VALUES
                ('secret-pass', 'station', ?1, 'login_password', '***pass', x'01', x'02', '1', '1'),
                ('secret-token', 'station', ?1, 'access_token', '***tokn', x'03', x'04', '1', '1')
            "#,
        )
        .bind(station_id)
        .execute(&mut connection)
        .await
        .expect("secrets");
        sqlx::query(
            r#"
            INSERT INTO station_credentials (
                station_id, login_password, login_password_secret_id, remember_password,
                login_status, session_status, access_token_secret_id, session_source, updated_at
            ) VALUES (?1, NULL, 'secret-pass', 1, 'logged_in', 'active', 'secret-token', 'web', '1')
            "#,
        )
        .bind(station_id)
        .execute(&mut connection)
        .await
        .expect("credentials");
        connection.close().await.expect("close fixture");
    }

    async fn endpoint_health_rows(&self) -> i64 {
        self.count("endpoint_health_snapshot").await
    }

    async fn station_key_health_rows(&self) -> i64 {
        self.count("routing_health_snapshot").await
    }

    async fn secret_rows(&self) -> i64 {
        self.count("secrets").await
    }

    async fn any_setting_contains(&self, needle: &str) -> bool {
        let mut connection = self.connect().await;
        let row = sqlx::query("SELECT COUNT(*) AS count FROM settings WHERE value LIKE ?1")
            .bind(format!("%{needle}%"))
            .fetch_one(&mut connection)
            .await
            .expect("setting scan");
        connection.close().await.expect("close fixture");
        row.get::<i64, _>("count") > 0
    }

    async fn any_text_contains(&self, needle: &str) -> bool {
        let mut connection = self.connect().await;
        for query in [
            "SELECT COUNT(*) AS count FROM secrets WHERE id LIKE ?1 OR scope LIKE ?1 OR owner_id LIKE ?1 OR kind LIKE ?1 OR masked_value LIKE ?1",
            "SELECT COUNT(*) AS count FROM station_keys WHERE id LIKE ?1 OR station_id LIKE ?1 OR name LIKE ?1 OR api_key LIKE ?1 OR COALESCE(api_key_secret_id, '') LIKE ?1 OR COALESCE(note, '') LIKE ?1",
            "SELECT COUNT(*) AS count FROM remote_station_keys WHERE id LIKE ?1 OR station_id LIKE ?1 OR COALESCE(api_key_masked, '') LIKE ?1 OR COALESCE(remote_key_name, '') LIKE ?1",
        ] {
            let row = sqlx::query(query)
                .bind(format!("%{needle}%"))
                .fetch_one(&mut connection)
                .await
                .expect("text canary scan");
            if row.get::<i64, _>("count") > 0 {
                connection.close().await.expect("close fixture");
                return true;
            }
        }
        connection.close().await.expect("close fixture");
        false
    }

    async fn any_blob_contains(&self, needle: &[u8]) -> bool {
        let mut connection = self.connect().await;
        let rows = sqlx::query("SELECT ciphertext, nonce FROM secrets")
            .fetch_all(&mut connection)
            .await
            .expect("blob canary scan");
        connection.close().await.expect("close fixture");
        rows.into_iter().any(|row| {
            let ciphertext: Vec<u8> = row.get("ciphertext");
            let nonce: Vec<u8> = row.get("nonce");
            contains_bytes(&ciphertext, needle) || contains_bytes(&nonce, needle)
        })
    }

    async fn station_key_secret_id(&self, station_key_id: &str) -> Option<String> {
        let mut connection = self.connect().await;
        let row = sqlx::query("SELECT api_key_secret_id FROM station_keys WHERE id = ?1")
            .bind(station_key_id)
            .fetch_one(&mut connection)
            .await
            .expect("station key secret id");
        connection.close().await.expect("close fixture");
        row.get("api_key_secret_id")
    }

    async fn credential_session_source(&self, station_id: &str) -> String {
        let mut connection = self.connect().await;
        let row =
            sqlx::query("SELECT session_source FROM station_credentials WHERE station_id = ?1")
                .bind(station_id)
                .fetch_one(&mut connection)
                .await
                .expect("credential row");
        connection.close().await.expect("close fixture");
        row.get("session_source")
    }

    async fn count(&self, table: &str) -> i64 {
        let mut connection = self.connect().await;
        let row = sqlx::query(&format!("SELECT COUNT(*) AS count FROM {table}"))
            .fetch_one(&mut connection)
            .await
            .expect("count rows");
        connection.close().await.expect("close fixture");
        row.get("count")
    }

    async fn connect(&self) -> sqlx::SqliteConnection {
        SqliteConnectOptions::new()
            .filename(&self.path)
            .create_if_missing(false)
            .connect()
            .await
            .expect("connect fixture")
    }
}

fn current_test_binary() -> BinaryCompatibility {
    let schema_version = persistence::current_schema_version();
    BinaryCompatibility {
        app_version: Version::new(0, 3, 3),
        database_generation: 2,
        readable_schema: 1..=schema_version,
        writable_schema: BTreeSet::from([schema_version]),
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn temp_db_path(name: &str) -> PathBuf {
    let id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
    let process_id = std::process::id();
    let root =
        std::env::temp_dir().join(format!("relay-pool-persistence-{name}-{process_id}-{id}"));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("clean stale fixture dir");
    }
    std::fs::create_dir_all(&root).expect("fixture dir");
    root.join("relay-pool-v2.sqlite3")
}
