use std::{cmp::Ordering, collections::BTreeMap, sync::Arc};

use crate::{
    application::{
        clock::Clock,
        error::ApplicationError,
        monitoring::buckets::{
            hourly_bucket_windows, local_day_bucket_windows, recent_target_result_limit,
            BucketAvailabilityState, BucketCounts, BucketTimezoneSource, BucketWindow,
            BucketWindowKind, DEGRADED_WEIGHT_BPS,
        },
    },
    models::monitoring::{
        ChannelMonitorAttemptCursor, ChannelMonitorAttemptHistoryInput, ChannelMonitorAttemptPage,
        ChannelMonitorExecutionCursor, ChannelMonitorExecutionDetail,
        ChannelMonitorExecutionIdInput, ChannelMonitorExecutionListInput,
        ChannelMonitorExecutionPage, ChannelStatusAggregate, ChannelStatusBucket,
        ChannelStatusBucketBoundary, ChannelStatusBucketCounts, ChannelStatusBucketKind,
        ChannelStatusBucketLayout, ChannelStatusBucketState, ChannelStatusCursor,
        ChannelStatusFreshness, ChannelStatusLatestResult, ChannelStatusMonitor,
        ChannelStatusOutcome, ChannelStatusPage, ChannelStatusRecentPoint, ChannelStatusRow,
        ChannelStatusSortDirection, ChannelStatusSortField, ChannelStatusTarget,
        ChannelStatusTimezone, ChannelStatusTimezoneSource, ChannelStatusWindowSummaryV2,
        ChannelStatusWorkspaceInput, ChannelStatusWorkspaceV2, ChannelStatusWorkspaceWindow,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::monitoring::status_read_repository::{
            DirtyRange, MonitoringStatusQueryRepository, RollupCell,
        },
    },
};

const WORKSPACE_SCHEMA_VERSION: u32 = 2;
const DEFAULT_WORKSPACE_LIMIT: u32 = 200;
const MAX_WORKSPACE_LIMIT: u32 = 500;
const DEFAULT_EXECUTION_LIMIT: u32 = 100;
const MAX_EXECUTION_LIMIT: u32 = 200;
const DEFAULT_ATTEMPT_LIMIT: u32 = 100;
const MAX_ATTEMPT_LIMIT: u32 = 200;
const MAX_BASE_SCAN_ROWS: i64 = 5_000;
const HOURLY_BUCKET_COUNT: u32 = 24;
const DAILY_BUCKET_COUNT: u32 = 30;

#[derive(Clone)]
pub(crate) struct ChannelStatusReadModelQuery {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    store: MonitoringStatusQueryRepository,
}

impl ChannelStatusReadModelQuery {
    pub(crate) fn new(runtime: PersistenceHandle, clock: Arc<dyn Clock>) -> Self {
        Self {
            runtime,
            clock,
            store: MonitoringStatusQueryRepository,
        }
    }

    pub(crate) async fn load_workspace(
        &self,
        input: ChannelStatusWorkspaceInput,
    ) -> Result<ChannelStatusWorkspaceV2, ApplicationError> {
        let now_ms = self.clock.now_utc().timestamp_millis();
        let limit = input
            .limit
            .unwrap_or(DEFAULT_WORKSPACE_LIMIT)
            .clamp(1, MAX_WORKSPACE_LIMIT);
        let hourly_windows = hourly_bucket_windows(now_ms, HOURLY_BUCKET_COUNT);
        let daily_windows =
            local_day_bucket_windows(now_ms, DAILY_BUCKET_COUNT, input.timezone_id.as_deref());

        let mut read = self.runtime.begin_read().await?;
        let base_rows = self
            .store
            .workspace_base_rows(&mut read, &input, MAX_BASE_SCAN_ROWS)
            .await?;
        let row_keys = base_rows
            .iter()
            .map(|row| (row.monitor_id.clone(), row.station_key_id.clone()))
            .collect::<Vec<_>>();
        let recent = self
            .store
            .workspace_recent_results(
                &mut read,
                &row_keys,
                i64::from(recent_target_result_limit()),
            )
            .await?;
        let running = self
            .store
            .workspace_running_executions(&mut read, &row_keys)
            .await?;
        let hourly_rollups = self
            .store
            .workspace_rollups(
                &mut read,
                &row_keys,
                "hour",
                hourly_windows
                    .windows
                    .first()
                    .map(|window| window.start_ms)
                    .unwrap_or_default(),
                hourly_windows
                    .windows
                    .last()
                    .map(|window| window.end_ms)
                    .unwrap_or_default(),
            )
            .await?;
        let daily_rollups = self
            .store
            .workspace_rollups(
                &mut read,
                &row_keys,
                "day",
                daily_windows
                    .windows
                    .first()
                    .map(|window| window.start_ms)
                    .unwrap_or_default(),
                daily_windows
                    .windows
                    .last()
                    .map(|window| window.end_ms)
                    .unwrap_or_default(),
            )
            .await?;
        let dirty_ranges = self
            .store
            .workspace_dirty_ranges(
                &mut read,
                &row_keys,
                min_window_start(&hourly_windows.windows, &daily_windows.windows),
                max_window_end(&hourly_windows.windows, &daily_windows.windows),
            )
            .await?;

        let mut rows = base_rows
            .into_iter()
            .map(|base| {
                let key = (base.monitor_id.clone(), base.station_key_id.clone());
                let recent_points = recent.get(&key).cloned().unwrap_or_default();
                let latest = recent_points.first().map(latest_from_recent);
                let hourly_buckets = build_buckets(
                    &hourly_windows.windows,
                    ChannelStatusBucketKind::Hour,
                    hourly_rollups.get(&key),
                    dirty_ranges.get(&key),
                );
                let daily_buckets = build_buckets(
                    &daily_windows.windows,
                    ChannelStatusBucketKind::Day,
                    daily_rollups.get(&key),
                    dirty_ranges.get(&key),
                );
                let running = running.get(&key).cloned();
                let selected_window = summarize_selected_window(
                    input.window,
                    &recent_points,
                    &hourly_buckets,
                    &daily_buckets,
                    latest.as_ref(),
                );
                ChannelStatusRow {
                    row_key: row_key(&key.0, key.1.as_deref()),
                    monitor: ChannelStatusMonitor {
                        id: base.monitor_id,
                        name: base.monitor_name,
                        target_type: base.target_type,
                        enabled: base.enabled,
                        pause_on_zero_balance: base.pause_on_zero_balance,
                        balance_paused: base.balance_paused,
                        protocol_kind: base.protocol_kind,
                        client_profile_id: base.client_profile_id,
                        client_profile_version: base.client_profile_version,
                        primary_model: base.primary_model,
                        fallback_models: base.fallback_models,
                        interval_seconds: base.interval_seconds,
                        jitter_seconds: base.jitter_seconds,
                        next_due_at_ms: base.next_due_at_ms,
                    },
                    target: ChannelStatusTarget {
                        station_id: base.station_id,
                        station_name: base.station_name,
                        station_key_id: base.station_key_id,
                        station_key_name: base.station_key_name,
                        group_name: base.group_name,
                        effective_group_category: base.effective_group_category,
                        endpoint_ping: base.endpoint_ping,
                    },
                    latest,
                    running,
                    recent: recent_points,
                    hourly_buckets,
                    daily_buckets,
                    selected_window,
                }
            })
            .filter(|row| {
                input
                    .filter
                    .outcome
                    .is_none_or(|outcome| row.selected_window.latest_outcome == outcome)
            })
            .collect::<Vec<_>>();

        sort_rows(&mut rows, input.sort.field, input.sort.direction);
        let total_rows = rows.len() as u32;
        let start_index = input
            .cursor
            .as_ref()
            .and_then(|cursor| {
                rows.iter()
                    .position(|row| row.row_key == cursor.row_key)
                    .map(|index| index.saturating_add(1))
            })
            .unwrap_or(0);
        let mut page_rows = rows
            .into_iter()
            .skip(start_index)
            .take(limit.saturating_add(1) as usize)
            .collect::<Vec<_>>();
        let next_cursor = if page_rows.len() > limit as usize {
            page_rows.pop().map(|row| ChannelStatusCursor {
                row_key: row.row_key,
            })
        } else {
            None
        };
        let aggregate = aggregate_rows(total_rows, &page_rows);
        let freshness = freshness(&page_rows);

        Ok(ChannelStatusWorkspaceV2 {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            generated_at_ms: now_ms,
            window: input.window,
            timezone: timezone_from_bucket_set(&daily_windows),
            bucket_layout: ChannelStatusBucketLayout {
                recent_limit: recent_target_result_limit(),
                hourly: boundaries(&hourly_windows.windows, ChannelStatusBucketKind::Hour),
                daily: boundaries(&daily_windows.windows, ChannelStatusBucketKind::Day),
            },
            aggregate,
            freshness,
            page: ChannelStatusPage {
                limit,
                returned: page_rows.len() as u32,
                next_cursor,
            },
            rows: page_rows,
        })
    }

    pub(crate) async fn list_executions(
        &self,
        input: ChannelMonitorExecutionListInput,
    ) -> Result<ChannelMonitorExecutionPage, ApplicationError> {
        let limit = input
            .limit
            .unwrap_or(DEFAULT_EXECUTION_LIMIT)
            .clamp(1, MAX_EXECUTION_LIMIT);
        let mut read = self.runtime.begin_read().await?;
        let mut items = self
            .store
            .list_execution_summaries(&mut read, &input, limit.saturating_add(1))
            .await?;
        let next_cursor = if items.len() > limit as usize {
            items.pop().map(|item| ChannelMonitorExecutionCursor {
                started_at_ms: item.started_at_ms.unwrap_or(item.planned_at_ms),
                execution_id: item.execution_id,
            })
        } else {
            None
        };
        Ok(ChannelMonitorExecutionPage { items, next_cursor })
    }

    pub(crate) async fn get_execution(
        &self,
        input: ChannelMonitorExecutionIdInput,
    ) -> Result<ChannelMonitorExecutionDetail, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .execution_detail(&mut read, &input.execution_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_attempt_history(
        &self,
        input: ChannelMonitorAttemptHistoryInput,
    ) -> Result<ChannelMonitorAttemptPage, ApplicationError> {
        let limit = input
            .limit
            .unwrap_or(DEFAULT_ATTEMPT_LIMIT)
            .clamp(1, MAX_ATTEMPT_LIMIT);
        let mut read = self.runtime.begin_read().await?;
        let mut items = self
            .store
            .attempt_history(&mut read, &input, limit.saturating_add(1))
            .await?;
        let next_cursor = if items.len() > limit as usize {
            items.pop().map(|item| ChannelMonitorAttemptCursor {
                started_at_ms: item.started_at_ms,
                attempt_id: item.attempt_id,
            })
        } else {
            None
        };
        Ok(ChannelMonitorAttemptPage { items, next_cursor })
    }
}

fn build_buckets(
    windows: &[BucketWindow],
    kind: ChannelStatusBucketKind,
    rollups: Option<&BTreeMap<i64, RollupCell>>,
    dirty_ranges: Option<&Vec<DirtyRange>>,
) -> Vec<ChannelStatusBucket> {
    windows
        .iter()
        .map(|window| {
            let dirty_without_rollup = dirty_ranges
                .is_some_and(|ranges| ranges.iter().any(|range| overlaps(window, range)));
            match rollups.and_then(|rollups| rollups.get(&window.start_ms)) {
                Some(cell) => bucket_from_rollup(window, kind, cell),
                None => ChannelStatusBucket {
                    kind,
                    start_ms: window.start_ms,
                    end_ms: window.end_ms,
                    state: if dirty_without_rollup {
                        ChannelStatusBucketState::Dirty
                    } else {
                        ChannelStatusBucketState::Missing
                    },
                    counts: ChannelStatusBucketCounts::default(),
                    strict_availability_bps: None,
                    effective_availability_bps: None,
                    p50_latency_ms: None,
                    p95_latency_ms: None,
                    failure_counts: BTreeMap::new(),
                    dirty: dirty_without_rollup,
                    corrupt: false,
                },
            }
        })
        .collect()
}

fn bucket_from_rollup(
    window: &BucketWindow,
    kind: ChannelStatusBucketKind,
    cell: &RollupCell,
) -> ChannelStatusBucket {
    let counts = bucket_counts(cell.counts);
    let state = if cell.dirty || cell.corrupt {
        ChannelStatusBucketState::Dirty
    } else {
        match counts.state() {
            BucketAvailabilityState::Missing => ChannelStatusBucketState::Missing,
            BucketAvailabilityState::SkippedOnly => ChannelStatusBucketState::SkippedOnly,
            BucketAvailabilityState::Available => ChannelStatusBucketState::Available,
            BucketAvailabilityState::Degraded => ChannelStatusBucketState::Degraded,
            BucketAvailabilityState::Unavailable => ChannelStatusBucketState::Unavailable,
        }
    };
    ChannelStatusBucket {
        kind,
        start_ms: window.start_ms,
        end_ms: window.end_ms,
        state,
        counts: cell.counts,
        strict_availability_bps: counts.strict_availability_bps(),
        effective_availability_bps: counts.effective_availability_bps(DEGRADED_WEIGHT_BPS),
        p50_latency_ms: cell.p50_latency_ms,
        p95_latency_ms: cell.p95_latency_ms,
        failure_counts: cell.failure_counts.clone(),
        dirty: cell.dirty,
        corrupt: cell.corrupt,
    }
}

fn summarize_selected_window(
    window: ChannelStatusWorkspaceWindow,
    recent: &[ChannelStatusRecentPoint],
    hourly: &[ChannelStatusBucket],
    daily: &[ChannelStatusBucket],
    latest: Option<&ChannelStatusLatestResult>,
) -> ChannelStatusWindowSummaryV2 {
    match window {
        ChannelStatusWorkspaceWindow::Recent => summarize_recent_window(window, recent, latest),
        ChannelStatusWorkspaceWindow::Last24h => summarize_bucket_window(window, hourly, latest),
        ChannelStatusWorkspaceWindow::Last7d => {
            summarize_bucket_window(window, select_tail(daily, 7), latest)
        }
        ChannelStatusWorkspaceWindow::Last30d => summarize_bucket_window(window, daily, latest),
    }
}

fn summarize_recent_window(
    window: ChannelStatusWorkspaceWindow,
    recent: &[ChannelStatusRecentPoint],
    latest: Option<&ChannelStatusLatestResult>,
) -> ChannelStatusWindowSummaryV2 {
    let counts = recent
        .iter()
        .fold(ChannelStatusBucketCounts::default(), |mut counts, point| {
            counts.total = counts.total.saturating_add(1);
            match point.outcome {
                ChannelStatusOutcome::Available => counts.available += 1,
                ChannelStatusOutcome::Degraded => counts.degraded += 1,
                ChannelStatusOutcome::Unavailable => counts.unavailable += 1,
                ChannelStatusOutcome::Skipped => counts.skipped += 1,
                ChannelStatusOutcome::Missing => {}
            }
            counts
        });
    let bucket_counts = bucket_counts(counts);
    ChannelStatusWindowSummaryV2 {
        window,
        bucket_kind: ChannelStatusBucketKind::Hour,
        start_ms: recent
            .last()
            .and_then(|point| point.checked_at_ms)
            .unwrap_or(0),
        end_ms: recent
            .first()
            .and_then(|point| point.checked_at_ms)
            .unwrap_or(0),
        counts,
        strict_availability_bps: bucket_counts.strict_availability_bps(),
        effective_availability_bps: bucket_counts.effective_availability_bps(DEGRADED_WEIGHT_BPS),
        latest_outcome: latest
            .map(|latest| latest.outcome)
            .unwrap_or(ChannelStatusOutcome::Missing),
        latest_checked_at_ms: latest.and_then(|latest| latest.finished_at_ms),
        dirty: false,
        corrupt: false,
    }
}

fn summarize_bucket_window(
    window: ChannelStatusWorkspaceWindow,
    buckets: &[ChannelStatusBucket],
    latest: Option<&ChannelStatusLatestResult>,
) -> ChannelStatusWindowSummaryV2 {
    let counts = buckets.iter().fold(
        ChannelStatusBucketCounts::default(),
        |mut counts, bucket| {
            counts.total = counts.total.saturating_add(bucket.counts.total);
            counts.available = counts.available.saturating_add(bucket.counts.available);
            counts.degraded = counts.degraded.saturating_add(bucket.counts.degraded);
            counts.unavailable = counts.unavailable.saturating_add(bucket.counts.unavailable);
            counts.skipped = counts.skipped.saturating_add(bucket.counts.skipped);
            counts
        },
    );
    let bucket_counts = bucket_counts(counts);
    ChannelStatusWindowSummaryV2 {
        window,
        bucket_kind: buckets
            .first()
            .map(|bucket| bucket.kind)
            .unwrap_or(ChannelStatusBucketKind::Day),
        start_ms: buckets.first().map(|bucket| bucket.start_ms).unwrap_or(0),
        end_ms: buckets.last().map(|bucket| bucket.end_ms).unwrap_or(0),
        counts,
        strict_availability_bps: bucket_counts.strict_availability_bps(),
        effective_availability_bps: bucket_counts.effective_availability_bps(DEGRADED_WEIGHT_BPS),
        latest_outcome: latest
            .map(|latest| latest.outcome)
            .unwrap_or(ChannelStatusOutcome::Missing),
        latest_checked_at_ms: latest.and_then(|latest| latest.finished_at_ms),
        dirty: buckets.iter().any(|bucket| bucket.dirty),
        corrupt: buckets.iter().any(|bucket| bucket.corrupt),
    }
}

fn latest_from_recent(point: &ChannelStatusRecentPoint) -> ChannelStatusLatestResult {
    ChannelStatusLatestResult {
        target_result_id: point.target_result_id.clone(),
        execution_id: point.execution_id.clone(),
        outcome: point.outcome,
        failure_kind: point.failure_kind.clone(),
        terminal_reason: point.terminal_reason.clone(),
        http_status: point.http_status,
        latency_ms: point.latency_ms,
        finished_at_ms: point.checked_at_ms,
        semantic_confidence: point.semantic_confidence.clone(),
        used_fallback: point.used_fallback,
        attempt_count: point.attempt_count,
        effective_model: point.effective_model.clone(),
    }
}

fn sort_rows(
    rows: &mut [ChannelStatusRow],
    field: ChannelStatusSortField,
    direction: ChannelStatusSortDirection,
) {
    rows.sort_by(|left, right| {
        let ordering = match field {
            ChannelStatusSortField::MonitorName => left
                .monitor
                .name
                .to_lowercase()
                .cmp(&right.monitor.name.to_lowercase())
                .then_with(|| left.row_key.cmp(&right.row_key)),
            ChannelStatusSortField::LatestCheckedAt => left
                .selected_window
                .latest_checked_at_ms
                .cmp(&right.selected_window.latest_checked_at_ms)
                .then_with(|| left.row_key.cmp(&right.row_key)),
            ChannelStatusSortField::Availability => left
                .selected_window
                .effective_availability_bps
                .cmp(&right.selected_window.effective_availability_bps)
                .then_with(|| left.row_key.cmp(&right.row_key)),
            ChannelStatusSortField::Latency => latest_latency(left)
                .cmp(&latest_latency(right))
                .then_with(|| left.row_key.cmp(&right.row_key)),
            ChannelStatusSortField::Status => left
                .selected_window
                .latest_outcome
                .cmp(&right.selected_window.latest_outcome)
                .then_with(|| left.row_key.cmp(&right.row_key)),
        };
        if matches!(direction, ChannelStatusSortDirection::Desc) {
            reverse_ordering(ordering)
        } else {
            ordering
        }
    });
}

fn aggregate_rows(total_rows: u32, rows: &[ChannelStatusRow]) -> ChannelStatusAggregate {
    let mut aggregate = ChannelStatusAggregate {
        total_rows,
        returned_rows: rows.len() as u32,
        ..ChannelStatusAggregate::default()
    };
    for row in rows {
        if row.running.is_some() {
            aggregate.running_rows = aggregate.running_rows.saturating_add(1);
        }
        if row.selected_window.dirty || row.selected_window.corrupt {
            aggregate.dirty_rows = aggregate.dirty_rows.saturating_add(1);
        }
        match row.selected_window.latest_outcome {
            ChannelStatusOutcome::Available => aggregate.available_rows += 1,
            ChannelStatusOutcome::Degraded => aggregate.degraded_rows += 1,
            ChannelStatusOutcome::Unavailable => aggregate.unavailable_rows += 1,
            ChannelStatusOutcome::Skipped => aggregate.skipped_rows += 1,
            ChannelStatusOutcome::Missing => aggregate.missing_rows += 1,
        }
    }
    aggregate
}

fn freshness(rows: &[ChannelStatusRow]) -> ChannelStatusFreshness {
    let checked = rows
        .iter()
        .flat_map(|row| row.recent.iter().filter_map(|point| point.checked_at_ms))
        .collect::<Vec<_>>();
    ChannelStatusFreshness {
        newest_result_at_ms: checked.iter().max().copied(),
        oldest_result_at_ms: checked.iter().min().copied(),
        has_dirty_rollups: rows.iter().any(|row| {
            row.hourly_buckets.iter().any(|bucket| bucket.dirty)
                || row.daily_buckets.iter().any(|bucket| bucket.dirty)
        }),
        has_corrupt_rollups: rows.iter().any(|row| {
            row.hourly_buckets.iter().any(|bucket| bucket.corrupt)
                || row.daily_buckets.iter().any(|bucket| bucket.corrupt)
        }),
        running_execution_count: rows.iter().filter(|row| row.running.is_some()).count() as u32,
    }
}

fn timezone_from_bucket_set(
    set: &crate::application::monitoring::buckets::BucketWindowSet,
) -> ChannelStatusTimezone {
    let (source, requested_id) = match &set.timezone_source {
        BucketTimezoneSource::Iana => (ChannelStatusTimezoneSource::Iana, None),
        BucketTimezoneSource::UtcFallback { requested } => {
            (ChannelStatusTimezoneSource::UtcFallback, requested.clone())
        }
    };
    ChannelStatusTimezone {
        id: set.timezone_id.clone(),
        source,
        requested_id,
    }
}

fn boundaries(
    windows: &[BucketWindow],
    kind: ChannelStatusBucketKind,
) -> Vec<ChannelStatusBucketBoundary> {
    windows
        .iter()
        .map(|window| {
            debug_assert!(matches!(
                (kind, window.kind),
                (ChannelStatusBucketKind::Hour, BucketWindowKind::Hour)
                    | (ChannelStatusBucketKind::Day, BucketWindowKind::Day)
            ));
            ChannelStatusBucketBoundary {
                kind,
                start_ms: window.start_ms,
                end_ms: window.end_ms,
                label: window.label.clone(),
            }
        })
        .collect()
}

fn bucket_counts(counts: ChannelStatusBucketCounts) -> BucketCounts {
    BucketCounts {
        available_count: counts.available,
        degraded_count: counts.degraded,
        unavailable_count: counts.unavailable,
        skipped_count: counts.skipped,
    }
}

fn row_key(monitor_id: &str, station_key_id: Option<&str>) -> String {
    format!("{}|{}", monitor_id, station_key_id.unwrap_or(""))
}

fn latest_latency(row: &ChannelStatusRow) -> Option<i64> {
    row.latest.as_ref().and_then(|latest| latest.latency_ms)
}

fn reverse_ordering(ordering: Ordering) -> Ordering {
    match ordering {
        Ordering::Less => Ordering::Greater,
        Ordering::Equal => Ordering::Equal,
        Ordering::Greater => Ordering::Less,
    }
}

fn overlaps(window: &BucketWindow, range: &DirtyRange) -> bool {
    range.start_ms < window.end_ms && range.end_ms > window.start_ms
}

fn min_window_start(hourly: &[BucketWindow], daily: &[BucketWindow]) -> i64 {
    hourly
        .first()
        .into_iter()
        .chain(daily.first())
        .map(|window| window.start_ms)
        .min()
        .unwrap_or_default()
}

fn max_window_end(hourly: &[BucketWindow], daily: &[BucketWindow]) -> i64 {
    hourly
        .last()
        .into_iter()
        .chain(daily.last())
        .map(|window| window.end_ms)
        .max()
        .unwrap_or_default()
}

fn select_tail<T>(items: &[T], count: usize) -> &[T] {
    if items.len() <= count {
        items
    } else {
        &items[items.len() - count..]
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::models::monitoring::ChannelStatusEndpointPing;
    use crate::persistence::runtime::PersistenceRuntime;

    struct FixedClock;

    impl Clock for FixedClock {
        fn now_utc(&self) -> chrono::DateTime<Utc> {
            Utc.timestamp_millis_opt(1_700_000_000_000)
                .single()
                .expect("timestamp")
        }
    }

    #[test]
    fn bucket_window_summary_uses_latest_probe_for_current_status() {
        let bucket = ChannelStatusBucket {
            kind: ChannelStatusBucketKind::Hour,
            start_ms: 0,
            end_ms: 3_600_000,
            state: ChannelStatusBucketState::Degraded,
            counts: ChannelStatusBucketCounts {
                total: 2,
                available: 1,
                degraded: 1,
                unavailable: 0,
                skipped: 0,
            },
            strict_availability_bps: Some(5_000),
            effective_availability_bps: Some(7_500),
            p50_latency_ms: Some(100),
            p95_latency_ms: Some(200),
            failure_counts: BTreeMap::new(),
            dirty: false,
            corrupt: false,
        };
        let latest = ChannelStatusLatestResult {
            target_result_id: "target-latest".to_string(),
            execution_id: "execution-latest".to_string(),
            outcome: ChannelStatusOutcome::Available,
            failure_kind: None,
            terminal_reason: None,
            http_status: Some(200),
            latency_ms: Some(100),
            finished_at_ms: Some(3_500_000),
            semantic_confidence: "protocol_validated".to_string(),
            used_fallback: false,
            attempt_count: 1,
            effective_model: Some("gpt-test".to_string()),
        };

        let summary = summarize_bucket_window(
            ChannelStatusWorkspaceWindow::Last24h,
            &[bucket],
            Some(&latest),
        );

        assert_eq!(summary.latest_outcome, ChannelStatusOutcome::Available);
        assert_eq!(summary.latest_checked_at_ms, Some(3_500_000));
    }

    #[tokio::test]
    async fn workspace_loads_when_a_monitor_produces_scoped_queries() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("status-workspace.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("runtime");
        runtime
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        r#"
                        INSERT INTO stations (
                            id, name, station_type, website_url, api_base_url,
                            created_at, updated_at
                        ) VALUES (
                            'station-1', 'Station', 'openai-compatible',
                            'https://example.test', 'https://example.test/v1', '1', '1'
                        )
                        "#,
                    )
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        r#"
                        INSERT INTO endpoint_health_snapshot (
                            station_id, endpoint_revision, status, latency_ms,
                            checked_at, error_summary, updated_at
                        ) VALUES (
                            'station-1', 1, 'success', 48,
                            '1700000000000', NULL, '1700000000000'
                        )
                        "#,
                    )
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        r#"
                        INSERT INTO station_keys (
                            id, station_id, name, group_name, group_binding_id,
                            created_at, updated_at
                        ) VALUES (
                            'key-1', 'station-1', 'Key', 'plus', 'binding-1', '1', '1'
                        )
                        "#,
                    )
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        r#"
                        INSERT INTO station_group_bindings (
                            id, station_id, station_key_id, binding_kind,
                            group_key_hash, group_name, binding_status,
                            inferred_group_category, confidence, created_at, updated_at
                        ) VALUES (
                            'binding-1', 'station-1', 'key-1', 'key_binding',
                            'group-hash-1', 'plus', 'bound', 'gpt', 1.0, '1', '1'
                        )
                        "#,
                    )
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        r#"
                        INSERT INTO channel_monitors (
                            id, name, target_type, station_id, station_key_id,
                            template_id, interval_seconds, timeout_seconds,
                            created_at, updated_at
                        ) VALUES (
                            'monitor-1', 'Monitor', 'station_key', 'station-1', 'key-1',
                            'builtin-openai-chat-low-token', 300, 30, '1', '1'
                        )
                        "#,
                    )
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("seed monitor");

        let query = ChannelStatusReadModelQuery::new(runtime.handle(), Arc::new(FixedClock));
        let workspace = query
            .load_workspace(ChannelStatusWorkspaceInput::default())
            .await
            .expect("load workspace");

        assert_eq!(workspace.rows.len(), 1);
        assert_eq!(workspace.rows[0].row_key, "monitor-1|key-1");
        assert_eq!(workspace.rows[0].target.group_name.as_deref(), Some("plus"));
        assert_eq!(
            workspace.rows[0].target.effective_group_category.as_deref(),
            Some("gpt")
        );
        assert_eq!(
            workspace.rows[0].target.endpoint_ping,
            Some(ChannelStatusEndpointPing {
                status: "success".to_string(),
                latency_ms: Some(48),
                checked_at_ms: Some(1_700_000_000_000),
            })
        );
        assert_eq!(workspace.aggregate.total_rows, 1);
        runtime.close().await.expect("close runtime");
    }
}
