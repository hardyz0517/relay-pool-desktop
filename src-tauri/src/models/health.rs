use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthObservationSource {
    ProxyRequest,
    SyntheticMonitor,
    ManualConnectivity,
}

impl HealthObservationSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ProxyRequest => "proxy",
            Self::SyntheticMonitor => "monitoring",
            Self::ManualConnectivity => "manual",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthObservationOutcome {
    Success,
    ObserveFailure,
    Cooldown,
    HardFail,
    Skipped,
    Neutral,
}

impl HealthObservationOutcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::ObserveFailure => "observe_failure",
            Self::Cooldown => "cooldown",
            Self::HardFail => "hard_fail",
            Self::Skipped => "skipped",
            Self::Neutral => "neutral",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthWritebackMode {
    Disabled,
    ObserveOnly,
    Authoritative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TrafficEquivalence {
    RealUserTraffic,
    SyntheticStandard,
    SyntheticCliCompat,
    Diagnostic,
}

impl TrafficEquivalence {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::RealUserTraffic => "real_user_traffic",
            Self::SyntheticStandard => "synthetic_standard",
            Self::SyntheticCliCompat => "synthetic_cli_compat",
            Self::Diagnostic => "diagnostic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HealthObservation {
    pub(crate) id: String,
    pub(crate) station_key_id: String,
    pub(crate) target_result_id: Option<String>,
    pub(crate) source: HealthObservationSource,
    pub(crate) source_event_id: String,
    pub(crate) observed_at_ms: i64,
    pub(crate) endpoint_revision: i64,
    pub(crate) outcome: HealthObservationOutcome,
    pub(crate) failure_kind: Option<String>,
    pub(crate) latency_ms: Option<i64>,
    pub(crate) retry_after_ms: Option<i64>,
    pub(crate) error_summary: Option<String>,
    pub(crate) writeback_mode: HealthWritebackMode,
    pub(crate) traffic_equivalence: TrafficEquivalence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StationKeyHealthSnapshot {
    pub(crate) station_key_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) last_success_at: Option<String>,
    pub(crate) last_failure_at: Option<String>,
    pub(crate) consecutive_failures: i64,
    pub(crate) success_count: i64,
    pub(crate) failure_count: i64,
    pub(crate) total_duration_ms: i64,
    pub(crate) avg_latency_ms: Option<i64>,
    pub(crate) last_error_summary: Option<String>,
    pub(crate) cooldown_until: Option<String>,
    pub(crate) updated_at: String,
}

impl StationKeyHealthSnapshot {
    pub(crate) fn empty(
        station_key_id: impl Into<String>,
        endpoint_revision: i64,
        now_ms: i64,
    ) -> Self {
        Self {
            station_key_id: station_key_id.into(),
            endpoint_revision,
            last_success_at: None,
            last_failure_at: None,
            consecutive_failures: 0,
            success_count: 0,
            failure_count: 0,
            total_duration_ms: 0,
            avg_latency_ms: None,
            last_error_summary: None,
            cooldown_until: None,
            updated_at: now_ms.to_string(),
        }
    }
}
