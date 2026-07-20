mod models {
    pub(crate) mod routing {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/routing.rs"
        ));
    }

    pub(crate) mod pricing {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/pricing.rs"
        ));
    }

    pub(crate) mod proxy {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/models/proxy.rs"));
    }

    pub(crate) mod settings {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/settings.rs"
        ));
    }

    pub(crate) mod remote_keys {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/remote_keys.rs"
        ));
    }

    pub(crate) mod station_keys {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/station_keys.rs"
        ));
    }

    pub(crate) mod stations {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/stations.rs"
        ));
    }
}

mod services {
    pub(crate) mod proxy {
        pub(crate) mod lifecycle {
            pub(crate) mod attempt {
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/src/services/proxy/lifecycle/attempt.rs"
                ));
            }
            pub(crate) mod delivery {
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/src/services/proxy/lifecycle/delivery.rs"
                ));
            }
            pub(crate) mod ports {
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/src/services/proxy/lifecycle/ports.rs"
                ));
            }
            pub(crate) mod request {
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/src/services/proxy/lifecycle/request.rs"
                ));
            }
        }
    }

    pub(crate) mod outbound {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/services/outbound.rs"
        ));
    }

    pub(crate) mod secrets {
        pub(crate) mod mask {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/services/secrets/mask.rs"
            ));
        }
    }

    pub(crate) mod station_endpoints {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/services/station_endpoints.rs"
        ));
    }
}

mod persistence {
    pub(crate) mod error {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/error.rs"
        ));
    }

    pub(crate) mod write_coordinator {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/write_coordinator.rs"
        ));
    }

    pub(crate) mod write_session {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/write_session.rs"
        ));
    }

    pub(crate) mod read_session {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/read_session.rs"
        ));
    }

    pub(crate) mod backup {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/backup.rs"
        ));
    }

    pub(crate) mod runtime {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/runtime.rs"
        ));
    }

    pub(crate) mod schema_compatibility {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/schema_compatibility.rs"
        ));
    }

    pub(crate) mod health_check {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/health_check.rs"
        ));
    }

    pub(crate) mod migrations {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/migrations.rs"
        ));
    }

    pub(crate) mod stores {
        pub(crate) mod station_catalog {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/persistence/stores/station_catalog.rs"
            ));
        }

        pub(crate) mod routing_store {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/persistence/stores/routing_store.rs"
            ));
        }

        pub(crate) mod settings_store {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/persistence/stores/settings_store.rs"
            ));
        }

        pub(crate) mod credential_store {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/persistence/stores/credential_store.rs"
            ));
        }

        pub(crate) mod request_log_store {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/persistence/stores/request_log_store.rs"
            ));
        }
    }
}

mod application {
    pub(crate) mod clock {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/clock.rs"
        ));
    }

    pub(crate) mod error {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/error.rs"
        ));
    }

    pub(crate) mod ids {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/ids.rs"
        ));
    }

    pub(crate) mod routing {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/routing.rs"
        ));
    }

    pub(crate) mod stations {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/stations.rs"
        ));
    }

    pub(crate) mod credentials {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/credentials.rs"
        ));
    }

    pub(crate) mod settings {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/settings.rs"
        ));
    }

    pub(crate) mod request_finalization {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/request_finalization.rs"
        ));
    }
}

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use application::{
    clock::Clock,
    credentials::{
        CredentialError, CredentialService, CredentialVault, EncryptedSecret, SecretBytes,
    },
    ids::IdGenerator,
    request_finalization::RequestFinalizationService,
    routing::RoutingService,
    settings::SettingsService,
    stations::StationService,
};
use chrono::{TimeZone, Utc};
use models::{
    remote_keys::RemoteKeyMatchStatus,
    routing::{RoutingProxyDefaults, RuntimeRoutingCandidate},
    settings::UpdateSettingsInput,
    station_keys::{CreateStationKeyInput, UpdateStationKeyInput},
    stations::{CreateStationInput, UpdateStationInput},
};
use persistence::stores::request_log_store::RequestLogStore;
use persistence::{runtime::PersistenceRuntime, schema_compatibility::BinaryCompatibility};
use semver::Version;
use services::proxy::lifecycle::{
    delivery::DeliveryTerminal,
    ports::RequestLifecycleStore,
    request::{
        FinalRequestRecord, RequestCompletion, RequestContextSnapshot, RequestLifecycle,
        RequestStartRecord, RequestTerminal,
    },
};
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
async fn station_endpoint_change_is_atomic_and_matches_v1_contract_boundary() {
    let fixture = V2Fixture::create().await;
    let station_service = fixture.station_service(vec!["station-1"]).await;
    let station = station_service
        .create(CreateStationInput {
            name: "Relay".to_string(),
            station_type: "openai-compatible".to_string(),
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
            station_type: "openai-compatible".to_string(),
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
            default_routing_strategy: "priority_fallback".to_string(),
            collector_proxy_mode: "direct".to_string(),
            collector_proxy_url: None,
            max_rate_multiplier: Some(Some(3.5)),
            default_routing_group_filter: None,
            scheduler_advanced_settings: None,
            low_balance_threshold_cny: 8.0,
            collector_interval_minutes: 15,
            balance_interval_minutes: 5,
            group_rate_interval_minutes: 20,
            model_list_interval_minutes: 60,
            pricing_refresh_interval_minutes: 60,
            collector_timeout_seconds: 15,
            collector_max_concurrency: 2,
            allow_depleted_fallback: true,
            developer_mode_enabled: true,
            tray_behavior: Some("disabled".to_string()),
        })
        .await
        .expect("update settings");

    assert_eq!(settings.local_proxy_port, 8788);
    assert_eq!(settings.max_rate_multiplier, Some(3.5));
    assert_eq!(settings.tray_behavior, "disabled");
    assert_eq!(settings.collector_max_concurrency, 2);

    let error = service
        .update(UpdateSettingsInput {
            collector_max_concurrency: 9,
            ..settings_input()
        })
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "not found");
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
    assert_eq!(vault.last_aad(), "station_key:key-1:api_key");
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
    let remote = service
        .upsert_remote_station_key(remote_key_row(&first_station.id, "remote-a"))
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
            NULL,
            NULL,
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
        INSERT INTO station_key_health (
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
    let candidates = service.load_runtime_candidates().await.expect("candidates");
    let proxy_defaults = service.load_proxy_defaults().await.expect("defaults");
    let alias_pairs = service.list_model_alias_pairs().await.expect("alias pairs");
    let health = service
        .station_key_health_by_id("routing-key")
        .await
        .expect("health by id");
    let balances = service
        .list_balance_snapshots_for_station(&station.id)
        .await
        .expect("balances");

    assert_eq!(proxy_defaults.collector_proxy_mode, "manual");
    assert_eq!(
        proxy_defaults.collector_proxy_url.as_deref(),
        Some("http://127.0.0.1:7890")
    );
    assert_eq!(
        alias_pairs,
        vec![("gpt-test".to_string(), "gpt-5".to_string())]
    );
    assert_eq!(health.consecutive_failures, 2);
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].scope, "station");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].station_key_id, "routing-key");
    assert_eq!(candidates[0].priority, 7);
    assert_eq!(candidates[0].routing_order, None);
    assert_eq!(candidates[0].collector_proxy_mode, "inherit");
    assert_eq!(candidates[0].collector_proxy_url.as_deref(), None);
    assert_eq!(candidates[0].capabilities.preferred_models, vec!["gpt-5.1"]);
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
        station_type: "openai-compatible".to_string(),
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
        default_routing_strategy: "cost_stable_first".to_string(),
        collector_proxy_mode: "direct".to_string(),
        collector_proxy_url: None,
        max_rate_multiplier: None,
        default_routing_group_filter: None,
        scheduler_advanced_settings: None,
        low_balance_threshold_cny: 15.0,
        collector_interval_minutes: 30,
        balance_interval_minutes: 5,
        group_rate_interval_minutes: 20,
        model_list_interval_minutes: 60,
        pricing_refresh_interval_minutes: 60,
        collector_timeout_seconds: 15,
        collector_max_concurrency: 3,
        allow_depleted_fallback: false,
        developer_mode_enabled: false,
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
        })
    }

    fn decrypt(
        &self,
        aad: &str,
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
        let mut connection = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .connect()
            .await
            .expect("connect fixture");
        persistence::migrations::migrator()
            .run(&mut connection)
            .await
            .expect("run migrations");
        connection.close().await.expect("close fixture");
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
        PersistenceRuntime::open(&self.path, binary_031())
            .await
            .expect("open runtime")
    }

    async fn seed_endpoint_state(&self, station_id: &str) {
        let mut connection = self.connect().await;
        sqlx::query("INSERT INTO station_keys (id, station_id) VALUES ('key-1', ?1)")
            .bind(station_id)
            .execute(&mut connection)
            .await
            .expect("station key");
        sqlx::query(
            "INSERT INTO station_endpoint_health (station_id, endpoint_revision) VALUES (?1, 1)",
        )
        .bind(station_id)
        .execute(&mut connection)
        .await
        .expect("endpoint health");
        sqlx::query("INSERT INTO station_key_health (station_key_id, endpoint_revision) VALUES ('key-1', 1)")
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
        self.count("station_endpoint_health").await
    }

    async fn station_key_health_rows(&self) -> i64 {
        self.count("station_key_health").await
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

fn binary_031() -> BinaryCompatibility {
    BinaryCompatibility {
        app_version: Version::new(0, 3, 1),
        database_generation: 2,
        readable_schema: 1..=8,
        writable_schema: BTreeSet::from([8]),
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
    let root = std::env::temp_dir().join(format!("relay-pool-persistence-{name}-{id}"));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("clean stale fixture dir");
    }
    std::fs::create_dir_all(&root).expect("fixture dir");
    root.join("relay-pool-v2.sqlite3")
}

#[allow(dead_code)]
fn assert_separate_fixture_files(left: &Path, right: &Path) {
    assert_ne!(
        left, right,
        "V1 and V2 writers must never share one fixture DB"
    );
}
