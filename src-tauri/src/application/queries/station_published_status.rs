use std::collections::HashMap;

use serde::Serialize;

use crate::{
    application::{clock::Clock, error::ApplicationError},
    models::station_published_status::{
        MAX_PUBLISHED_STATUS_MONITORS, MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL,
        STATION_PUBLISHED_STATUS_SOURCE_KIND,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            station_catalog::StationCatalogStore,
            station_published_status_store::{
                PublishedMonitorSampleRow, PublishedStatusWorkspaceRows,
                StationPublishedStatusStore,
            },
        },
    },
    services::collectors::{
        contract::CollectorTaskKind, drivers::station_type_supports_collector_task,
    },
};

#[derive(Clone)]
pub(crate) struct StationPublishedStatusQuery {
    runtime: PersistenceHandle,
    clock: std::sync::Arc<dyn Clock>,
    stations: StationCatalogStore,
    published_status: StationPublishedStatusStore,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationPublishedStatusWorkspace {
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) supported: bool,
    pub(crate) source_state: String,
    pub(crate) completeness: Option<String>,
    pub(crate) last_attempt_at_ms: Option<i64>,
    pub(crate) last_success_at_ms: Option<i64>,
    pub(crate) last_complete_at_ms: Option<i64>,
    pub(crate) safe_error_kind: Option<String>,
    pub(crate) monitor_count: u32,
    pub(crate) stale: bool,
    pub(crate) rows: Vec<StationPublishedStatusMonitor>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationPublishedStatusMonitor {
    pub(crate) id: String,
    pub(crate) upstream_monitor_id: String,
    pub(crate) identity_kind: String,
    pub(crate) name: String,
    pub(crate) provider: String,
    pub(crate) group_name: Option<String>,
    pub(crate) primary_model: String,
    pub(crate) extra_models: Vec<String>,
    pub(crate) presence_status: String,
    pub(crate) current_outcome: String,
    pub(crate) current_latency_ms: Option<i64>,
    pub(crate) current_ping_latency_ms: Option<i64>,
    pub(crate) recent_availability_percent: Option<f64>,
    pub(crate) upstream_checked_at_ms: Option<i64>,
    pub(crate) samples: Vec<StationPublishedStatusSample>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationPublishedStatusSample {
    pub(crate) model: String,
    pub(crate) checked_at_ms: i64,
    pub(crate) outcome: String,
    pub(crate) latency_ms: Option<i64>,
    pub(crate) ping_latency_ms: Option<i64>,
}

impl StationPublishedStatusQuery {
    pub(crate) fn new(runtime: PersistenceHandle, clock: std::sync::Arc<dyn Clock>) -> Self {
        Self {
            runtime,
            clock,
            stations: StationCatalogStore,
            published_status: StationPublishedStatusStore,
        }
    }

    pub(crate) async fn load_workspace(
        &self,
        station_id: &str,
        published_status_interval_minutes: u16,
    ) -> Result<StationPublishedStatusWorkspace, ApplicationError> {
        if station_id.trim().is_empty() || !(1..=1_440).contains(&published_status_interval_minutes)
        {
            return Err(ApplicationError::ConstraintViolation);
        }

        let mut read = self.runtime.begin_read().await?;
        let station = self.stations.get(&mut read, station_id).await?;
        let supported = station_type_supports_collector_task(
            &station.station_type,
            CollectorTaskKind::PublishedStatus,
        );
        if !supported {
            return Ok(unsupported_workspace(station.id, station.endpoint_revision));
        }

        let rows = self
            .published_status
            .load_workspace(
                &mut read,
                &station.id,
                station.endpoint_revision,
                STATION_PUBLISHED_STATUS_SOURCE_KIND,
                MAX_PUBLISHED_STATUS_MONITORS as u32,
                MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL as u32,
            )
            .await?;
        Ok(workspace_from_rows(
            station.id,
            station.endpoint_revision,
            rows,
            self.clock.now_utc().timestamp_millis(),
            published_status_interval_minutes,
        ))
    }
}

fn unsupported_workspace(
    station_id: String,
    endpoint_revision: i64,
) -> StationPublishedStatusWorkspace {
    StationPublishedStatusWorkspace {
        station_id,
        endpoint_revision,
        supported: false,
        source_state: "unsupported".to_string(),
        completeness: None,
        last_attempt_at_ms: None,
        last_success_at_ms: None,
        last_complete_at_ms: None,
        safe_error_kind: None,
        monitor_count: 0,
        stale: false,
        rows: Vec::new(),
    }
}

fn workspace_from_rows(
    station_id: String,
    endpoint_revision: i64,
    rows: PublishedStatusWorkspaceRows,
    now_ms: i64,
    interval_minutes: u16,
) -> StationPublishedStatusWorkspace {
    let source = rows.source;
    let last_success_at_ms = source
        .as_ref()
        .and_then(|source| source.last_success_at.as_deref())
        .and_then(parse_timestamp_ms);
    let freshness_deadline = stale_after_ms(interval_minutes);
    let stale = last_success_at_ms
        .is_some_and(|last_success| now_ms.saturating_sub(last_success) > freshness_deadline);
    let samples_by_monitor = samples_by_monitor(rows.samples);
    let monitor_rows: Vec<StationPublishedStatusMonitor> = rows
        .monitors
        .into_iter()
        .map(|monitor| {
            let samples = samples_by_monitor
                .get(&monitor.id)
                .cloned()
                .unwrap_or_default();
            StationPublishedStatusMonitor {
                id: monitor.id.clone(),
                upstream_monitor_id: monitor.upstream_monitor_id,
                identity_kind: monitor.identity_kind,
                name: monitor.name,
                provider: monitor.provider,
                group_name: monitor.group_name,
                primary_model: monitor.primary_model,
                extra_models: bounded_extra_models(&monitor.extra_models_json),
                presence_status: monitor.presence_status,
                current_outcome: monitor.current_outcome,
                current_latency_ms: monitor.current_latency_ms,
                current_ping_latency_ms: monitor.current_ping_latency_ms,
                recent_availability_percent: recent_availability_percent(&samples),
                upstream_checked_at_ms: monitor.upstream_checked_at_ms,
                samples,
            }
        })
        .collect();
    let source_state = source
        .as_ref()
        .map(|source| source.source_state.clone())
        .unwrap_or_else(|| "never_collected".to_string());
    let completeness = source.as_ref().and_then(|source| {
        matches!(
            source.source_state.as_str(),
            "available" | "empty" | "degraded"
        )
        .then(|| {
            if source.source_state == "degraded" {
                "partial"
            } else {
                "complete"
            }
            .to_string()
        })
    });

    StationPublishedStatusWorkspace {
        station_id,
        endpoint_revision,
        supported: true,
        source_state,
        completeness,
        last_attempt_at_ms: source
            .as_ref()
            .and_then(|source| parse_timestamp_ms(&source.last_attempt_at)),
        last_success_at_ms,
        last_complete_at_ms: source
            .as_ref()
            .and_then(|source| source.last_complete_at.as_deref())
            .and_then(parse_timestamp_ms),
        safe_error_kind: source.and_then(|source| source.last_error_kind),
        monitor_count: monitor_rows.len() as u32,
        stale,
        rows: monitor_rows,
    }
}

fn samples_by_monitor(
    rows: Vec<PublishedMonitorSampleRow>,
) -> HashMap<String, Vec<StationPublishedStatusSample>> {
    let mut samples = HashMap::<String, Vec<StationPublishedStatusSample>>::new();
    for sample in rows {
        let entry = samples.entry(sample.monitor_id).or_default();
        if entry.len() >= MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL {
            continue;
        }
        entry.push(StationPublishedStatusSample {
            model: sample.model,
            checked_at_ms: sample.checked_at_ms,
            outcome: sample.outcome,
            latency_ms: sample.latency_ms,
            ping_latency_ms: sample.ping_latency_ms,
        });
    }
    samples
}

fn recent_availability_percent(samples: &[StationPublishedStatusSample]) -> Option<f64> {
    (!samples.is_empty()).then(|| {
        let available = samples
            .iter()
            .filter(|sample| sample.outcome == "available")
            .count();
        available as f64 * 100.0 / samples.len() as f64
    })
}

fn bounded_extra_models(value: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(value)
        .unwrap_or_default()
        .into_iter()
        .take(32)
        .collect()
}

fn parse_timestamp_ms(value: &str) -> Option<i64> {
    value.parse::<i64>().ok().or_else(|| {
        chrono::DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|value| value.timestamp_millis())
    })
}

fn stale_after_ms(interval_minutes: u16) -> i64 {
    i64::from(interval_minutes)
        .saturating_mul(2)
        .saturating_mul(60_000)
        .max(10 * 60_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_threshold_is_based_on_the_configured_collection_interval() {
        assert_eq!(parse_timestamp_ms("1700000000000"), Some(1_700_000_000_000));
        assert_eq!(parse_timestamp_ms("not-a-time"), None);
        assert_eq!(stale_after_ms(1), 10 * 60_000);
        assert_eq!(stale_after_ms(5), 10 * 60_000);
        assert_eq!(stale_after_ms(10), 20 * 60_000);
    }

    #[test]
    fn extra_models_are_bounded_and_fail_closed_for_invalid_storage() {
        assert!(bounded_extra_models("not-json").is_empty());
        assert_eq!(bounded_extra_models("[\"a\",\"b\"]"), vec!["a", "b"]);
    }

    #[test]
    fn recent_availability_uses_all_retained_samples_as_the_denominator() {
        let samples = [
            sample("available"),
            sample("degraded"),
            sample("unavailable"),
            sample("unknown"),
        ];

        assert_eq!(recent_availability_percent(&samples), Some(25.0));
        assert_eq!(recent_availability_percent(&[]), None);
    }

    fn sample(outcome: &str) -> StationPublishedStatusSample {
        StationPublishedStatusSample {
            model: "fixture-model".to_string(),
            checked_at_ms: 1,
            outcome: outcome.to_string(),
            latency_ms: None,
            ping_latency_ms: None,
        }
    }
}
