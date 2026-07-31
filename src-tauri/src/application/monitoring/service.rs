use std::sync::Arc;

use crate::{
    application::{
        clock::Clock,
        error::ApplicationError,
        ids::IdGenerator,
        monitoring::{
            definition_bridge::planning_snapshot_from_config, planner::ProbePlan,
            recorder::BufferedExecution, write_path::MonitoringExecutionCommitter,
        },
        pagination::PageLimit,
    },
    models::{
        channel_monitors::{
            ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRunCursor,
            ChannelMonitorRunPage, CreateChannelMonitorInput, CreateChannelMonitorTemplateInput,
            UpdateChannelMonitorInput, UpdateChannelMonitorTemplateInput,
        },
        monitoring::CancelChannelMonitorExecutionReceipt,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::legacy_monitor_run_store::LegacyMonitorRunReader,
        stores::monitoring::{
            budgets::MonitoringBudgetRepository,
            definitions::{MonitorDefinitionConfigRow, MonitoringDefinitionRepository},
            executions::{ExecutionSummaryRow, MonitoringExecutionRepository, NewExecutionRow},
            retention::MonitoringRetentionRepository,
        },
        stores::monitoring_store::{
            MonitorPatch, MonitorTemplatePatch, MonitoringStore, NewMonitorRow,
            NewMonitorTemplateRow,
        },
    },
};

#[derive(Clone)]
pub(crate) struct MonitoringService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    store: MonitoringStore,
    definition_store: MonitoringDefinitionRepository,
    budget_store: MonitoringBudgetRepository,
    legacy_run_reader: LegacyMonitorRunReader,
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
            legacy_run_reader: LegacyMonitorRunReader,
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
        let executions = MonitoringExecutionRepository;
        self.runtime
            .write(move |write| {
                Box::pin(async move {
                    executions
                        .cancel_execution(write.connection(), &execution_id, now_ms)
                        .await
                        .map(|row| CancelChannelMonitorExecutionReceipt {
                            execution_id: row.execution_id,
                            status: row.status,
                            cancelled: row.cancelled,
                        })
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn find_execution_by_trigger_request_id(
        &self,
        trigger_request_id: &str,
    ) -> Result<Option<(String, String, String)>, ApplicationError> {
        if trigger_request_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.execution_store()
            .find_by_trigger_request_id(read.connection(), trigger_request_id)
            .await
            .map(|row| row.map(|row| (row.execution_id, row.monitor_id, row.status)))
            .map_err(Into::into)
    }

    pub(crate) async fn queue_monitoring_execution(
        &self,
        execution_id: String,
        trigger_request_id: Option<String>,
        plan: &ProbePlan,
        planned_at_ms: i64,
    ) -> Result<(), ApplicationError> {
        let executions = MonitoringExecutionRepository;
        let row = NewExecutionRow {
            id: execution_id,
            monitor_id: plan.monitor_id.clone(),
            trigger_kind: plan.trigger_kind.as_str().to_string(),
            trigger_request_id,
            status: "queued".to_string(),
            planned_at_ms,
            started_at_ms: None,
            config_revision: plan.revision.0 as i64,
            config_snapshot_hash: plan.config_snapshot_hash.clone(),
            endpoint_revision: plan
                .target_plans
                .iter()
                .map(|target| target.endpoint_revision)
                .min()
                .unwrap_or(1),
            target_count: plan.target_plans.len() as i64,
            created_at_ms: planned_at_ms,
        };
        self.runtime
            .write(|write| {
                Box::pin(async move { executions.insert_execution(write.connection(), &row).await })
            })
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn start_queued_monitoring_execution(
        &self,
        execution_id: &str,
    ) -> Result<bool, ApplicationError> {
        if execution_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let execution_id = execution_id.to_string();
        let now_ms = self.now_ms();
        let executions = MonitoringExecutionRepository;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    executions
                        .start_queued(write.connection(), &execution_id, now_ms)
                        .await
                })
            })
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn interrupt_monitoring_execution(
        &self,
        execution_id: &str,
    ) -> Result<bool, ApplicationError> {
        if execution_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let execution_id = execution_id.to_string();
        let now_ms = self.now_ms();
        let executions = MonitoringExecutionRepository;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    executions
                        .interrupt(write.connection(), &execution_id, now_ms)
                        .await
                })
            })
            .await
            .map_err(ApplicationError::from)
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

    pub(crate) async fn repair_pending_monitoring_rollups(
        &self,
        limit: u32,
    ) -> Result<u32, ApplicationError> {
        let retention = MonitoringRetentionRepository;
        let now_ms = self.now_ms();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    retention
                        .repair_dirty_ranges(write.connection(), limit, now_ms)
                        .await
                        .map(|outcome| outcome.repaired_ranges)
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn mark_corrupt_monitoring_rollups(&self) -> Result<u32, ApplicationError> {
        let retention = MonitoringRetentionRepository;
        let now_ms = self.now_ms();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    retention
                        .mark_corrupt_rollups_dirty(write.connection(), now_ms)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn delete_rolled_up_monitoring_executions(
        &self,
        cutoff_ms: i64,
        per_monitor_limit: u32,
        global_limit: u32,
    ) -> Result<u32, ApplicationError> {
        let retention = MonitoringRetentionRepository;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    retention
                        .delete_rolled_up_raw_executions(
                            write.connection(),
                            cutoff_ms,
                            per_monitor_limit,
                            global_limit,
                        )
                        .await
                        .map(|outcome| outcome.deleted_executions)
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

    pub(crate) async fn list_legacy_run_page(
        &self,
        monitor_id: &str,
        cursor: Option<&ChannelMonitorRunCursor>,
        limit: PageLimit,
    ) -> Result<ChannelMonitorRunPage, ApplicationError> {
        if monitor_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.legacy_run_reader
            .list_page(&mut read, monitor_id, cursor, limit.get())
            .await
            .map_err(Into::into)
    }

    fn now_ms(&self) -> i64 {
        self.clock.now_utc().timestamp_millis()
    }

    fn now_ms_string(&self) -> String {
        self.now_ms().to_string()
    }

    fn execution_store(&self) -> MonitoringExecutionRepository {
        MonitoringExecutionRepository
    }
}

fn utc_day_window_ms(now_ms: i64) -> (i64, i64) {
    const DAY_MS: i64 = 86_400_000;
    let start = now_ms.div_euclid(DAY_MS).saturating_mul(DAY_MS);
    (start, start.saturating_add(DAY_MS))
}
