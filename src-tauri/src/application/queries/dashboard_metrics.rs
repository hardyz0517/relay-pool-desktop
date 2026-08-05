use std::sync::Arc;

use crate::{
    application::{clock::Clock, error::ApplicationError},
    models::dashboard_metrics::{
        DASHBOARD_METRICS_SCHEMA_VERSION, DashboardCumulativeRequestMetricsSnapshot,
        DashboardLiveRequestMetricsSnapshot, DashboardRecentMetrics, DashboardRequestMetricsInput,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            dashboard_metrics_read::DashboardMetricsReadRepository,
            dashboard_metrics_rollup::{
                dashboard_rollups_rebuild_required, rebuild_dashboard_metric_rollups,
            },
        },
    },
};

const RECENT_WINDOW_MS: i64 = 5 * 60 * 1_000;
const MIN_DAY_MS: i64 = 22 * 60 * 60 * 1_000;
const MAX_DAY_MS: i64 = 26 * 60 * 60 * 1_000;

#[derive(Clone)]
pub(crate) struct DashboardMetricsQuery {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    repository: DashboardMetricsReadRepository,
}

impl DashboardMetricsQuery {
    pub(crate) fn new(runtime: PersistenceHandle, clock: Arc<dyn Clock>) -> Self {
        Self {
            runtime,
            clock,
            repository: DashboardMetricsReadRepository,
        }
    }

    pub(crate) async fn load_live(
        &self,
        input: DashboardRequestMetricsInput,
    ) -> Result<DashboardLiveRequestMetricsSnapshot, ApplicationError> {
        let captured_at_ms = self.clock.now_utc().timestamp_millis();
        validate_day_window(&input, captured_at_ms)?;
        let recent_start_ms = captured_at_ms
            .checked_sub(RECENT_WINDOW_MS)
            .ok_or(ApplicationError::ConstraintViolation)?;
        let end_exclusive_ms = fact_end_exclusive(captured_at_ms)?;
        self.repair_rollups_if_needed().await?;
        let mut read = self.runtime.begin_read().await?;
        let result = self
            .repository
            .load_live(
                &mut read,
                recent_start_ms,
                end_exclusive_ms,
                input.local_day_start_ms,
            )
            .await?;
        Ok(DashboardLiveRequestMetricsSnapshot {
            schema_version: DASHBOARD_METRICS_SCHEMA_VERSION,
            captured_at_ms,
            recent: DashboardRecentMetrics::from_period(result.recent),
            today: result.today,
            today_costs: result.today_costs,
            data_quality: result.data_quality,
        })
    }

    pub(crate) async fn load_cumulative(
        &self,
    ) -> Result<DashboardCumulativeRequestMetricsSnapshot, ApplicationError> {
        let captured_at_ms = self.clock.now_utc().timestamp_millis();
        let end_exclusive_ms = fact_end_exclusive(captured_at_ms)?;
        self.repair_rollups_if_needed().await?;
        let mut read = self.runtime.begin_read().await?;
        let result = self
            .repository
            .load_cumulative(&mut read, end_exclusive_ms)
            .await?;
        Ok(DashboardCumulativeRequestMetricsSnapshot {
            schema_version: DASHBOARD_METRICS_SCHEMA_VERSION,
            captured_at_ms,
            lifetime: result.lifetime,
            lifetime_costs: result.lifetime_costs,
            data_quality: result.data_quality,
        })
    }

    async fn repair_rollups_if_needed(&self) -> Result<(), ApplicationError> {
        let mut write = self.runtime.begin_write().await?;
        if dashboard_rollups_rebuild_required(write.connection()).await? {
            rebuild_dashboard_metric_rollups(write.connection()).await?;
        }
        write.commit().await?;
        Ok(())
    }
}

fn validate_day_window(
    input: &DashboardRequestMetricsInput,
    captured_at_ms: i64,
) -> Result<(), ApplicationError> {
    if input.local_day_start_ms <= 0
        || input.local_day_end_ms <= input.local_day_start_ms
        || input.local_day_start_ms > captured_at_ms
        || captured_at_ms >= input.local_day_end_ms
    {
        return Err(ApplicationError::ConstraintViolation);
    }
    let length = input
        .local_day_end_ms
        .checked_sub(input.local_day_start_ms)
        .ok_or(ApplicationError::ConstraintViolation)?;
    if !(MIN_DAY_MS..=MAX_DAY_MS).contains(&length) {
        return Err(ApplicationError::ConstraintViolation);
    }
    Ok(())
}

fn fact_end_exclusive(captured_at_ms: i64) -> Result<i64, ApplicationError> {
    if captured_at_ms <= 0 {
        return Err(ApplicationError::ConstraintViolation);
    }
    Ok(captured_at_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn input(start: i64, end: i64) -> DashboardRequestMetricsInput {
        DashboardRequestMetricsInput {
            local_day_start_ms: start,
            local_day_end_ms: end,
        }
    }

    #[test]
    fn day_start_is_inclusive_and_day_end_is_exclusive() {
        let start = 1_700_000_000_000;
        let end = start + 24 * 60 * 60 * 1_000;
        assert!(validate_day_window(&input(start, end), start).is_ok());
        assert!(validate_day_window(&input(start, end), end).is_err());
    }

    #[test]
    fn dst_day_lengths_are_bounded_to_22_through_26_hours() {
        let start = 1_700_000_000_000;
        for hours in [22, 23, 24, 25, 26] {
            assert!(
                validate_day_window(&input(start, start + hours * 60 * 60 * 1_000), start + 1)
                    .is_ok()
            );
        }
        assert!(
            validate_day_window(&input(start, start + 21 * 60 * 60 * 1_000), start + 1).is_err()
        );
        assert!(
            validate_day_window(&input(start, start + 27 * 60 * 60 * 1_000), start + 1).is_err()
        );
    }

    #[test]
    fn clock_input_is_millisecond_based() {
        assert!(
            Utc.timestamp_millis_opt(1_700_000_000_000)
                .single()
                .is_some()
        );
    }

    #[test]
    fn fact_windows_exclude_the_exact_capture_millisecond() {
        assert_eq!(
            fact_end_exclusive(1_700_000_000_000).unwrap(),
            1_700_000_000_000
        );
        assert!(fact_end_exclusive(0).is_err());
    }
}
