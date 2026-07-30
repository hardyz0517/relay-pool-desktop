use serde::{Deserialize, Serialize};

use crate::models::proxy::RequestLog;

pub(crate) const REQUEST_DECISION_TRACE_VERSION: &str = "request_decision_trace_v1";
pub(crate) const RECENT_ROUTE_DECISION_PAGE_VERSION: &str = "recent_route_decisions_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentRouteDecisionsInput {
    pub(crate) limit: Option<usize>,
    pub(crate) cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentRouteDecisionsPage {
    pub(crate) page_version: &'static str,
    pub(crate) decisions: Vec<RecentRouteDecisionSummary>,
    pub(crate) next_cursor: Option<String>,
    pub(crate) read_model_status: RouteDecisionReadModelStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RouteDecisionReadModelStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentRouteDecisionSummary {
    pub(crate) request_log_id: String,
    pub(crate) request_id: Option<String>,
    pub(crate) created_at: String,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) endpoint: String,
    pub(crate) model: Option<String>,
    pub(crate) status: String,
    pub(crate) lifecycle_status: Option<String>,
    pub(crate) station_key_id: Option<String>,
    pub(crate) station_id: Option<String>,
    pub(crate) route_policy: Option<String>,
    pub(crate) route_reason: Option<String>,
    pub(crate) fallback_count: i64,
    pub(crate) cost_status: Option<String>,
    pub(crate) estimated_total_cost: Option<f64>,
    pub(crate) cost_currency: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequestDecisionTraceStatus {
    LegacySummary,
    TraceUnavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestDecisionTrace {
    pub(crate) trace_version: &'static str,
    pub(crate) request_log_id: String,
    pub(crate) status: RequestDecisionTraceStatus,
    pub(crate) reason: String,
    pub(crate) legacy_summary: Option<LegacyDecisionSummary>,
    pub(crate) planning_rounds: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyDecisionSummary {
    pub(crate) route_policy: Option<String>,
    pub(crate) route_reason: Option<String>,
    pub(crate) station_key_id: Option<String>,
    pub(crate) station_id: Option<String>,
    pub(crate) fallback_count: i64,
}

pub(crate) fn recent_route_decisions_from_logs(
    logs: Vec<RequestLog>,
    input: RecentRouteDecisionsInput,
) -> RecentRouteDecisionsPage {
    let limit = input.limit.unwrap_or(50).clamp(1, 200);
    let start = input
        .cursor
        .as_deref()
        .and_then(|cursor| cursor.strip_prefix("offset:"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut decisions = logs
        .into_iter()
        .filter(|log| log.route_policy.as_deref() != Some("channel_monitor"))
        .skip(start)
        .take(limit.saturating_add(1))
        .map(route_decision_summary_from_log)
        .collect::<Vec<_>>();
    let has_more = decisions.len() > limit;
    if has_more {
        decisions.truncate(limit);
    }
    let next_cursor = has_more.then(|| format!("offset:{}", start + decisions.len()));
    RecentRouteDecisionsPage {
        page_version: RECENT_ROUTE_DECISION_PAGE_VERSION,
        decisions,
        next_cursor,
        read_model_status: RouteDecisionReadModelStatus::Available,
    }
}

pub(crate) fn decision_trace_from_legacy_log(log: Option<RequestLog>) -> RequestDecisionTrace {
    let Some(log) = log else {
        return RequestDecisionTrace {
            trace_version: REQUEST_DECISION_TRACE_VERSION,
            request_log_id: String::new(),
            status: RequestDecisionTraceStatus::TraceUnavailable,
            reason: "trace_unavailable".to_string(),
            legacy_summary: None,
            planning_rounds: Vec::new(),
        };
    };
    RequestDecisionTrace {
        trace_version: REQUEST_DECISION_TRACE_VERSION,
        request_log_id: log.id,
        status: RequestDecisionTraceStatus::LegacySummary,
        reason: "legacy_summary_only_before_cutover".to_string(),
        legacy_summary: Some(LegacyDecisionSummary {
            route_policy: log.route_policy,
            route_reason: log.route_reason,
            station_key_id: log.station_key_id,
            station_id: log.station_id,
            fallback_count: log.fallback_count,
        }),
        planning_rounds: Vec::new(),
    }
}

fn route_decision_summary_from_log(log: RequestLog) -> RecentRouteDecisionSummary {
    RecentRouteDecisionSummary {
        request_log_id: log.id,
        request_id: log.request_id,
        created_at: log.created_at,
        started_at: log.started_at,
        finished_at: log.finished_at,
        duration_ms: log.duration_ms,
        endpoint: log.path,
        model: log.model,
        status: log.status,
        lifecycle_status: log.lifecycle_status,
        station_key_id: log.station_key_id,
        station_id: log.station_id,
        route_policy: log.route_policy,
        route_reason: log.route_reason,
        fallback_count: log.fallback_count,
        cost_status: log.cost_status,
        estimated_total_cost: log.estimated_total_cost,
        cost_currency: log.cost_currency,
    }
}
