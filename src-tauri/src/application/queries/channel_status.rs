use std::{collections::BTreeMap, sync::Arc};

use crate::{
    application::{clock::Clock, error::ApplicationError, pagination::PageLimit},
    models::{
        channel_monitors::{ChannelMonitor, ChannelMonitorRun},
        shared_capabilities::{
            ChannelStatusSummary, ChannelStatusTimelinePoint, ChannelStatusWindowSummary,
            ChannelStatusWorkspace,
        },
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            credential_store::CredentialStore,
            monitoring_store::{ChannelStatusRunRow, ChannelWindowAggregate, MonitoringStore},
            request_log_store::RequestLogStore,
            routing_store::RoutingStore,
        },
        ReadSession,
    },
};

const RECENT_RUN_LIMIT: u32 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

#[derive(Clone)]
pub(crate) struct ChannelStatusQuery {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    store: MonitoringStore,
    credentials: CredentialStore,
    request_logs: RequestLogStore,
    routing: RoutingStore,
}

impl ChannelStatusQuery {
    pub(crate) fn new(runtime: PersistenceHandle, clock: Arc<dyn Clock>) -> Self {
        Self {
            runtime,
            clock,
            store: MonitoringStore,
            credentials: CredentialStore,
            request_logs: RequestLogStore,
            routing: RoutingStore,
        }
    }

    pub(crate) async fn load(
        &self,
        monitor_limit: PageLimit,
    ) -> Result<Vec<ChannelStatusSummary>, ApplicationError> {
        let now_ms = self.clock.now_utc().timestamp_millis();
        let mut read = self.runtime.begin_read().await?;
        self.load_summaries(&mut read, monitor_limit, now_ms).await
    }

    pub(crate) async fn load_workspace(
        &self,
        monitor_limit: PageLimit,
    ) -> Result<ChannelStatusWorkspace, ApplicationError> {
        let now_ms = self.clock.now_utc().timestamp_millis();
        let mut read = self.runtime.begin_read().await?;
        let key_pool_items = self.credentials.list_key_pool_items(&mut read).await?;
        let request_logs = self.request_logs.list_recent(&mut read, 500).await?;
        let station_key_health = self.routing.list_station_key_health(&mut read).await?;
        let channel_status_summaries = self
            .load_summaries(&mut read, monitor_limit, now_ms)
            .await?;
        Ok(ChannelStatusWorkspace {
            key_pool_items,
            request_logs,
            station_key_health,
            channel_status_summaries,
        })
    }

    async fn load_summaries(
        &self,
        read: &mut ReadSession,
        monitor_limit: PageLimit,
        now_ms: i64,
    ) -> Result<Vec<ChannelStatusSummary>, ApplicationError> {
        let monitors = self.store.list_monitors(read, monitor_limit.get()).await?;
        let recent_runs = self
            .store
            .recent_status_runs(read, monitor_limit.get(), RECENT_RUN_LIMIT)
            .await?;
        let day = self
            .store
            .window_aggregates(read, now_ms.saturating_sub(DAY_MS), monitor_limit.get())
            .await?;
        let week = self
            .store
            .window_aggregates(read, now_ms.saturating_sub(7 * DAY_MS), monitor_limit.get())
            .await?;

        Ok(build_summaries(monitors, recent_runs, day, week, now_ms))
    }
}

fn build_summaries(
    monitors: Vec<ChannelMonitor>,
    recent_rows: Vec<ChannelStatusRunRow>,
    day_rows: Vec<ChannelWindowAggregate>,
    week_rows: Vec<ChannelWindowAggregate>,
    now_ms: i64,
) -> Vec<ChannelStatusSummary> {
    let mut recent_by_monitor: BTreeMap<String, Vec<ChannelMonitorRun>> = BTreeMap::new();
    for row in recent_rows {
        recent_by_monitor
            .entry(row.monitor_id)
            .or_default()
            .push(row.run);
    }
    let day_by_monitor = day_rows
        .into_iter()
        .map(|row| (row.monitor_id.clone(), row))
        .collect::<BTreeMap<_, _>>();
    let week_by_monitor = week_rows
        .into_iter()
        .map(|row| (row.monitor_id.clone(), row))
        .collect::<BTreeMap<_, _>>();

    monitors
        .into_iter()
        .map(|monitor| {
            let runs = recent_by_monitor.remove(&monitor.id).unwrap_or_default();
            let recent = summarize_recent("recent", &runs);
            let last24h = summarize_window(
                "last24h",
                day_by_monitor.get(&monitor.id),
                &runs,
                now_ms.saturating_sub(DAY_MS),
            );
            let last7d = summarize_window(
                "last7d",
                week_by_monitor.get(&monitor.id),
                &runs,
                now_ms.saturating_sub(7 * DAY_MS),
            );
            ChannelStatusSummary {
                monitor,
                recent,
                last24h,
                last7d,
            }
        })
        .collect()
}

fn summarize_recent(window: &str, runs: &[ChannelMonitorRun]) -> ChannelStatusWindowSummary {
    let total_count = runs.len() as i64;
    let success_count = count_status(runs, "success");
    let failure_count = count_status(runs, "failed");
    let warning_count = runs
        .iter()
        .filter(|run| matches!(run.status.as_str(), "warning" | "skipped"))
        .count() as i64;
    let latencies = runs
        .iter()
        .filter_map(|run| run.latency_ms.or(run.duration_ms))
        .collect::<Vec<_>>();
    let avg_latency_ms =
        (!latencies.is_empty()).then(|| latencies.iter().sum::<i64>() / latencies.len() as i64);
    summary_from_parts(
        window,
        total_count,
        success_count,
        failure_count,
        warning_count,
        avg_latency_ms,
        runs,
    )
}

fn summarize_window(
    window: &str,
    aggregate: Option<&ChannelWindowAggregate>,
    recent_runs: &[ChannelMonitorRun],
    since_ms: i64,
) -> ChannelStatusWindowSummary {
    let matching_runs = recent_runs
        .iter()
        .filter(|run| parse_millis(&run.started_at).is_some_and(|value| value >= since_ms))
        .cloned()
        .collect::<Vec<_>>();
    let empty = ChannelWindowAggregate {
        monitor_id: String::new(),
        total_count: 0,
        success_count: 0,
        failure_count: 0,
        warning_count: 0,
        avg_latency_ms: None,
    };
    let aggregate = aggregate.unwrap_or(&empty);
    summary_from_parts(
        window,
        aggregate.total_count,
        aggregate.success_count,
        aggregate.failure_count,
        aggregate.warning_count,
        aggregate.avg_latency_ms,
        &matching_runs,
    )
}

#[allow(clippy::too_many_arguments)]
fn summary_from_parts(
    window: &str,
    total_count: i64,
    success_count: i64,
    failure_count: i64,
    warning_count: i64,
    avg_latency_ms: Option<i64>,
    timeline_runs: &[ChannelMonitorRun],
) -> ChannelStatusWindowSummary {
    let latest = timeline_runs.first();
    ChannelStatusWindowSummary {
        window: window.to_string(),
        total_count,
        success_count,
        failure_count,
        warning_count,
        availability_percent: (total_count > 0)
            .then(|| success_count as f64 * 100.0 / total_count as f64),
        avg_latency_ms,
        avg_endpoint_ping_ms: None,
        last_checked_at: latest.map(run_checked_at),
        latest_status: latest.map(|run| run.status.clone()),
        latest_error_message: timeline_runs
            .iter()
            .find_map(|run| run.error_message.clone()),
        timeline: timeline_runs
            .iter()
            .map(|run| ChannelStatusTimelinePoint {
                status: run.status.clone(),
                latency_ms: run.latency_ms.or(run.duration_ms),
                endpoint_ping_ms: None,
                checked_at: run_checked_at(run),
            })
            .collect(),
    }
}

fn count_status(runs: &[ChannelMonitorRun], status: &str) -> i64 {
    runs.iter().filter(|run| run.status == status).count() as i64
}

fn run_checked_at(run: &ChannelMonitorRun) -> String {
    run.finished_at
        .clone()
        .unwrap_or_else(|| run.started_at.clone())
}

fn parse_millis(value: &str) -> Option<i64> {
    value.trim().parse().ok()
}
