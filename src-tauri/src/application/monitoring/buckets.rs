use chrono::{Datelike, TimeZone, Timelike, Utc};
use chrono_tz::Tz;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BucketWindowKind {
    Hour,
    Day,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BucketTimezoneSource {
    Iana,
    UtcFallback { requested: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BucketWindow {
    pub(crate) kind: BucketWindowKind,
    pub(crate) start_ms: i64,
    pub(crate) end_ms: i64,
    pub(crate) label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BucketWindowSet {
    pub(crate) timezone_id: String,
    pub(crate) timezone_source: BucketTimezoneSource,
    pub(crate) windows: Vec<BucketWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BucketAvailabilityState {
    Missing,
    SkippedOnly,
    Available,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BucketCounts {
    pub(crate) available_count: u32,
    pub(crate) degraded_count: u32,
    pub(crate) unavailable_count: u32,
    pub(crate) skipped_count: u32,
}

pub(crate) const DEGRADED_WEIGHT_BPS: u32 = 5_000;
pub(crate) const AVAILABLE_BUCKET_THRESHOLD_BPS: u32 = 9_000;

impl BucketCounts {
    pub(crate) fn eligible_count(&self) -> u32 {
        self.available_count + self.degraded_count + self.unavailable_count
    }

    pub(crate) fn state(&self) -> BucketAvailabilityState {
        let eligible_count = self.eligible_count();
        if eligible_count == 0 {
            if self.skipped_count > 0 {
                BucketAvailabilityState::SkippedOnly
            } else {
                BucketAvailabilityState::Missing
            }
        } else if self.available_count == 0 && self.degraded_count == 0 {
            BucketAvailabilityState::Unavailable
        } else if self
            .effective_availability_bps(DEGRADED_WEIGHT_BPS)
            .is_some_and(|availability| availability >= AVAILABLE_BUCKET_THRESHOLD_BPS)
        {
            BucketAvailabilityState::Available
        } else {
            BucketAvailabilityState::Degraded
        }
    }

    pub(crate) fn strict_availability_bps(&self) -> Option<u32> {
        let eligible_count = self.eligible_count();
        (eligible_count > 0).then(|| self.available_count.saturating_mul(10_000) / eligible_count)
    }

    pub(crate) fn effective_availability_bps(&self, degraded_weight_bps: u32) -> Option<u32> {
        let eligible_count = self.eligible_count();
        if eligible_count == 0 {
            return None;
        }
        let capped_weight = degraded_weight_bps.min(10_000);
        Some(
            (self.available_count.saturating_mul(10_000)
                + self.degraded_count.saturating_mul(capped_weight))
                / eligible_count,
        )
    }
}

pub(crate) const RECENT_TARGET_RESULT_LIMIT: u32 = 60;

pub(crate) fn recent_target_result_limit() -> u32 {
    RECENT_TARGET_RESULT_LIMIT
}

pub(crate) fn hourly_bucket_windows(now_ms: i64, count: u32) -> BucketWindowSet {
    let bounded_count = count.clamp(1, 168);
    let current_hour_start = floor_ms(now_ms, 3_600_000);
    let first_start = current_hour_start - (i64::from(bounded_count) - 1) * 3_600_000;
    let windows = (0..bounded_count)
        .map(|index| {
            let start_ms = first_start + i64::from(index) * 3_600_000;
            let end_ms = start_ms + 3_600_000;
            BucketWindow {
                kind: BucketWindowKind::Hour,
                start_ms,
                end_ms,
                label: Utc
                    .timestamp_millis_opt(start_ms)
                    .single()
                    .map(|instant| format!("{:02}:00", instant.hour()))
                    .unwrap_or_else(|| "invalid".to_string()),
            }
        })
        .collect();

    BucketWindowSet {
        timezone_id: "UTC".to_string(),
        timezone_source: BucketTimezoneSource::UtcFallback { requested: None },
        windows,
    }
}

pub(crate) fn local_day_bucket_windows(
    now_ms: i64,
    days: u32,
    requested_timezone_id: Option<&str>,
) -> BucketWindowSet {
    let bounded_days = days.clamp(1, 366);
    let (timezone, timezone_id, timezone_source) = match requested_timezone_id
        .and_then(|id| id.parse::<Tz>().ok().map(|timezone| (id, timezone)))
    {
        Some((id, timezone)) => (timezone, id.to_string(), BucketTimezoneSource::Iana),
        None => (
            chrono_tz::UTC,
            "UTC".to_string(),
            BucketTimezoneSource::UtcFallback {
                requested: requested_timezone_id.map(str::to_string),
            },
        ),
    };

    let now_utc = Utc
        .timestamp_millis_opt(now_ms)
        .single()
        .unwrap_or_else(Utc::now);
    let current_local_date = now_utc.with_timezone(&timezone).date_naive();
    let first_date = current_local_date - chrono::Duration::days(i64::from(bounded_days - 1));

    let windows = (0..bounded_days)
        .filter_map(|index| {
            let date = first_date + chrono::Duration::days(i64::from(index));
            let next_date = date + chrono::Duration::days(1);
            let start = timezone
                .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
                .earliest()?;
            let end = timezone
                .with_ymd_and_hms(
                    next_date.year(),
                    next_date.month(),
                    next_date.day(),
                    0,
                    0,
                    0,
                )
                .earliest()?;
            Some(BucketWindow {
                kind: BucketWindowKind::Day,
                start_ms: start.timestamp_millis(),
                end_ms: end.timestamp_millis(),
                label: format!("{:02}-{:02}", date.month(), date.day()),
            })
        })
        .collect();

    BucketWindowSet {
        timezone_id,
        timezone_source,
        windows,
    }
}

fn floor_ms(value: i64, unit_ms: i64) -> i64 {
    value.div_euclid(unit_ms) * unit_ms
}
