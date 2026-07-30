use serde::{Deserialize, Serialize};

use crate::models::routing::RuntimeRoutingCandidate;

pub(crate) const ROUTING_RUNTIME_OVERLAY_VERSION: &str = "routing_runtime_overlay_v1";

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

pub(crate) fn runtime_overlay_from_candidates(
    candidates: Vec<RuntimeRoutingCandidate>,
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
            .map(|candidate| {
                let cooldown_until = candidate
                    .health
                    .as_ref()
                    .and_then(|health| health.cooldown_until.clone());
                let health_state = candidate
                    .health
                    .as_ref()
                    .map(|health| {
                        if health.cooldown_until.is_some() {
                            "cooldown"
                        } else if health.consecutive_failures > 0 {
                            "degraded"
                        } else {
                            "ready"
                        }
                    })
                    .unwrap_or("unknown")
                    .to_string();
                RoutingRuntimeCandidateOverlay {
                    station_key_id: candidate.station_key_id,
                    station_id: candidate.station_id,
                    endpoint_revision: candidate.station_endpoint_revision,
                    in_flight: candidate.load_factor,
                    health_state,
                    cooldown_until,
                }
            })
            .collect(),
    }
}
