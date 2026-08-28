use sha2::{Digest, Sha256};
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
                PublishedMonitorRow, PublishedMonitorSampleRow, PublishedStatusWorkspaceRows,
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
pub(crate) struct PublishedStatusSourceDescriptor {
    pub(crate) station_type: String,
    pub(crate) source_kind: String,
    pub(crate) descriptor_version: u16,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StationPublishedStatusOverviewInput {
    pub(crate) filter: Option<StationPublishedStatusOverviewFilter>,
    pub(crate) cursor: Option<String>,
    pub(crate) limit: Option<u32>,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StationPublishedStatusOverviewFilter {
    pub(crate) search: Option<String>,
    pub(crate) station_id: Option<String>,
    pub(crate) outcome: Option<String>,
    pub(crate) source_state: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationPublishedStatusOverview {
    pub(crate) read_at_ms: i64,
    pub(crate) summary: StationPublishedStatusOverviewSummary,
    pub(crate) rows: Vec<StationPublishedStatusOverviewRow>,
    pub(crate) page: StationPublishedStatusOverviewPage,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationPublishedStatusOverviewSummary {
    pub(crate) station_total: usize,
    pub(crate) supported_station_count: usize,
    pub(crate) unsupported_capability_station_count: usize,
    pub(crate) never_collected_station_count: usize,
    pub(crate) available_source_count: usize,
    pub(crate) empty_source_count: usize,
    pub(crate) authorization_required_source_count: usize,
    pub(crate) degraded_source_count: usize,
    pub(crate) failed_source_count: usize,
    pub(crate) unsupported_source_count: usize,
    pub(crate) monitor_total: usize,
    pub(crate) available_monitor_count: usize,
    pub(crate) degraded_monitor_count: usize,
    pub(crate) unavailable_monitor_count: usize,
    pub(crate) unknown_monitor_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationPublishedStatusOverviewPage {
    pub(crate) limit: u32,
    pub(crate) returned: usize,
    pub(crate) next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationPublishedStatusOverviewRow {
    pub(crate) station_id: String,
    pub(crate) station_name: String,
    pub(crate) station_type: String,
    pub(crate) station_enabled: bool,
    pub(crate) station_priority: i64,
    pub(crate) source_kind: String,
    pub(crate) source_state: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) completeness: Option<String>,
    pub(crate) last_attempt_at_ms: Option<i64>,
    pub(crate) last_success_at_ms: Option<i64>,
    pub(crate) last_complete_at_ms: Option<i64>,
    pub(crate) stale: bool,
    #[serde(flatten)]
    pub(crate) monitor: StationPublishedStatusMonitor,
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

    pub(crate) async fn load_overview(
        &self,
        input: &StationPublishedStatusOverviewInput,
        published_status_interval_minutes: u16,
    ) -> Result<StationPublishedStatusOverview, ApplicationError> {
        if !(1..=1_440).contains(&published_status_interval_minutes) {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        let stations = self.stations.list(&mut read).await?;
        let descriptors = vec![PublishedStatusSourceDescriptor {
            station_type: "sub2api".to_string(),
            source_kind: STATION_PUBLISHED_STATUS_SOURCE_KIND.to_string(),
            descriptor_version: 1,
        }];
        let source_kinds: Vec<String> = descriptors.iter().map(|d| d.source_kind.clone()).collect();
        let data = self
            .published_status
            .load_overview(&mut read, &source_kinds, 200, 12_000)
            .await?;
        let now = self.clock.now_utc().timestamp_millis();
        let mut rows = Vec::new();
        let supported = stations
            .iter()
            .filter(|station| {
                descriptors
                    .iter()
                    .any(|d| d.station_type == station.station_type)
            })
            .count();
        let never = stations
            .iter()
            .filter(|station| {
                descriptors
                    .iter()
                    .any(|d| d.station_type == station.station_type)
                    && !data.sources.iter().any(|s| {
                        s.station_id == station.id
                            && s.endpoint_revision == station.endpoint_revision
                            && s.source_kind == STATION_PUBLISHED_STATUS_SOURCE_KIND
                    })
            })
            .count();
        let filter = input.filter.as_ref();
        let limit = input.limit.unwrap_or(100).clamp(1, 200) as usize;
        for station in &stations {
            if filter
                .and_then(|f| f.station_id.as_deref())
                .is_some_and(|id| id != station.id)
            {
                continue;
            }
            let is_supported = descriptors
                .iter()
                .any(|d| d.station_type == station.station_type);
            if !is_supported {
                continue;
            }
            let source = data.sources.iter().find(|s| {
                s.station_id == station.id
                    && s.endpoint_revision == station.endpoint_revision
                    && s.source_kind == STATION_PUBLISHED_STATUS_SOURCE_KIND
            });
            for monitor in data.monitors.iter().filter(|m| {
                m.station_id == station.id
                    && m.endpoint_revision == station.endpoint_revision
                    && m.source_kind == STATION_PUBLISHED_STATUS_SOURCE_KIND
            }) {
                if let Some(f) = filter {
                    if f.outcome
                        .as_deref()
                        .is_some_and(|v| v != monitor.current_outcome)
                    {
                        continue;
                    }
                    if f.source_state
                        .as_deref()
                        .is_some_and(|v| source.map(|s| s.source_state.as_str()) != Some(v))
                    {
                        continue;
                    }
                    if let Some(q) = f.search.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                        let hay = format!(
                            "{} {} {} {} {}",
                            station.name,
                            monitor.name,
                            monitor.provider,
                            monitor.group_name.as_deref().unwrap_or(""),
                            monitor.primary_model
                        )
                        .to_lowercase();
                        if !hay.contains(&q.to_lowercase()) {
                            continue;
                        }
                    }
                }
                let samples: Vec<StationPublishedStatusSample> = data
                    .samples
                    .iter()
                    .filter(|s| s.monitor_id == monitor.id)
                    .map(|s| StationPublishedStatusSample {
                        model: s.model.clone(),
                        checked_at_ms: s.checked_at_ms,
                        outcome: s.outcome.clone(),
                        latency_ms: s.latency_ms,
                        ping_latency_ms: s.ping_latency_ms,
                    })
                    .collect();
                let monitor_view = project_monitor(monitor, samples);
                let last_success = source
                    .and_then(|s| s.last_success_at.as_deref())
                    .and_then(parse_timestamp_ms);
                rows.push(StationPublishedStatusOverviewRow {
                    station_id: station.id.clone(),
                    station_name: station.name.clone(),
                    station_type: station.station_type.clone(),
                    station_enabled: station.enabled,
                    station_priority: station.priority,
                    source_kind: STATION_PUBLISHED_STATUS_SOURCE_KIND.to_string(),
                    source_state: source
                        .map(|s| s.source_state.clone())
                        .unwrap_or_else(|| "never_collected".into()),
                    endpoint_revision: station.endpoint_revision,
                    completeness: source.and_then(|s| {
                        matches!(s.source_state.as_str(), "available" | "empty" | "degraded").then(
                            || {
                                if s.source_state == "degraded" {
                                    "partial".to_string()
                                } else {
                                    "complete".to_string()
                                }
                            },
                        )
                    }),
                    last_attempt_at_ms: source.and_then(|s| parse_timestamp_ms(&s.last_attempt_at)),
                    last_success_at_ms: source
                        .and_then(|s| s.last_success_at.as_deref())
                        .and_then(parse_timestamp_ms),
                    last_complete_at_ms: source
                        .and_then(|s| s.last_complete_at.as_deref())
                        .and_then(parse_timestamp_ms),
                    stale: last_success.is_some_and(|v| {
                        now.saturating_sub(v) > stale_after_ms(published_status_interval_minutes)
                    }),
                    monitor: monitor_view,
                });
            }
        }
        rows.sort_by(|a, b| {
            b.monitor
                .upstream_checked_at_ms
                .cmp(&a.monitor.upstream_checked_at_ms)
                .then_with(|| b.last_attempt_at_ms.cmp(&a.last_attempt_at_ms))
                .then_with(|| a.station_priority.cmp(&b.station_priority))
                .then_with(|| a.station_id.cmp(&b.station_id))
                .then_with(|| {
                    a.monitor
                        .provider
                        .to_lowercase()
                        .cmp(&b.monitor.provider.to_lowercase())
                })
                .then_with(|| {
                    a.monitor
                        .group_name
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .cmp(&b.monitor.group_name.as_deref().unwrap_or("").to_lowercase())
                })
                .then_with(|| {
                    a.monitor
                        .name
                        .to_lowercase()
                        .cmp(&b.monitor.name.to_lowercase())
                })
                .then_with(|| {
                    a.monitor
                        .primary_model
                        .to_lowercase()
                        .cmp(&b.monitor.primary_model.to_lowercase())
                })
                .then_with(|| {
                    a.monitor
                        .upstream_monitor_id
                        .cmp(&b.monitor.upstream_monitor_id)
                })
                .then_with(|| a.monitor.id.cmp(&b.monitor.id))
        });
        let monitor_total = rows.len();
        let source_count = |state: &str| {
            data.sources
                .iter()
                .filter(|s| s.source_state == state)
                .count()
        };
        let count = |outcome: &str| {
            rows.iter()
                .filter(|r| r.monitor.current_outcome == outcome)
                .count()
        };
        let available_count = count("available");
        let degraded_count = count("degraded");
        let unavailable_count = count("unavailable");
        let unknown_count = count("unknown");
        let fingerprint = overview_filter_fingerprint(filter);
        let start = match input.cursor.as_deref() {
            None => 0,
            Some(cursor) => parse_overview_cursor(cursor, &fingerprint)
                .ok_or(ApplicationError::ConstraintViolation)?
                .min(monitor_total),
        };
        let next_cursor = (start + limit < monitor_total)
            .then(|| format!("v1:{}:{}", start + limit, fingerprint));
        let rows = rows.into_iter().skip(start).take(limit).collect::<Vec<_>>();
        let returned = rows.len();
        Ok(StationPublishedStatusOverview {
            read_at_ms: now,
            summary: StationPublishedStatusOverviewSummary {
                station_total: stations.len(),
                supported_station_count: supported,
                unsupported_capability_station_count: stations.len().saturating_sub(supported),
                never_collected_station_count: never,
                available_source_count: source_count("available"),
                empty_source_count: source_count("empty"),
                authorization_required_source_count: source_count("authorization_required"),
                degraded_source_count: source_count("degraded"),
                failed_source_count: source_count("failed"),
                unsupported_source_count: source_count("unsupported"),
                monitor_total,
                available_monitor_count: available_count,
                degraded_monitor_count: degraded_count,
                unavailable_monitor_count: unavailable_count,
                unknown_monitor_count: unknown_count,
            },
            rows,
            page: StationPublishedStatusOverviewPage {
                limit: limit as u32,
                returned,
                next_cursor: next_cursor,
            },
        })
    }
}

fn overview_filter_fingerprint(filter: Option<&StationPublishedStatusOverviewFilter>) -> String {
    let canonical = format!(
        "{}\0{}\0{}\0{}",
        filter
            .and_then(|f| f.search.as_deref())
            .unwrap_or("")
            .trim()
            .to_lowercase(),
        filter.and_then(|f| f.station_id.as_deref()).unwrap_or(""),
        filter.and_then(|f| f.outcome.as_deref()).unwrap_or(""),
        filter.and_then(|f| f.source_state.as_deref()).unwrap_or("")
    );
    format!("{:x}", Sha256::digest(canonical.as_bytes()))
}

pub(crate) fn is_valid_overview_cursor_shape(cursor: &str) -> bool {
    let Some(value) = cursor.strip_prefix("v1:") else {
        return false;
    };
    let mut parts = value.split(':');
    let Some(offset) = parts.next() else {
        return false;
    };
    let Some(fingerprint) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && !offset.is_empty()
        && offset.bytes().all(|byte| byte.is_ascii_digit())
        && offset.parse::<usize>().is_ok()
        && fingerprint.len() == 64
        && fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn parse_overview_cursor(cursor: &str, fingerprint: &str) -> Option<usize> {
    if !is_valid_overview_cursor_shape(cursor) {
        return None;
    }
    let mut parts = cursor.strip_prefix("v1:")?.splitn(2, ':');
    let offset = parts.next()?.parse::<usize>().ok()?;
    (parts.next()? == fingerprint).then_some(offset)
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
            project_monitor(&monitor, samples)
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

fn project_monitor(
    monitor: &PublishedMonitorRow,
    samples: Vec<StationPublishedStatusSample>,
) -> StationPublishedStatusMonitor {
    StationPublishedStatusMonitor {
        id: monitor.id.clone(),
        upstream_monitor_id: monitor.upstream_monitor_id.clone(),
        identity_kind: monitor.identity_kind.clone(),
        name: monitor.name.clone(),
        provider: monitor.provider.clone(),
        group_name: monitor.group_name.clone(),
        primary_model: monitor.primary_model.clone(),
        extra_models: bounded_extra_models(&monitor.extra_models_json),
        presence_status: monitor.presence_status.clone(),
        current_outcome: monitor.current_outcome.clone(),
        current_latency_ms: monitor.current_latency_ms,
        current_ping_latency_ms: monitor.current_ping_latency_ms,
        recent_availability_percent: recent_availability_percent(&samples),
        upstream_checked_at_ms: monitor.upstream_checked_at_ms,
        samples,
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
    use crate::application::clock::SystemClock;
    use crate::persistence::runtime::PersistenceRuntime;
    use crate::persistence::stores::station_published_status_store::{
        PublishedMonitorWrite, PublishedStatusSourceWrite,
    };
    use std::sync::Arc;

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

    #[tokio::test]
    async fn overview_filters_and_paginates_without_changing_global_summary() {
        let directory = tempfile::tempdir().expect("temp directory");
        let runtime =
            PersistenceRuntime::initialize_new(&directory.path().join("overview.sqlite3"))
                .await
                .expect("initialize database");
        let status_store = StationPublishedStatusStore;
        let mut write = runtime.handle().begin_write().await.expect("write session");
        seed_overview_station(&mut write, "station-a", "Alpha", "sub2api", 1, 1).await;
        seed_overview_station(&mut write, "station-b", "Beta", "sub2api", 1, 2).await;
        seed_overview_station(&mut write, "station-c", "Unsupported", "openai", 1, 3).await;
        status_store
            .upsert_source(
                &mut write,
                &overview_source("station-a", 1, "available", "100"),
            )
            .await
            .expect("source a");
        status_store
            .upsert_source(
                &mut write,
                &overview_source("station-b", 1, "degraded", "200"),
            )
            .await
            .expect("source b");
        status_store
            .upsert_monitor(
                &mut write,
                &overview_monitor("station-a", "monitor-a", "available", "Provider A", 300),
            )
            .await
            .expect("monitor a");
        status_store
            .upsert_monitor(
                &mut write,
                &overview_monitor("station-b", "monitor-b", "degraded", "Provider B", 200),
            )
            .await
            .expect("monitor b");
        write.commit().await.expect("commit fixture");

        let query = StationPublishedStatusQuery::new(runtime.handle(), Arc::new(SystemClock));
        let first = query
            .load_overview(
                &StationPublishedStatusOverviewInput {
                    limit: Some(1),
                    ..Default::default()
                },
                15,
            )
            .await
            .expect("overview");
        assert_eq!(first.rows.len(), 1);
        assert_eq!(first.summary.station_total, 3);
        assert_eq!(first.summary.supported_station_count, 2);
        assert_eq!(first.summary.unsupported_capability_station_count, 1);
        assert_eq!(first.summary.monitor_total, 2);
        assert_eq!(first.summary.available_monitor_count, 1);
        assert_eq!(first.summary.degraded_monitor_count, 1);
        assert_eq!(first.summary.available_source_count, 1);
        assert_eq!(first.summary.degraded_source_count, 1);
        assert_eq!(first.rows[0].station_id, "station-a");
        let cursor = first.page.next_cursor.clone().expect("next cursor");

        let second = query
            .load_overview(
                &StationPublishedStatusOverviewInput {
                    cursor: Some(cursor.clone()),
                    limit: Some(1),
                    ..Default::default()
                },
                15,
            )
            .await
            .expect("second page");
        assert_eq!(second.rows.len(), 1);
        assert_eq!(second.rows[0].station_id, "station-b");
        assert_eq!(second.summary.monitor_total, 2);

        let mismatch = query
            .load_overview(
                &StationPublishedStatusOverviewInput {
                    cursor: Some(cursor),
                    filter: Some(StationPublishedStatusOverviewFilter {
                        outcome: Some("degraded".into()),
                        ..Default::default()
                    }),
                    limit: Some(1),
                },
                15,
            )
            .await;
        assert!(matches!(
            mismatch,
            Err(ApplicationError::ConstraintViolation)
        ));

        let filtered = query
            .load_overview(
                &StationPublishedStatusOverviewInput {
                    filter: Some(StationPublishedStatusOverviewFilter {
                        search: Some("provider b".into()),
                        outcome: Some("degraded".into()),
                        source_state: Some("degraded".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                15,
            )
            .await
            .expect("filtered overview");
        assert_eq!(filtered.rows.len(), 1);
        assert_eq!(filtered.rows[0].station_id, "station-b");
        assert_eq!(filtered.rows[0].source_state, "degraded");
        assert_eq!(filtered.summary.monitor_total, 1);

        runtime.close().await.expect("close runtime");
    }

    async fn seed_overview_station(
        write: &mut crate::persistence::WriteSession,
        id: &str,
        name: &str,
        station_type: &str,
        revision: i64,
        priority: i64,
    ) {
        sqlx::query(
            "INSERT INTO stations (id,name,station_type,website_url,api_base_url,endpoint_revision,enabled,priority,collection_interval_minutes,created_at,updated_at) VALUES (?1,?2,?3,'https://example.invalid','https://example.invalid/v1',?4,1,?5,15,'0','0')",
        )
        .bind(id).bind(name).bind(station_type).bind(revision).bind(priority)
        .execute(write.connection()).await.expect("seed station");
    }

    fn overview_source(
        station_id: &str,
        revision: i64,
        state: &str,
        last_attempt_at: &str,
    ) -> PublishedStatusSourceWrite {
        PublishedStatusSourceWrite {
            station_id: station_id.into(),
            endpoint_revision: revision,
            source_kind: STATION_PUBLISHED_STATUS_SOURCE_KIND.into(),
            source_state: state.into(),
            last_attempt_at: last_attempt_at.into(),
            last_success_at: Some(last_attempt_at.into()),
            last_complete_at: Some(last_attempt_at.into()),
            last_error_kind: None,
            monitor_count: Some(1),
            created_at: "0".into(),
            updated_at: last_attempt_at.into(),
        }
    }

    fn overview_monitor(
        station_id: &str,
        id: &str,
        outcome: &str,
        provider: &str,
        upstream_checked_at_ms: i64,
    ) -> PublishedMonitorWrite {
        PublishedMonitorWrite {
            id: id.into(),
            station_id: station_id.into(),
            endpoint_revision: 1,
            source_kind: STATION_PUBLISHED_STATUS_SOURCE_KIND.into(),
            upstream_monitor_id: id.into(),
            identity_kind: "upstream_id".into(),
            name: format!("{provider} monitor"),
            provider: provider.into(),
            group_name: None,
            primary_model: "model-a".into(),
            extra_models_json: "[]".into(),
            current_outcome: outcome.into(),
            source_status: outcome.into(),
            current_latency_ms: Some(10),
            current_ping_latency_ms: Some(5),
            upstream_checked_at_ms: Some(upstream_checked_at_ms),
            last_seen_run_id: "run-1".into(),
            last_seen_at: "0".into(),
            created_at: "0".into(),
            updated_at: "0".into(),
        }
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
