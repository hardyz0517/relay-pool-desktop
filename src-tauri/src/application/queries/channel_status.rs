use std::sync::Arc;

use crate::{
    application::{
        clock::Clock, error::ApplicationError, monitoring::queries::ChannelStatusReadModelQuery,
        pagination::PageLimit,
    },
    models::{
        channel_monitors::ChannelMonitor,
        monitoring::{
            ChannelMonitorAttemptHistoryInput, ChannelMonitorAttemptPage,
            ChannelMonitorExecutionDetail, ChannelMonitorExecutionIdInput,
            ChannelMonitorExecutionListInput, ChannelMonitorExecutionPage, ChannelStatusBucket,
            ChannelStatusOutcome, ChannelStatusWorkspaceInput, ChannelStatusWorkspaceV2,
            MonitoringCapabilityCatalog, MonitoringClientProfileCapability,
            MonitoringProtocolCapability, ProtocolKind,
        },
        shared_capabilities::{
            ChannelStatusSummary, ChannelStatusTimelinePoint, ChannelStatusWindowSummary,
        },
    },
    persistence::runtime::PersistenceHandle,
    services::monitoring::profiles::registry::BuiltinProfileRegistry,
};

#[derive(Clone)]
pub(crate) struct ChannelStatusQuery {
    read_model: ChannelStatusReadModelQuery,
}

impl ChannelStatusQuery {
    pub(crate) fn new(runtime: PersistenceHandle, clock: Arc<dyn Clock>) -> Self {
        Self {
            read_model: ChannelStatusReadModelQuery::new(runtime, clock),
        }
    }

    pub(crate) async fn load(
        &self,
        monitor_limit: PageLimit,
    ) -> Result<Vec<ChannelStatusSummary>, ApplicationError> {
        let workspace = self
            .read_model
            .load_workspace(ChannelStatusWorkspaceInput {
                limit: Some(monitor_limit.get()),
                ..ChannelStatusWorkspaceInput::default()
            })
            .await?;
        Ok(workspace.rows.into_iter().map(legacy_summary).collect())
    }

    pub(crate) async fn load_workspace(
        &self,
        input: ChannelStatusWorkspaceInput,
    ) -> Result<ChannelStatusWorkspaceV2, ApplicationError> {
        self.read_model.load_workspace(input).await
    }

    pub(crate) async fn list_executions(
        &self,
        input: ChannelMonitorExecutionListInput,
    ) -> Result<ChannelMonitorExecutionPage, ApplicationError> {
        self.read_model.list_executions(input).await
    }

    pub(crate) async fn get_execution(
        &self,
        input: ChannelMonitorExecutionIdInput,
    ) -> Result<ChannelMonitorExecutionDetail, ApplicationError> {
        self.read_model.get_execution(input).await
    }

    pub(crate) async fn list_attempt_history(
        &self,
        input: ChannelMonitorAttemptHistoryInput,
    ) -> Result<ChannelMonitorAttemptPage, ApplicationError> {
        self.read_model.list_attempt_history(input).await
    }

    pub(crate) async fn list_monitoring_capabilities(
        &self,
    ) -> Result<MonitoringCapabilityCatalog, ApplicationError> {
        let protocols = [
            ProtocolKind::OpenAiChat,
            ProtocolKind::OpenAiResponses,
            ProtocolKind::AnthropicMessages,
            ProtocolKind::GeminiNative,
            ProtocolKind::XaiGrok,
            ProtocolKind::GenericOpenAi,
        ]
        .into_iter()
        .map(|protocol| MonitoringProtocolCapability {
            id: protocol.as_str().to_string(),
            enabled: true,
            streaming: !matches!(protocol, ProtocolKind::GenericOpenAi),
        })
        .collect();
        let registry = BuiltinProfileRegistry::default();
        let profiles = registry
            .list()
            .map(|profile| {
                let summary = profile.golden_summary();
                MonitoringClientProfileCapability {
                    id: summary.id.as_str().to_string(),
                    version: summary.version,
                    enabled: summary.enabled,
                    cli_compat: !matches!(
                        summary.id,
                        crate::models::monitoring::ClientProfileId::StandardApi
                    ),
                    supported_protocols: summary
                        .supported_protocols
                        .into_iter()
                        .map(|protocol| protocol.as_str().to_string())
                        .collect(),
                    method: summary.method,
                    path: summary.path,
                    header_names: summary.header_names,
                    body_defaults: summary.body_defaults,
                    profile_hash: summary.profile_hash,
                }
            })
            .collect();
        Ok(MonitoringCapabilityCatalog {
            protocols,
            profiles,
        })
    }
}

fn legacy_summary(row: crate::models::monitoring::ChannelStatusRow) -> ChannelStatusSummary {
    let monitor = ChannelMonitor {
        id: row.monitor.id.clone(),
        name: row.monitor.name.clone(),
        target_type: row.monitor.target_type.clone(),
        station_id: row.target.station_id.clone(),
        station_key_id: row.target.station_key_id.clone(),
        template_id: String::new(),
        enabled: row.monitor.enabled,
        protocol_kind: row.monitor.protocol_kind.clone(),
        client_profile_id: row.monitor.client_profile_id.clone(),
        client_profile_version: row.monitor.client_profile_version,
        primary_model: row.monitor.primary_model.clone(),
        retry_max_attempts_per_model: 1,
        retry_initial_backoff_ms: 200,
        retry_max_backoff_ms: 2_000,
        risk_daily_probe_budget: 200,
        health_writeback_mode: "observe_only".to_string(),
        health_failure_threshold: 2,
        health_recovery_threshold: 2,
        attempt_timeout_ms: 10_000,
        execution_timeout_ms: 30_000,
        schedule_revision: 1,
        interval_seconds: row.monitor.interval_seconds,
        jitter_seconds: row.monitor.jitter_seconds,
        timeout_seconds: 0,
        max_concurrency: 1,
        consecutive_failure_threshold: 1,
        fallback_models: row.monitor.fallback_models.clone(),
        note: None,
        created_at: "0".to_string(),
        updated_at: "0".to_string(),
    };
    ChannelStatusSummary {
        monitor,
        recent: legacy_recent_summary(&row),
        last24h: legacy_bucket_summary("24h", &row.hourly_buckets),
        last7d: legacy_bucket_summary("7d", tail(&row.daily_buckets, 7)),
    }
}

fn legacy_recent_summary(
    row: &crate::models::monitoring::ChannelStatusRow,
) -> ChannelStatusWindowSummary {
    let total_count = row.recent.len() as i64;
    let success_count = row
        .recent
        .iter()
        .filter(|point| matches!(point.outcome, ChannelStatusOutcome::Available))
        .count() as i64;
    let failure_count = row
        .recent
        .iter()
        .filter(|point| matches!(point.outcome, ChannelStatusOutcome::Unavailable))
        .count() as i64;
    let warning_count = row
        .recent
        .iter()
        .filter(|point| {
            matches!(
                point.outcome,
                ChannelStatusOutcome::Degraded | ChannelStatusOutcome::Skipped
            )
        })
        .count() as i64;
    ChannelStatusWindowSummary {
        window: "recent".to_string(),
        total_count,
        success_count,
        failure_count,
        warning_count,
        availability_percent: percent(success_count, total_count),
        avg_latency_ms: average_latency(row.recent.iter().filter_map(|point| point.latency_ms)),
        avg_endpoint_ping_ms: None,
        last_checked_at: row
            .latest
            .as_ref()
            .and_then(|latest| latest.finished_at_ms)
            .map(|value| value.to_string()),
        latest_status: row
            .latest
            .as_ref()
            .map(|latest| legacy_status(latest.outcome)),
        latest_error_message: None,
        timeline: row
            .recent
            .iter()
            .map(|point| ChannelStatusTimelinePoint {
                status: legacy_status(point.outcome),
                latency_ms: point.latency_ms,
                endpoint_ping_ms: None,
                checked_at: point.checked_at_ms.unwrap_or_default().to_string(),
            })
            .collect(),
    }
}

fn legacy_bucket_summary(
    window: &str,
    buckets: &[ChannelStatusBucket],
) -> ChannelStatusWindowSummary {
    let total_count = buckets
        .iter()
        .map(|bucket| i64::from(bucket.counts.total))
        .sum::<i64>();
    let success_count = buckets
        .iter()
        .map(|bucket| i64::from(bucket.counts.available))
        .sum::<i64>();
    let failure_count = buckets
        .iter()
        .map(|bucket| i64::from(bucket.counts.unavailable))
        .sum::<i64>();
    let warning_count = buckets
        .iter()
        .map(|bucket| i64::from(bucket.counts.degraded + bucket.counts.skipped))
        .sum::<i64>();
    ChannelStatusWindowSummary {
        window: window.to_string(),
        total_count,
        success_count,
        failure_count,
        warning_count,
        availability_percent: percent(success_count, total_count),
        avg_latency_ms: None,
        avg_endpoint_ping_ms: None,
        last_checked_at: buckets
            .iter()
            .rev()
            .find(|bucket| bucket.counts.total > 0)
            .map(|bucket| bucket.end_ms.to_string()),
        latest_status: buckets
            .iter()
            .rev()
            .find(|bucket| bucket.counts.total > 0)
            .map(|bucket| legacy_bucket_status(bucket)),
        latest_error_message: None,
        timeline: Vec::new(),
    }
}

fn legacy_status(outcome: ChannelStatusOutcome) -> String {
    match outcome {
        ChannelStatusOutcome::Available => "success",
        ChannelStatusOutcome::Degraded => "warning",
        ChannelStatusOutcome::Unavailable => "failed",
        ChannelStatusOutcome::Skipped | ChannelStatusOutcome::Missing => "skipped",
    }
    .to_string()
}

fn legacy_bucket_status(bucket: &ChannelStatusBucket) -> String {
    if bucket.counts.unavailable > 0 {
        "failed"
    } else if bucket.counts.degraded > 0 || bucket.counts.skipped > 0 {
        "warning"
    } else if bucket.counts.available > 0 {
        "success"
    } else {
        "skipped"
    }
    .to_string()
}

fn percent(count: i64, total: i64) -> Option<f64> {
    (total > 0).then(|| count as f64 * 100.0 / total as f64)
}

fn average_latency(values: impl Iterator<Item = i64>) -> Option<i64> {
    let values = values.collect::<Vec<_>>();
    (!values.is_empty()).then(|| values.iter().sum::<i64>() / values.len() as i64)
}

fn tail<T>(items: &[T], count: usize) -> &[T] {
    if items.len() <= count {
        items
    } else {
        &items[items.len() - count..]
    }
}
