use serde::{Deserialize, Serialize};

pub const DASHBOARD_METRICS_SCHEMA_VERSION: u16 = 1;
pub const DASHBOARD_RECENT_WINDOW_MINUTES: u16 = 5;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardRequestMetricsInput {
    pub local_day_start_ms: i64,
    pub local_day_end_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardLiveRequestMetricsSnapshot {
    pub schema_version: u16,
    pub captured_at_ms: i64,
    pub recent: DashboardRecentMetrics,
    pub today: DashboardPeriodMetrics,
    pub today_costs: DashboardCostMetrics,
    pub data_quality: DashboardLiveMetricsDataQuality,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardCumulativeRequestMetricsSnapshot {
    pub schema_version: u16,
    pub captured_at_ms: i64,
    pub lifetime: DashboardPeriodMetrics,
    pub lifetime_costs: DashboardCostMetrics,
    pub data_quality: DashboardMetricsDataQuality,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardRecentMetrics {
    pub period: DashboardPeriodMetrics,
    pub window_minutes: u16,
    pub rpm: f64,
    pub tpm: f64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardPeriodMetrics {
    pub request_count: u64,
    pub terminal_count: u64,
    pub success_count: u64,
    pub failed_count: u64,
    pub interrupted_count: u64,
    pub in_progress_count: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub known_usage_request_count: u64,
    pub missing_usage_request_count: u64,
    pub stream_usage_missing_request_count: u64,
    pub not_applicable_usage_request_count: u64,
    pub unknown_usage_request_count: u64,
    pub total_duration_ms: u64,
    pub duration_sample_count: u64,
    pub first_token_total_ms: u64,
    pub first_token_sample_count: u64,
    pub avg_total_duration_ms: Option<f64>,
    pub avg_first_token_ms: Option<f64>,
}

impl DashboardPeriodMetrics {
    pub fn finish_averages(&mut self) {
        self.avg_total_duration_ms = average(self.total_duration_ms, self.duration_sample_count);
        self.avg_first_token_ms = average(self.first_token_total_ms, self.first_token_sample_count);
    }
}

impl DashboardRecentMetrics {
    pub fn from_period(mut period: DashboardPeriodMetrics) -> Self {
        period.finish_averages();
        let divisor = f64::from(DASHBOARD_RECENT_WINDOW_MINUTES);
        Self {
            rpm: period.request_count as f64 / divisor,
            tpm: period.total_tokens as f64 / divisor,
            period,
            window_minutes: DASHBOARD_RECENT_WINDOW_MINUTES,
        }
    }
}

fn average(total: u64, samples: u64) -> Option<f64> {
    (samples > 0).then(|| total as f64 / samples as f64)
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardCostTotal {
    pub currency: String,
    pub amount_micro: i64,
    pub request_count: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardCostMetrics {
    pub totals: Vec<DashboardCostTotal>,
    pub cost_totals_complete: bool,
    pub complete_single_currency_count: u64,
    pub complete_mixed_currency_count: u64,
    pub incomplete_count: u64,
    pub not_applicable_count: u64,
    pub no_attempts_count: u64,
    pub legacy_or_missing_aggregate_count: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardMetricsDataQuality {
    pub invalid_timestamp_count: u64,
    pub future_timestamp_count: u64,
    pub invalid_duration_count: u64,
    pub unknown_lifecycle_count: u64,
    pub corrupt_cost_aggregate_count: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardLiveMetricsDataQuality {
    pub invalid_duration_count: u64,
    pub unknown_lifecycle_count: u64,
    pub corrupt_cost_aggregate_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_preserve_zero_samples_and_zero_values() {
        let mut empty = DashboardPeriodMetrics::default();
        empty.finish_averages();
        assert_eq!(empty.avg_total_duration_ms, None);

        let mut zero = DashboardPeriodMetrics {
            total_duration_ms: 0,
            duration_sample_count: 1,
            ..Default::default()
        };
        zero.finish_averages();
        assert_eq!(zero.avg_total_duration_ms, Some(0.0));
    }

    #[test]
    fn recent_rates_are_not_capped_by_log_page_size() {
        let recent = DashboardRecentMetrics::from_period(DashboardPeriodMetrics {
            request_count: 501,
            total_tokens: 3_000,
            ..Default::default()
        });
        assert_eq!(recent.rpm, 100.2);
        assert_eq!(recent.tpm, 600.0);
    }
}
