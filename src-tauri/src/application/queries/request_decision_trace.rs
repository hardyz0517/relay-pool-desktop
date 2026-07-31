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
    pub(crate) timeline: Vec<RequestDecisionTimelineItem>,
    pub(crate) planning_rounds: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequestDecisionTimelineKind {
    LegacySummary,
    PlanningRound,
    SlotWait,
    AttemptProtocol,
    Fallback,
    DownstreamDelivery,
    CostAggregate,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequestDecisionTimelineStatus {
    Available,
    LegacySummary,
    Unavailable,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestDecisionTimelineItem {
    pub(crate) ordinal: u32,
    pub(crate) kind: RequestDecisionTimelineKind,
    pub(crate) status: RequestDecisionTimelineStatus,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) detail_code: String,
    pub(crate) route_policy: Option<String>,
    pub(crate) route_reason: Option<String>,
    pub(crate) station_key_id: Option<String>,
    pub(crate) station_id: Option<String>,
    pub(crate) attempt_count: Option<i64>,
    pub(crate) fallback_count: Option<i64>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) cost_status: Option<String>,
    pub(crate) estimated_total_cost: Option<f64>,
    pub(crate) cost_currency: Option<String>,
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
            timeline: vec![RequestDecisionTimelineItem {
                ordinal: 1,
                kind: RequestDecisionTimelineKind::Unavailable,
                status: RequestDecisionTimelineStatus::Unavailable,
                title: "Trace unavailable".to_string(),
                summary:
                    "No matching request log was found in the bounded local read model window."
                        .to_string(),
                detail_code: "trace_unavailable".to_string(),
                route_policy: None,
                route_reason: None,
                station_key_id: None,
                station_id: None,
                attempt_count: None,
                fallback_count: None,
                duration_ms: None,
                cost_status: None,
                estimated_total_cost: None,
                cost_currency: None,
            }],
            planning_rounds: Vec::new(),
        };
    };
    let legacy_summary = LegacyDecisionSummary {
        route_policy: log.route_policy.clone(),
        route_reason: log.route_reason.clone(),
        station_key_id: log.station_key_id.clone(),
        station_id: log.station_id.clone(),
        fallback_count: log.fallback_count,
    };
    let timeline = timeline_from_legacy_log(&log);
    RequestDecisionTrace {
        trace_version: REQUEST_DECISION_TRACE_VERSION,
        request_log_id: log.id,
        status: RequestDecisionTraceStatus::LegacySummary,
        reason: "legacy_summary_only_before_cutover".to_string(),
        legacy_summary: Some(legacy_summary),
        timeline,
        planning_rounds: Vec::new(),
    }
}

fn timeline_from_legacy_log(log: &RequestLog) -> Vec<RequestDecisionTimelineItem> {
    vec![
        RequestDecisionTimelineItem {
            ordinal: 1,
            kind: RequestDecisionTimelineKind::LegacySummary,
            status: RequestDecisionTimelineStatus::LegacySummary,
            title: "Legacy routing summary".to_string(),
            summary: format!(
                "Policy {} selected {} with {} fallback(s).",
                log.route_policy.as_deref().unwrap_or("unknown"),
                log.station_key_id.as_deref().unwrap_or("none"),
                log.fallback_count
            ),
            detail_code: "legacy_summary_only_before_cutover".to_string(),
            route_policy: log.route_policy.clone(),
            route_reason: log.route_reason.clone(),
            station_key_id: log.station_key_id.clone(),
            station_id: log.station_id.clone(),
            attempt_count: log.attempt_count,
            fallback_count: Some(log.fallback_count),
            duration_ms: log.duration_ms,
            cost_status: log.cost_status.clone(),
            estimated_total_cost: log.estimated_total_cost,
            cost_currency: log.cost_currency.clone(),
        },
        RequestDecisionTimelineItem {
            ordinal: 2,
            kind: RequestDecisionTimelineKind::PlanningRound,
            status: RequestDecisionTimelineStatus::Unavailable,
            title: "Planning rounds".to_string(),
            summary: "Typed planning rounds are unavailable for legacy request logs.".to_string(),
            detail_code: "planning_rounds_unavailable_before_cutover".to_string(),
            route_policy: log.route_policy.clone(),
            route_reason: log.route_reason.clone(),
            station_key_id: None,
            station_id: None,
            attempt_count: None,
            fallback_count: None,
            duration_ms: None,
            cost_status: None,
            estimated_total_cost: None,
            cost_currency: None,
        },
        RequestDecisionTimelineItem {
            ordinal: 3,
            kind: RequestDecisionTimelineKind::SlotWait,
            status: if log.route_wait_ms.is_some() {
                RequestDecisionTimelineStatus::Available
            } else {
                RequestDecisionTimelineStatus::Unavailable
            },
            title: "Slot / wait".to_string(),
            summary: log
                .route_wait_ms
                .map(|duration_ms| format!("Route wait completed in {duration_ms} ms."))
                .unwrap_or_else(|| {
                    "Slot wait details are unavailable for this legacy request log.".to_string()
                }),
            detail_code: if log.route_wait_ms.is_some() {
                "route_wait_recorded"
            } else {
                "slot_wait_unavailable_before_cutover"
            }
            .to_string(),
            route_policy: None,
            route_reason: None,
            station_key_id: None,
            station_id: None,
            attempt_count: None,
            fallback_count: None,
            duration_ms: log.route_wait_ms,
            cost_status: None,
            estimated_total_cost: None,
            cost_currency: None,
        },
        RequestDecisionTimelineItem {
            ordinal: 4,
            kind: RequestDecisionTimelineKind::AttemptProtocol,
            status: RequestDecisionTimelineStatus::LegacySummary,
            title: "Attempt protocol".to_string(),
            summary: format!(
                "Legacy request status {}; detailed attempt protocol rows are unavailable.",
                log.status
            ),
            detail_code: "attempt_protocol_legacy_summary".to_string(),
            route_policy: None,
            route_reason: None,
            station_key_id: log.station_key_id.clone(),
            station_id: log.station_id.clone(),
            attempt_count: log.attempt_count,
            fallback_count: Some(log.fallback_count),
            duration_ms: log.upstream_headers_ms.or(log.first_token_ms),
            cost_status: None,
            estimated_total_cost: None,
            cost_currency: None,
        },
        RequestDecisionTimelineItem {
            ordinal: 5,
            kind: RequestDecisionTimelineKind::Fallback,
            status: if log.fallback_count > 0 {
                RequestDecisionTimelineStatus::LegacySummary
            } else {
                RequestDecisionTimelineStatus::Skipped
            },
            title: "Fallback".to_string(),
            summary: if log.fallback_count > 0 {
                format!(
                    "Legacy log recorded {} fallback(s); per-round exclusion details are unavailable.",
                    log.fallback_count
                )
            } else {
                "No fallback recorded in the legacy request log.".to_string()
            },
            detail_code: if log.fallback_count > 0 {
                "fallback_legacy_summary"
            } else {
                "fallback_not_recorded"
            }
            .to_string(),
            route_policy: log.route_policy.clone(),
            route_reason: log.route_reason.clone(),
            station_key_id: log.station_key_id.clone(),
            station_id: log.station_id.clone(),
            attempt_count: log.attempt_count,
            fallback_count: Some(log.fallback_count),
            duration_ms: None,
            cost_status: None,
            estimated_total_cost: None,
            cost_currency: None,
        },
        RequestDecisionTimelineItem {
            ordinal: 6,
            kind: RequestDecisionTimelineKind::DownstreamDelivery,
            status: if log.completion_source.is_some() || log.lifecycle_status.is_some() {
                RequestDecisionTimelineStatus::LegacySummary
            } else {
                RequestDecisionTimelineStatus::Unavailable
            },
            title: "Downstream delivery".to_string(),
            summary: match (&log.completion_source, &log.lifecycle_status) {
                (Some(completion_source), Some(lifecycle_status)) => {
                    format!("Completion source {completion_source}; lifecycle {lifecycle_status}.")
                }
                (Some(completion_source), None) => {
                    format!("Completion source {completion_source}; lifecycle unavailable.")
                }
                (None, Some(lifecycle_status)) => {
                    format!("Lifecycle {lifecycle_status}; delivery terminal detail unavailable.")
                }
                (None, None) => {
                    "Downstream delivery details are unavailable for this legacy request log."
                        .to_string()
                }
            },
            detail_code: "downstream_delivery_legacy_summary".to_string(),
            route_policy: None,
            route_reason: None,
            station_key_id: None,
            station_id: None,
            attempt_count: None,
            fallback_count: None,
            duration_ms: log.duration_ms,
            cost_status: None,
            estimated_total_cost: None,
            cost_currency: None,
        },
        RequestDecisionTimelineItem {
            ordinal: 7,
            kind: RequestDecisionTimelineKind::CostAggregate,
            status: if log.cost_status.is_some() || log.estimated_total_cost.is_some() {
                RequestDecisionTimelineStatus::LegacySummary
            } else {
                RequestDecisionTimelineStatus::Unavailable
            },
            title: "Cost aggregate".to_string(),
            summary: match (
                log.cost_status.as_deref(),
                log.estimated_total_cost,
                log.cost_currency.as_deref(),
            ) {
                (Some(status), Some(total), Some(currency)) => {
                    format!("Legacy cost projection {status}: {total:.6} {currency}.")
                }
                (Some(status), Some(total), None) => {
                    format!("Legacy cost projection {status}: {total:.6}; currency unavailable.")
                }
                (Some(status), None, _) => {
                    format!("Legacy cost status {status}; aggregate amount unavailable.")
                }
                _ => "Cost aggregate is unavailable for this legacy request log.".to_string(),
            },
            detail_code: "cost_aggregate_legacy_summary".to_string(),
            route_policy: None,
            route_reason: None,
            station_key_id: log.station_key_id.clone(),
            station_id: log.station_id.clone(),
            attempt_count: log.attempt_count,
            fallback_count: Some(log.fallback_count),
            duration_ms: None,
            cost_status: log.cost_status.clone(),
            estimated_total_cost: log.estimated_total_cost,
            cost_currency: log.cost_currency.clone(),
        },
    ]
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
