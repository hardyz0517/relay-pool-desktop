use std::{collections::HashMap, sync::Arc};

use crate::{
    application::{
        clock::Clock,
        error::ApplicationError,
        ids::IdGenerator,
        monitoring::{
            definition_bridge::planning_snapshot_from_config, recorder::BufferedExecution,
            write_path::MonitoringExecutionCommitter,
        },
        pagination::{PageLimit, MAX_PAGE_LIMIT},
    },
    models::{
        channel_monitors::{
            ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun,
            ChannelMonitorRunCursor, ChannelMonitorRunPage, CreateChannelMonitorInput,
            CreateChannelMonitorTemplateInput, UpdateChannelMonitorInput,
            UpdateChannelMonitorTemplateInput,
        },
        monitoring::CancelChannelMonitorExecutionReceipt,
        shared_capabilities::{ChannelMonitorRunsLoadStatus, ChannelMonitorSummary},
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::monitoring::{
            budgets::MonitoringBudgetRepository,
            definitions::{MonitorDefinitionConfigRow, MonitoringDefinitionRepository},
            executions::{ExecutionSummaryRow, MonitoringExecutionRepository},
        },
        stores::monitoring_store::{
            ChannelStatusRunRow, MonitorPatch, MonitorTemplatePatch, MonitoringStore,
            NewMonitorRow, NewMonitorTemplateRow,
        },
    },
};

const DEFAULT_SUMMARY_RUN_LIMIT: usize = 60;

#[derive(Clone)]
pub(crate) struct MonitoringService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    store: MonitoringStore,
    definition_store: MonitoringDefinitionRepository,
    budget_store: MonitoringBudgetRepository,
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
            definition_store: MonitoringDefinitionRepository,
            budget_store: MonitoringBudgetRepository,
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

    pub(crate) async fn cancel_execution(
        &self,
        execution_id: String,
    ) -> Result<CancelChannelMonitorExecutionReceipt, ApplicationError> {
        if execution_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let now_ms = self.now_ms();
        self.runtime
            .write(move |write| {
                Box::pin(async move {
                    let current_status: String = sqlx::query_scalar(
                        "SELECT status FROM channel_monitor_executions WHERE id = ?1",
                    )
                    .bind(&execution_id)
                    .fetch_one(write.connection())
                    .await?;
                    let cancelled = matches!(current_status.as_str(), "queued" | "running");
                    if cancelled {
                        sqlx::query(
                            r#"
                            UPDATE channel_monitor_executions
                            SET status = 'cancelled',
                                finished_at_ms = COALESCE(finished_at_ms, ?1),
                                summary_failure_kind = COALESCE(summary_failure_kind, 'cancelled')
                            WHERE id = ?2
                            "#,
                        )
                        .bind(now_ms)
                        .bind(&execution_id)
                        .execute(write.connection())
                        .await?;
                    }
                    Ok(CancelChannelMonitorExecutionReceipt {
                        execution_id,
                        status: if cancelled {
                            "cancelled".to_string()
                        } else {
                            current_status
                        },
                        cancelled,
                    })
                })
            })
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

    pub(crate) async fn due_monitor_ids_v2(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<String>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.definition_store
            .list_due(read.connection(), self.now_ms(), limit.get())
            .await
            .map(|rows| rows.into_iter().map(|row| row.id).collect())
            .map_err(Into::into)
    }

    pub(crate) async fn next_due_at_ms_v2(&self) -> Result<Option<i64>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.definition_store
            .next_due_at_ms(read.connection())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn load_monitoring_config(
        &self,
        monitor_id: &str,
    ) -> Result<MonitorDefinitionConfigRow, ApplicationError> {
        if monitor_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.definition_store
            .load_config(read.connection(), monitor_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn load_monitoring_planning_snapshot(
        &self,
        monitor_id: &str,
    ) -> Result<super::planner::MonitorPlanningSnapshot, ApplicationError> {
        planning_snapshot_from_config(self.load_monitoring_config(monitor_id).await?)
    }

    pub(crate) async fn commit_monitoring_execution(
        &self,
        execution: BufferedExecution,
    ) -> Result<ExecutionSummaryRow, ApplicationError> {
        let committer = MonitoringExecutionCommitter::new();
        self.runtime
            .write(|write| {
                Box::pin(async move { committer.commit(write.connection(), &execution).await })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn recover_startup_interrupted_monitoring_executions(
        &self,
    ) -> Result<i64, ApplicationError> {
        let executions = MonitoringExecutionRepository;
        let now_ms = self.now_ms();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    executions
                        .mark_startup_recovery_interrupted(write.connection(), now_ms)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn reserve_monitoring_probe_budget(
        &self,
        monitor_id: &str,
        station_key_id: &str,
        amount: i64,
        limit: i64,
    ) -> Result<bool, ApplicationError> {
        if monitor_id.trim().is_empty()
            || station_key_id.trim().is_empty()
            || amount <= 0
            || limit <= 0
        {
            return Err(ApplicationError::ConstraintViolation);
        }
        let store = self.budget_store;
        let id = self.ids.next_id();
        let now_ms = self.now_ms();
        let (window_start_ms, window_end_ms) = utc_day_window_ms(now_ms);
        let monitor_id = monitor_id.to_string();
        let station_key_id = station_key_id.to_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .reserve_attempts(
                            write.connection(),
                            &id,
                            &monitor_id,
                            Some(&station_key_id),
                            window_start_ms,
                            window_end_ms,
                            amount,
                            limit,
                            now_ms,
                        )
                        .await
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

fn utc_day_window_ms(now_ms: i64) -> (i64, i64) {
    const DAY_MS: i64 = 86_400_000;
    let start = now_ms.div_euclid(DAY_MS).saturating_mul(DAY_MS);
    (start, start.saturating_add(DAY_MS))
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
    use chrono::{TimeZone, Utc};

    use super::*;

    struct FixedClock;

    impl Clock for FixedClock {
        fn now_utc(&self) -> chrono::DateTime<Utc> {
            Utc.timestamp_millis_opt(2_000).single().expect("timestamp")
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
