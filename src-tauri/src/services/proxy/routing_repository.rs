use std::{collections::HashSet, sync::Arc};

use crate::{
    models::proxy::{CreateRequestLogInput, RequestLog},
    services::{
        database::{now_millis_for_services, AppDatabase},
        proxy::router::RichRouteCandidate,
    },
};

#[derive(Clone)]
pub(crate) struct SqliteRoutingRepository {
    database: AppDatabase,
    data_key: [u8; 32],
    finalized_request_ids: Arc<std::sync::Mutex<HashSet<String>>>,
}

impl SqliteRoutingRepository {
    pub(crate) fn new(database: AppDatabase, data_key: [u8; 32]) -> Self {
        Self {
            database,
            data_key,
            finalized_request_ids: Arc::new(std::sync::Mutex::new(HashSet::new())),
        }
    }
}

pub(crate) trait RoutingRepository: Send + Sync {
    fn load_runtime_candidates(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<RichRouteCandidate>, String>>;

    fn record_final_outcome(
        &self,
        outcome: FinalRequestOutcome,
    ) -> futures_util::future::BoxFuture<'static, Result<Option<RequestLog>, String>>;
}

impl RoutingRepository for SqliteRoutingRepository {
    fn load_runtime_candidates(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<RichRouteCandidate>, String>> {
        let database = self.database.clone();
        let data_key = self.data_key;
        Box::pin(async move {
            tauri::async_runtime::spawn_blocking(move || {
                database.proxy_rich_route_candidates_with_data_key(&data_key)
            })
            .await
            .map_err(|error| format!("routing repository candidate task failed: {error}"))?
        })
    }

    fn record_final_outcome(
        &self,
        outcome: FinalRequestOutcome,
    ) -> futures_util::future::BoxFuture<'static, Result<Option<RequestLog>, String>> {
        let database = self.database.clone();
        let finalized = Arc::clone(&self.finalized_request_ids);
        Box::pin(async move {
            tauri::async_runtime::spawn_blocking(move || {
                let mut finalized = finalized
                    .lock()
                    .map_err(|_| "final outcome guard lock poisoned".to_string())?;
                if finalized.contains(&outcome.request_id) {
                    return Ok(None);
                }

                if let Some(feedback) = outcome.feedback.as_ref() {
                    match feedback.kind {
                        CandidateFeedbackKind::Success => database
                            .record_station_key_success_for_endpoint_revision(
                                &feedback.station_key_id,
                                &feedback.station_id,
                                feedback.station_endpoint_revision,
                                outcome.duration_ms.unwrap_or_default().max(0),
                                outcome.finished_at.as_str(),
                            ),
                        CandidateFeedbackKind::Failure => database
                            .record_station_key_failure_for_endpoint_revision(
                                &feedback.station_key_id,
                                &feedback.station_id,
                                feedback.station_endpoint_revision,
                                outcome
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("upstream request failed"),
                                outcome.finished_at.as_str(),
                            ),
                    }?;
                }

                let request_id = outcome.request_id.clone();
                let log = database.insert_request_log(outcome.into_request_log_input())?;
                finalized.insert(request_id);
                Ok(Some(log))
            })
            .await
            .map_err(|error| format!("routing repository final outcome task failed: {error}"))?
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinalRequestOutcome {
    pub request_id: String,
    pub method: String,
    pub path: String,
    pub model: Option<String>,
    pub stream: bool,
    pub status: String,
    pub lifecycle_status: Option<String>,
    pub selected_station_key_id: Option<String>,
    pub selected_station_id: Option<String>,
    pub upstream_base_url: Option<String>,
    pub fallback_count: i64,
    pub error_message: Option<String>,
    pub route_policy: Option<String>,
    pub route_reason: Option<String>,
    pub rejected_candidates_json: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: Option<i64>,
    pub feedback: Option<CandidateFeedback>,
}

impl FinalRequestOutcome {
    pub(crate) fn success(status: impl Into<String>) -> Self {
        let now = now_millis_for_services().to_string();
        Self {
            request_id: format!("outcome_{now}"),
            method: "GET".to_string(),
            path: "/v1/models".to_string(),
            model: None,
            stream: false,
            status: status.into(),
            lifecycle_status: None,
            selected_station_key_id: None,
            selected_station_id: None,
            upstream_base_url: None,
            fallback_count: 0,
            error_message: None,
            route_policy: None,
            route_reason: None,
            rejected_candidates_json: None,
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            started_at: now.clone(),
            finished_at: now,
            duration_ms: None,
            feedback: None,
        }
    }

    fn into_request_log_input(self) -> CreateRequestLogInput {
        CreateRequestLogInput {
            method: self.method,
            path: self.path,
            model: self.model,
            stream: self.stream,
            status: self.status,
            lifecycle_status: self.lifecycle_status,
            station_key_id: self.selected_station_key_id,
            station_id: self.selected_station_id,
            upstream_base_url: self.upstream_base_url,
            fallback_count: self.fallback_count,
            error_message: self.error_message,
            route_policy: self.route_policy,
            route_reason: self.route_reason,
            rejected_candidates_json: self.rejected_candidates_json,
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            total_tokens: self.total_tokens,
            cache_creation_tokens: None,
            cache_read_tokens: None,
            reasoning_effort: None,
            first_token_ms: None,
            billing_mode: None,
            estimated_input_cost: None,
            estimated_output_cost: None,
            estimated_total_cost: None,
            base_input_cost: None,
            base_output_cost: None,
            base_fixed_cost: None,
            base_total_cost: None,
            cost_currency: None,
            pricing_rule_id: None,
            pricing_source: None,
            cost_status: None,
            group_binding_id: None,
            normalization_status: None,
            balance_scope: None,
            economic_context_json: None,
            started_at: self.started_at,
            finished_at: Some(self.finished_at),
            duration_ms: self.duration_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateFeedback {
    pub station_key_id: String,
    pub station_id: String,
    pub station_endpoint_revision: i64,
    pub kind: CandidateFeedbackKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateFeedbackKind {
    Success,
    Failure,
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, time::Duration};

    use crate::{
        models::{
            station_keys::CreateStationKeyInput,
            stations::{CreateStationInput, UpdateStationInput},
        },
        services::{database::AppDatabase, secrets::crypto::generate_data_key},
    };

    use super::*;

    #[tokio::test]
    async fn repository_loads_runtime_candidates_without_blocking_async_callers() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        seed_candidate(&database, &data_key, "async-load");
        let release = hold_database_connection(&database);
        let repository = SqliteRoutingRepository::new(database, data_key);

        let load = tokio::spawn(async move { repository.load_runtime_candidates().await });
        tokio::time::timeout(Duration::from_millis(100), async {
            tokio::task::yield_now().await;
        })
        .await
        .expect("runtime remains available while SQLite waits in blocking pool");

        release.send(()).expect("release DB lock");
        let candidates = load.await.expect("join load").expect("candidates");

        assert!(candidates
            .iter()
            .any(|candidate| candidate.candidate.api_key == "sk-async-load"));
    }

    #[tokio::test]
    async fn repository_records_one_final_outcome_for_endpoint_revision() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let seeded = seed_candidate(&database, &data_key, "final");
        let repository = SqliteRoutingRepository::new(database.clone(), data_key);
        let outcome = success_outcome(
            "req-final-1",
            &seeded.station_id,
            &seeded.station_key_id,
            seeded.station_endpoint_revision,
        );

        let first = repository
            .record_final_outcome(outcome.clone())
            .await
            .expect("first outcome");
        let second = repository
            .record_final_outcome(outcome)
            .await
            .expect("duplicate outcome");

        assert!(first.is_some());
        assert!(second.is_none());
        let logs = database.list_local_proxy_request_logs().expect("logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, "success");
        let health = database
            .get_station_key_health(seeded.station_key_id)
            .expect("health");
        assert_eq!(health.success_count, 1);
    }

    #[tokio::test]
    async fn repository_rejects_stale_final_outcome_endpoint_revision() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let seeded = seed_candidate(&database, &data_key, "stale-final");
        change_station_endpoint(&database, &seeded.station_id);
        let repository = SqliteRoutingRepository::new(database.clone(), data_key);

        let error = repository
            .record_final_outcome(success_outcome(
                "req-stale-final",
                &seeded.station_id,
                &seeded.station_key_id,
                seeded.station_endpoint_revision,
            ))
            .await
            .expect_err("stale endpoint revision rejects final outcome");

        assert_eq!(error, "station_endpoint_revision_changed");
        assert!(database
            .list_local_proxy_request_logs()
            .expect("logs")
            .is_empty());
    }

    struct SeededCandidate {
        station_id: String,
        station_key_id: String,
        station_endpoint_revision: i64,
    }

    fn seed_candidate(
        database: &AppDatabase,
        data_key: &[u8; 32],
        suffix: &str,
    ) -> SeededCandidate {
        let station = database
            .create_station(CreateStationInput {
                name: format!("Station {suffix}"),
                station_type: "openai-compatible".to_string(),
                website_url: "https://example.test".to_string(),
                api_base_url: format!("https://{suffix}.example.test/v1"),
                api_key: "sk-station".to_string(),
                collector_proxy_mode: "direct".to_string(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        let key = database
            .create_station_key_with_data_key(
                CreateStationKeyInput {
                    station_id: station.id.clone(),
                    name: format!("Key {suffix}"),
                    api_key: format!("sk-{suffix}"),
                    enabled: true,
                    priority: Some(0),
                    max_concurrency: Some(0),
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
                },
                data_key,
            )
            .expect("station key");
        SeededCandidate {
            station_id: station.id,
            station_key_id: key.id,
            station_endpoint_revision: station.endpoint_revision,
        }
    }

    fn success_outcome(
        request_id: &str,
        station_id: &str,
        station_key_id: &str,
        station_endpoint_revision: i64,
    ) -> FinalRequestOutcome {
        FinalRequestOutcome {
            request_id: request_id.to_string(),
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            model: Some("gpt-test".to_string()),
            stream: false,
            status: "success".to_string(),
            lifecycle_status: Some("buffered_success".to_string()),
            selected_station_key_id: Some(station_key_id.to_string()),
            selected_station_id: Some(station_id.to_string()),
            upstream_base_url: Some("https://example.test/v1".to_string()),
            fallback_count: 0,
            error_message: None,
            route_policy: Some("priority_fallback".to_string()),
            route_reason: Some("selected first healthy key".to_string()),
            rejected_candidates_json: Some("[]".to_string()),
            prompt_tokens: Some(1),
            completion_tokens: Some(2),
            total_tokens: Some(3),
            started_at: "1000".to_string(),
            finished_at: "1045".to_string(),
            duration_ms: Some(45),
            feedback: Some(CandidateFeedback {
                station_key_id: station_key_id.to_string(),
                station_id: station_id.to_string(),
                station_endpoint_revision,
                kind: CandidateFeedbackKind::Success,
            }),
        }
    }

    fn hold_database_connection(database: &AppDatabase) -> mpsc::Sender<()> {
        let database = database.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _guard = database
                .connection_for_repository_tests()
                .expect("connection");
            started_tx.send(()).expect("signal lock");
            release_rx.recv().expect("release lock");
        });
        started_rx.recv().expect("wait for lock");
        release_tx
    }

    fn change_station_endpoint(database: &AppDatabase, station_id: &str) {
        let station = database
            .list_stations()
            .expect("stations")
            .into_iter()
            .find(|station| station.id == station_id)
            .expect("station");
        database
            .update_station(UpdateStationInput {
                id: station.id,
                name: station.name,
                station_type: station.station_type,
                website_url: "https://changed.example.test".to_string(),
                api_base_url: "https://changed.example.test/v1".to_string(),
                api_key: None,
                collector_proxy_mode: station.collector_proxy_mode,
                collector_proxy_url: station.collector_proxy_url,
                enabled: station.enabled,
                credit_per_cny: station.credit_per_cny,
                low_balance_threshold_cny: station.low_balance_threshold_cny,
                collection_interval_minutes: station.collection_interval_minutes,
                note: station.note,
            })
            .expect("change station endpoint");
    }
}
