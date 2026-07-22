use std::{collections::HashMap, sync::Arc};

use crate::{
    application::{
        clock::Clock,
        error::ApplicationError,
        ids::IdGenerator,
        pagination::{PageLimit, MAX_PAGE_LIMIT},
    },
    models::{
        channel_monitors::{
            ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun,
            ChannelMonitorRunCursor, ChannelMonitorRunPage, CompletedMonitorProbe,
            CreateChannelMonitorInput, CreateChannelMonitorTemplateInput,
            UpdateChannelMonitorInput, UpdateChannelMonitorTemplateInput,
        },
        shared_capabilities::{ChannelMonitorRunsLoadStatus, ChannelMonitorSummary},
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::monitoring_store::{
            ChannelStatusRunRow, MonitorPatch, MonitorTemplatePatch, MonitoringStore,
            NewMonitorRow, NewMonitorRunRow, NewMonitorTemplateRow,
        },
        stores::request_log_store::{CompletedMonitorRequestWrite, RequestLogStore},
    },
};

const DEFAULT_SUMMARY_RUN_LIMIT: usize = 60;

#[derive(Clone)]
pub(crate) struct MonitoringService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    store: MonitoringStore,
}

impl MonitoringService {
    pub(crate) fn new(
        runtime: PersistenceHandle,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            runtime,
            clock,
            ids,
            store: MonitoringStore,
        }
    }

    pub(crate) async fn list_templates(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ChannelMonitorRequestTemplate>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_templates(&mut read, limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn get_template(
        &self,
        id: &str,
    ) -> Result<ChannelMonitorRequestTemplate, ApplicationError> {
        if id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.store
            .get_template(&mut read, id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn create_template(
        &self,
        input: CreateChannelMonitorTemplateInput,
    ) -> Result<ChannelMonitorRequestTemplate, ApplicationError> {
        let store = self.store;
        let row = NewMonitorTemplateRow {
            id: self.ids.next_id(),
            now: self.now_ms_string(),
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.insert_template(write, row).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_template(
        &self,
        input: UpdateChannelMonitorTemplateInput,
    ) -> Result<ChannelMonitorRequestTemplate, ApplicationError> {
        let store = self.store;
        let patch = MonitorTemplatePatch {
            now: self.now_ms_string(),
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.update_template(write, patch).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn duplicate_template(
        &self,
        id: String,
    ) -> Result<ChannelMonitorRequestTemplate, ApplicationError> {
        let source = self.get_template(&id).await?;
        self.create_template(CreateChannelMonitorTemplateInput {
            name: format!("{} Copy", source.name),
            endpoint_kind: source.endpoint_kind,
            method: source.method,
            path: source.path,
            request_body_json: source.request_body_json,
            enabled: source.enabled,
            note: source.note,
        })
        .await
    }

    pub(crate) async fn delete_template(&self, id: String) -> Result<(), ApplicationError> {
        if id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let store = self.store;
        self.runtime
            .write(|write| Box::pin(async move { store.delete_template(write, &id).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_monitors(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ChannelMonitor>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_monitors(&mut read, limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_channel_monitor_summaries(
        &self,
        run_since: Option<&str>,
        run_limit: Option<usize>,
    ) -> Result<Vec<ChannelMonitorSummary>, ApplicationError> {
        let run_since_ms = parse_summary_run_since(run_since)?;
        let run_limit = summary_run_limit(run_limit)?;
        let mut read = self.runtime.begin_read().await?;
        let monitors = self.store.list_monitors(&mut read, MAX_PAGE_LIMIT).await?;
        let runs = self
            .store
            .summary_runs(&mut read, run_since_ms, MAX_PAGE_LIMIT, run_limit)
            .await
            .ok();
        Ok(build_monitor_summaries(monitors, runs))
    }

    pub(crate) async fn get_monitor(&self, id: &str) -> Result<ChannelMonitor, ApplicationError> {
        if id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.store
            .get_monitor(&mut read, id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn create_monitor(
        &self,
        input: CreateChannelMonitorInput,
    ) -> Result<ChannelMonitor, ApplicationError> {
        let store = self.store;
        let next_run_at = input.enabled.then(|| self.now_ms_string());
        let row = NewMonitorRow {
            id: self.ids.next_id(),
            now: self.now_ms_string(),
            next_run_at,
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.insert_monitor(write, row).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_monitor(
        &self,
        input: UpdateChannelMonitorInput,
    ) -> Result<ChannelMonitor, ApplicationError> {
        let store = self.store;
        let patch = MonitorPatch {
            now: self.now_ms_string(),
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.update_monitor(write, patch).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn delete_monitor(&self, id: String) -> Result<(), ApplicationError> {
        if id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let store = self.store;
        self.runtime
            .write(|write| Box::pin(async move { store.delete_monitor(write, &id).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn due_monitors(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ChannelMonitor>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .due_monitors(&mut read, self.now_ms(), limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn record_probe_outcome(
        &self,
        outcome: CompletedMonitorProbe,
    ) -> Result<ChannelMonitorRun, ApplicationError> {
        validate_probe_outcome(&outcome)?;
        let store = self.store;
        let request_logs = RequestLogStore;
        let now_ms = self.now_ms();
        let now = now_ms.to_string();
        let id = self.ids.next_id();
        let request_record = completed_monitor_request_write(&id, &outcome);
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    let (interval_seconds, jitter_seconds) = store
                        .monitor_schedule(write, &outcome.run.monitor_id)
                        .await?;
                    let next_run_at = next_run_at(
                        &outcome.run.monitor_id,
                        now_ms,
                        interval_seconds,
                        jitter_seconds,
                    )
                    .to_string();
                    let run = store
                        .insert_run_and_advance_monitor(
                            write,
                            NewMonitorRunRow {
                                id: id.clone(),
                                now: now.clone(),
                                next_run_at,
                                input: outcome.run.clone(),
                            },
                        )
                        .await?;
                    if let Some(request_record) = request_record.as_ref() {
                        request_logs
                            .insert_completed_monitor_observation(write, request_record, &now)
                            .await?;
                    }
                    Ok(run)
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_run_page(
        &self,
        monitor_id: &str,
        cursor: Option<&ChannelMonitorRunCursor>,
        limit: PageLimit,
    ) -> Result<ChannelMonitorRunPage, ApplicationError> {
        if monitor_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_run_page(&mut read, monitor_id, cursor, limit.get())
            .await
            .map_err(Into::into)
    }

    fn now_ms(&self) -> i64 {
        self.clock.now_utc().timestamp_millis()
    }

    fn now_ms_string(&self) -> String {
        self.now_ms().to_string()
    }
}

fn validate_probe_outcome(outcome: &CompletedMonitorProbe) -> Result<(), ApplicationError> {
    let Some(request) = outcome.request.as_ref() else {
        return Ok(());
    };
    let run = &outcome.run;
    let identity_matches = run.station_id == request.station_id
        && run.station_key_id.as_deref() == Some(request.station_key_id.as_str())
        && run.response_model.as_deref() == Some(request.model.as_str());
    let usage_matches = request.usage.as_ref().is_none_or(|usage| {
        let estimate = &request.pricing.estimate;
        usage.prompt_tokens == estimate.prompt_tokens
            && usage.completion_tokens == estimate.completion_tokens
            && usage.total_tokens == estimate.total_tokens
            && usage.cache_creation_tokens == estimate.cache_creation_tokens
            && usage.cache_read_tokens == estimate.cache_read_tokens
    });
    if !identity_matches || !usage_matches {
        return Err(ApplicationError::ConstraintViolation);
    }
    Ok(())
}

fn completed_monitor_request_write(
    request_id: &str,
    outcome: &CompletedMonitorProbe,
) -> Option<CompletedMonitorRequestWrite> {
    let request = outcome.request.as_ref()?;
    let run = &outcome.run;
    Some(CompletedMonitorRequestWrite {
        request_id: request_id.to_string(),
        started_at: run.started_at.clone(),
        finished_at: run.finished_at.clone(),
        duration_ms: run.duration_ms,
        method: request.method.clone(),
        path: request.path.clone(),
        endpoint: request.endpoint.clone(),
        model: request.model.clone(),
        stream: request.stream,
        status: run.status.clone(),
        station_key_id: request.station_key_id.clone(),
        station_id: request.station_id.clone(),
        upstream_base_url: request.upstream_base_url.clone(),
        reasoning_effort: request.reasoning_effort.clone(),
        first_token_ms: request.first_token_ms,
        pricing: request.pricing.estimate.clone(),
        group_binding_id: request.pricing.group_binding_id.clone(),
        normalization_status: request.pricing.normalization_status.clone(),
    })
}

fn parse_summary_run_since(run_since: Option<&str>) -> Result<Option<i64>, ApplicationError> {
    run_since
        .map(|value| {
            value
                .trim()
                .parse::<i64>()
                .ok()
                .filter(|timestamp| *timestamp > 0)
                .ok_or(ApplicationError::ConstraintViolation)
        })
        .transpose()
}

fn summary_run_limit(run_limit: Option<usize>) -> Result<u32, ApplicationError> {
    let value = run_limit.unwrap_or(DEFAULT_SUMMARY_RUN_LIMIT);
    let value = u32::try_from(value).map_err(|_| ApplicationError::ConstraintViolation)?;
    PageLimit::new(value).map(PageLimit::get)
}

fn build_monitor_summaries(
    monitors: Vec<ChannelMonitor>,
    runs: Option<Vec<ChannelStatusRunRow>>,
) -> Vec<ChannelMonitorSummary> {
    let Some(runs) = runs else {
        return monitors
            .into_iter()
            .map(|monitor| ChannelMonitorSummary {
                monitor,
                recent_runs: Vec::new(),
                runs_load_status: ChannelMonitorRunsLoadStatus::Failed,
                latest_run: None,
            })
            .collect();
    };
    let mut runs_by_monitor = HashMap::<String, Vec<ChannelMonitorRun>>::new();
    for row in runs {
        runs_by_monitor
            .entry(row.monitor_id)
            .or_default()
            .push(row.run);
    }
    monitors
        .into_iter()
        .map(|monitor| {
            let recent_runs = runs_by_monitor.remove(&monitor.id).unwrap_or_default();
            let latest_run = recent_runs.first().cloned();
            ChannelMonitorSummary {
                monitor,
                recent_runs,
                runs_load_status: ChannelMonitorRunsLoadStatus::Ok,
                latest_run,
            }
        })
        .collect()
}

fn next_run_at(monitor_id: &str, now_ms: i64, interval_seconds: i64, jitter_seconds: i64) -> i64 {
    let jitter_range_ms = u64::try_from(jitter_seconds.max(0))
        .unwrap_or_default()
        .saturating_mul(1_000)
        .saturating_add(1);
    let jitter_ms = if jitter_seconds <= 0 {
        0
    } else {
        stable_schedule_hash(monitor_id, now_ms) % jitter_range_ms
    };
    now_ms
        .saturating_add(interval_seconds.max(1).saturating_mul(1_000))
        .saturating_add(i64::try_from(jitter_ms).unwrap_or(i64::MAX))
}

fn stable_schedule_hash(monitor_id: &str, now_ms: i64) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    monitor_id
        .as_bytes()
        .iter()
        .chain(now_ms.to_le_bytes().iter())
        .fold(FNV_OFFSET, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
        })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::{TimeZone, Utc};
    use sqlx::Row;

    use crate::{
        models::{
            channel_monitors::{
                CompletedMonitorRequestEvidence, MonitorProbeUsageEvidence,
                MonitorRequestPricingEvidence,
            },
            pricing::RequestCostEstimate,
        },
        persistence::runtime::PersistenceRuntime,
    };

    use super::*;

    struct FixedClock;

    impl Clock for FixedClock {
        fn now_utc(&self) -> chrono::DateTime<Utc> {
            Utc.timestamp_millis_opt(2_000).single().expect("timestamp")
        }
    }

    struct FixedIds(AtomicUsize);

    impl IdGenerator for FixedIds {
        fn next_id(&self) -> String {
            format!("observation-{}", self.0.fetch_add(1, Ordering::Relaxed) + 1)
        }
    }

    #[test]
    fn jitter_is_deterministic_and_bounded() {
        let first = next_run_at("monitor-a", 1_000, 30, 5);
        let second = next_run_at("monitor-a", 1_000, 30, 5);
        assert_eq!(first, second);
        assert!((31_000..=36_000).contains(&first));
    }

    #[test]
    fn summary_query_validation_is_strict_and_bounded() {
        assert_eq!(summary_run_limit(None).expect("default limit"), 60);
        assert_eq!(summary_run_limit(Some(1)).expect("minimum limit"), 1);
        assert_eq!(summary_run_limit(Some(500)).expect("maximum limit"), 500);
        assert!(summary_run_limit(Some(0)).is_err());
        assert!(summary_run_limit(Some(501)).is_err());
        assert_eq!(
            parse_summary_run_since(Some(" 1234 ")).expect("valid timestamp"),
            Some(1234)
        );
        assert_eq!(
            parse_summary_run_since(None).expect("optional timestamp"),
            None
        );
        assert!(parse_summary_run_since(Some("")).is_err());
        assert!(parse_summary_run_since(Some("0")).is_err());
        assert!(parse_summary_run_since(Some("not-a-timestamp")).is_err());
    }

    #[test]
    fn summary_mapping_preserves_latest_run_and_failure_status() {
        let monitor_a = monitor("monitor-a");
        let monitor_b = monitor("monitor-b");
        let newest = run("run-newest", "monitor-a", "2000");
        let older = run("run-older", "monitor-a", "1000");
        let summaries = build_monitor_summaries(
            vec![monitor_a.clone(), monitor_b.clone()],
            Some(vec![
                ChannelStatusRunRow {
                    monitor_id: monitor_a.id.clone(),
                    run: newest.clone(),
                },
                ChannelStatusRunRow {
                    monitor_id: monitor_a.id.clone(),
                    run: older,
                },
            ]),
        );

        assert_eq!(summaries.len(), 2);
        assert_eq!(
            summaries[0].latest_run.as_ref().map(|run| &run.id),
            Some(&newest.id)
        );
        assert_eq!(summaries[0].recent_runs.len(), 2);
        assert_eq!(
            summaries[0].runs_load_status,
            ChannelMonitorRunsLoadStatus::Ok
        );
        assert!(summaries[1].recent_runs.is_empty());
        assert_eq!(
            summaries[1].runs_load_status,
            ChannelMonitorRunsLoadStatus::Ok
        );

        let failed = build_monitor_summaries(vec![monitor_a, monitor_b], None);
        assert!(failed.iter().all(|summary| {
            summary.runs_load_status == ChannelMonitorRunsLoadStatus::Failed
                && summary.recent_runs.is_empty()
                && summary.latest_run.is_none()
        }));
    }

    #[tokio::test]
    async fn monitor_run_and_request_log_commit_atomically_with_usage_evidence() {
        let runtime = test_runtime().await;
        seed_monitor(&runtime).await;
        let service = MonitoringService::new(
            runtime.handle(),
            Arc::new(FixedClock),
            Arc::new(FixedIds(AtomicUsize::new(0))),
        );

        service
            .record_probe_outcome(completed_probe())
            .await
            .expect("record monitor observation");

        let mut read = runtime.begin_read().await.expect("read");
        let row = sqlx::query(
            "SELECT r.id AS run_id, l.request_id, l.prompt_tokens, l.total_tokens,
                    l.cache_read_tokens, l.stream, l.reasoning_effort, l.first_token_ms,
                    l.estimated_total_cost, l.route_policy, l.error_message,
                    l.economic_context_json
             FROM channel_monitor_runs r
             JOIN request_logs l ON l.request_id = r.id",
        )
        .fetch_one(read.connection())
        .await
        .expect("joined observation");
        assert_eq!(row.get::<String, _>("run_id"), "observation-1");
        assert_eq!(row.get::<String, _>("request_id"), "observation-1");
        assert_eq!(row.get::<i64, _>("prompt_tokens"), 4);
        assert_eq!(row.get::<i64, _>("total_tokens"), 10);
        assert_eq!(row.get::<i64, _>("cache_read_tokens"), 2);
        assert_eq!(row.get::<i64, _>("stream"), 1);
        assert_eq!(row.get::<String, _>("reasoning_effort"), "minimal");
        assert_eq!(row.get::<i64, _>("first_token_ms"), 7);
        assert_eq!(row.get::<f64, _>("estimated_total_cost"), 0.000016);
        assert_eq!(row.get::<String, _>("route_policy"), "channel_monitor");
        assert_eq!(row.get::<Option<String>, _>("error_message"), None);
        assert_eq!(row.get::<Option<String>, _>("economic_context_json"), None);
        drop(read);

        let mut write = runtime.begin_write().await.expect("conflict seed write");
        sqlx::query(
            "INSERT INTO request_logs (
                id, request_id, started_at, method, path, endpoint, status, created_at
             ) VALUES ('observation-2', 'observation-2', '1', 'POST', '/conflict',
                       'responses', 'success', '1')",
        )
        .execute(write.connection())
        .await
        .expect("conflicting request log");
        write.commit().await.expect("conflict seed commit");

        let failed = service.record_probe_outcome(completed_probe()).await;
        assert!(failed.is_err());
        let mut read = runtime.begin_read().await.expect("read after rollback");
        let run_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM channel_monitor_runs")
            .fetch_one(read.connection())
            .await
            .expect("run count");
        assert_eq!(
            run_count, 1,
            "failed request-log insert must roll back its run"
        );
    }

    async fn test_runtime() -> PersistenceRuntime {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("monitoring.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("runtime");
        std::mem::forget(root);
        runtime
    }

    async fn seed_monitor(runtime: &PersistenceRuntime) {
        let mut write = runtime.begin_write().await.expect("seed write");
        sqlx::query(
            "INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, enabled, priority,
                credit_per_cny, collection_interval_minutes, status, created_at, updated_at
             ) VALUES ('station-1', 'Station', 'openai-compatible',
                       'https://example.test', 'https://example.test/v1', 1, 0,
                       1.0, 30, 'unchecked', '1', '1')",
        )
        .execute(write.connection())
        .await
        .expect("station");
        sqlx::query("INSERT INTO station_keys (id, station_id) VALUES ('key-1', 'station-1')")
            .execute(write.connection())
            .await
            .expect("station key");
        sqlx::query(
            "INSERT INTO channel_monitor_request_templates (
                id, name, endpoint_kind, method, path, request_body_json,
                enabled, built_in, created_at, updated_at
             ) VALUES ('template-1', 'Responses', 'responses', 'POST',
                       '/v1/responses', '{}', 1, 0, '1', '1')",
        )
        .execute(write.connection())
        .await
        .expect("template");
        sqlx::query(
            "INSERT INTO channel_monitors (
                id, name, target_type, station_id, station_key_id, template_id,
                enabled, interval_seconds, jitter_seconds, timeout_seconds,
                max_concurrency, consecutive_failure_threshold, fallback_models_json,
                next_run_at, created_at, updated_at
             ) VALUES ('monitor-1', 'Primary', 'station_key', 'station-1', 'key-1',
                       'template-1', 1, 30, 0, 15, 1, 3, '[]', '1000', '1', '1')",
        )
        .execute(write.connection())
        .await
        .expect("monitor");
        write.commit().await.expect("seed commit");
    }

    fn completed_probe() -> CompletedMonitorProbe {
        CompletedMonitorProbe {
            run: crate::models::channel_monitors::CreateChannelMonitorRunInput {
                monitor_id: "monitor-1".to_string(),
                template_id: "template-1".to_string(),
                station_id: "station-1".to_string(),
                station_key_id: Some("key-1".to_string()),
                status: "success".to_string(),
                started_at: "1000".to_string(),
                finished_at: Some("1100".to_string()),
                duration_ms: Some(100),
                http_status: Some(200),
                latency_ms: Some(42),
                response_model: Some("gpt-test".to_string()),
                fallback_model: None,
                error_message: None,
            },
            request: Some(CompletedMonitorRequestEvidence {
                method: "POST".to_string(),
                path: "/v1/responses".to_string(),
                endpoint: "responses".to_string(),
                model: "gpt-test".to_string(),
                stream: true,
                reasoning_effort: Some("minimal".to_string()),
                station_key_id: "key-1".to_string(),
                station_id: "station-1".to_string(),
                upstream_base_url: "https://example.test/v1".to_string(),
                first_token_ms: Some(7),
                usage: Some(MonitorProbeUsageEvidence {
                    prompt_tokens: Some(4),
                    completion_tokens: Some(6),
                    total_tokens: Some(10),
                    cache_creation_tokens: None,
                    cache_read_tokens: Some(2),
                }),
                pricing: MonitorRequestPricingEvidence {
                    estimate: RequestCostEstimate {
                        prompt_tokens: Some(4),
                        completion_tokens: Some(6),
                        total_tokens: Some(10),
                        cache_creation_tokens: None,
                        cache_read_tokens: Some(2),
                        billing_mode: Some("token".to_string()),
                        estimated_input_cost: Some(0.000004),
                        estimated_output_cost: Some(0.000012),
                        estimated_total_cost: Some(0.000016),
                        base_input_cost: Some(0.000004),
                        base_output_cost: Some(0.000012),
                        base_fixed_cost: None,
                        base_total_cost: Some(0.000016),
                        cost_currency: Some("USD".to_string()),
                        pricing_rule_id: None,
                        pricing_source: Some("manual".to_string()),
                        cost_status: "priced".to_string(),
                    },
                    group_binding_id: None,
                    normalization_status: Some("complete".to_string()),
                },
            }),
        }
    }

    fn monitor(id: &str) -> ChannelMonitor {
        ChannelMonitor {
            id: id.into(),
            name: id.into(),
            target_type: "station".into(),
            station_id: "station-a".into(),
            station_key_id: None,
            template_id: "template-a".into(),
            enabled: true,
            interval_seconds: 60,
            jitter_seconds: 0,
            timeout_seconds: 30,
            max_concurrency: 1,
            consecutive_failure_threshold: 3,
            fallback_models: Vec::new(),
            note: None,
            created_at: "1000".into(),
            updated_at: "1000".into(),
        }
    }

    fn run(id: &str, monitor_id: &str, started_at: &str) -> ChannelMonitorRun {
        ChannelMonitorRun {
            id: id.into(),
            monitor_id: monitor_id.into(),
            template_id: "template-a".into(),
            station_id: "station-a".into(),
            station_key_id: None,
            status: "success".into(),
            started_at: started_at.into(),
            finished_at: Some(started_at.into()),
            duration_ms: Some(1),
            http_status: Some(200),
            latency_ms: Some(1),
            response_model: None,
            fallback_model: None,
            error_message: None,
            created_at: started_at.into(),
        }
    }
}
