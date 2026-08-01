use serde::{Deserialize, Serialize};

use crate::application::operational_facts::candidate_projector::RouteCandidateProjection;

pub(crate) const OPERATIONAL_DETAIL_VERSION: &str = "station_key_operational_detail_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationKeyOperationalDetailInput {
    pub(crate) station_key_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationKeyOperationalDetail {
    pub(crate) detail_version: &'static str,
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) facts: Vec<OperationalDetailFact>,
    pub(crate) lazy_history_available: bool,
    pub(crate) read_model_status: OperationalDetailReadModelStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationalDetailFact {
    pub(crate) scope: String,
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) source: String,
    pub(crate) freshness: String,
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperationalDetailReadModelStatus {
    Available,
    Unavailable,
}

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "projection-backed operational detail is the Task 9 preview seam before full production cutover"
    )
)]
pub(crate) fn operational_detail_from_projection(
    projection: &RouteCandidateProjection,
) -> StationKeyOperationalDetail {
    StationKeyOperationalDetail {
        detail_version: OPERATIONAL_DETAIL_VERSION,
        station_key_id: projection.identity.station_key_id.clone(),
        station_id: projection.identity.station_id.clone(),
        endpoint_revision: projection.identity.endpoint_revision,
        facts: vec![
            OperationalDetailFact {
                scope: "group".to_string(),
                name: "routing_group".to_string(),
                value: projection
                    .group
                    .as_ref()
                    .map(|group| group.stable_key.clone())
                    .unwrap_or_else(|| "missing".to_string()),
                source: "group_projector".to_string(),
                freshness: "snapshot".to_string(),
                reason: projection
                    .group
                    .as_ref()
                    .map(|group| group.reason.to_string()),
            },
            OperationalDetailFact {
                scope: "pricing".to_string(),
                name: "cost_basis".to_string(),
                value: format!("{:?}", projection.pricing.basis),
                source: projection
                    .pricing
                    .source_chain
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "pricing_projector".to_string()),
                freshness: projection
                    .pricing
                    .observed_at
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                reason: projection.pricing.reason.map(ToString::to_string),
            },
            OperationalDetailFact {
                scope: "health".to_string(),
                name: "station_key_health".to_string(),
                value: format!("{:?}", projection.health.station_key),
                source: "health_projector".to_string(),
                freshness: "snapshot_plus_runtime_overlay".to_string(),
                reason: projection
                    .health
                    .reasons
                    .first()
                    .map(|value| value.to_string()),
            },
            OperationalDetailFact {
                scope: "capacity".to_string(),
                name: "capacity_mode".to_string(),
                value: "snapshot_only".to_string(),
                source: "capacity_snapshot".to_string(),
                freshness: "runtime_snapshot".to_string(),
                reason: None,
            },
        ],
        lazy_history_available: true,
        read_model_status: OperationalDetailReadModelStatus::Available,
    }
}

pub(crate) fn unavailable_operational_detail(
    station_key_id: String,
) -> StationKeyOperationalDetail {
    StationKeyOperationalDetail {
        detail_version: OPERATIONAL_DETAIL_VERSION,
        station_key_id,
        station_id: String::new(),
        endpoint_revision: 0,
        facts: Vec::new(),
        lazy_history_available: false,
        read_model_status: OperationalDetailReadModelStatus::Unavailable,
    }
}
