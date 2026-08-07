use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::models::proxy::UpstreamApiFormat;

pub(crate) const ROUTING_RUNTIME_OVERLAY_VERSION: &str = "routing_runtime_overlay_v1";

pub(crate) trait RoutingRuntimeActivity: Send + Sync {
    fn active_for_station<'a>(
        &'a self,
        station_type: &'a str,
        station_id: &'a str,
        station_key_id: &'a str,
    ) -> BoxFuture<'a, Option<i64>>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingRuntimeOverlay {
    pub(crate) overlay_version: &'static str,
    pub(crate) sampled_at_ms: i64,
    pub(crate) revision: u64,
    pub(crate) candidates: Vec<RoutingRuntimeCandidateOverlay>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingRuntimeCandidateOverlay {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) in_flight: Option<i64>,
    pub(crate) health_state: String,
    pub(crate) cooldown_until: Option<String>,
}

/// Narrow runtime facts consumed by the overlay read model. This boundary
/// prevents the UI/read path from depending on the legacy executable
/// the legacy executable candidate compatibility DTO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingRuntimeCandidateFact {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) in_flight: Option<i64>,
    pub(crate) health_state: String,
    pub(crate) cooldown_until: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingMonitoringTargetSnapshot {
    pub(crate) station_id: String,
    pub(crate) station_key_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) api_base_url: String,
    pub(crate) upstream_api_format: UpstreamApiFormat,
    pub(crate) supports_chat_completions: bool,
    pub(crate) supports_responses: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingMonitoringTargetFacts {
    pub(crate) station_id: String,
    pub(crate) station_key_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) api_base_url: String,
    pub(crate) upstream_api_format: UpstreamApiFormat,
    pub(crate) supports_chat_completions: bool,
    pub(crate) supports_responses: bool,
}

pub(crate) fn runtime_overlay_from_candidates(
    candidates: Vec<RoutingRuntimeCandidateFact>,
    sampled_at_ms: i64,
    revision: u64,
    limit: usize,
) -> RoutingRuntimeOverlay {
    let limit = limit.clamp(1, 1024);
    RoutingRuntimeOverlay {
        overlay_version: ROUTING_RUNTIME_OVERLAY_VERSION,
        sampled_at_ms,
        revision,
        candidates: candidates
            .into_iter()
            .take(limit)
            .map(|candidate| RoutingRuntimeCandidateOverlay {
                station_key_id: candidate.station_key_id,
                station_id: candidate.station_id,
                endpoint_revision: candidate.endpoint_revision,
                in_flight: candidate.in_flight,
                health_state: candidate.health_state,
                cooldown_until: candidate.cooldown_until,
            })
            .collect(),
    }
}

pub(crate) fn monitoring_target_snapshots_from_facts(
    facts: Vec<RoutingMonitoringTargetFacts>,
) -> Vec<RoutingMonitoringTargetSnapshot> {
    facts
        .into_iter()
        .map(|facts| RoutingMonitoringTargetSnapshot {
            station_id: facts.station_id,
            station_key_id: facts.station_key_id,
            endpoint_revision: facts.endpoint_revision,
            api_base_url: facts.api_base_url,
            upstream_api_format: facts.upstream_api_format,
            supports_chat_completions: facts.supports_chat_completions,
            supports_responses: facts.supports_responses,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitoring_target_snapshot_keeps_only_monitoring_endpoint_facts() {
        let snapshots =
            monitoring_target_snapshots_from_facts(vec![RoutingMonitoringTargetFacts {
                station_key_id: "key-1".to_string(),
                station_id: "station-1".to_string(),
                endpoint_revision: 7,
                api_base_url: "https://station.example/v1".to_string(),
                upstream_api_format: UpstreamApiFormat::OpenAiResponses,
                supports_chat_completions: false,
                supports_responses: true,
            }]);

        assert_eq!(
            snapshots,
            vec![RoutingMonitoringTargetSnapshot {
                station_id: "station-1".to_string(),
                station_key_id: "key-1".to_string(),
                endpoint_revision: 7,
                api_base_url: "https://station.example/v1".to_string(),
                upstream_api_format: UpstreamApiFormat::OpenAiResponses,
                supports_chat_completions: false,
                supports_responses: true,
            }]
        );
    }
}
