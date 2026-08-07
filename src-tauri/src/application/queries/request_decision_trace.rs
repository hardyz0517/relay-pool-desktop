use crate::persistence::stores::routing_decisions::queries::{
    RouteCandidateDecisionRow, RoutingDecisionCursor, RoutingDecisionPage,
    RoutingDecisionSummaryRow,
};

pub(crate) const REQUEST_DECISION_TRACE_VERSION: &str = "request_decision_trace_v1";
pub(crate) const RECENT_ROUTE_DECISION_PAGE_VERSION: &str = "recent_route_decisions_v1";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentRouteDecisionsInput {
    pub(crate) limit: Option<usize>,
    pub(crate) cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentRouteDecisionsPage {
    pub(crate) page_version: &'static str,
    pub(crate) decisions: Vec<RecentRouteDecisionSummary>,
    pub(crate) next_cursor: Option<String>,
    pub(crate) read_model_status: RouteDecisionReadModelStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RouteDecisionReadModelStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequestDecisionTraceStatus {
    LegacySummary,
    TraceUnavailable,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequestDecisionTimelineStatus {
    Available,
    LegacySummary,
    Unavailable,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyDecisionSummary {
    pub(crate) route_policy: Option<String>,
    pub(crate) route_reason: Option<String>,
    pub(crate) station_key_id: Option<String>,
    pub(crate) station_id: Option<String>,
    pub(crate) fallback_count: i64,
}

pub fn decision_cursor(value: Option<&str>) -> Option<RoutingDecisionCursor> {
    let value = value?.strip_prefix("decision:")?;
    let (decided_at_ms, id) = value.split_once(':')?;
    Some(RoutingDecisionCursor {
        decided_at_ms: decided_at_ms.parse().ok()?,
        id: id.to_string(),
    })
}

fn encoded_cursor(cursor: Option<&RoutingDecisionCursor>) -> Option<String> {
    cursor.map(|cursor| format!("decision:{}:{}", cursor.decided_at_ms, cursor.id))
}

pub fn recent_route_decisions_from_page(page: RoutingDecisionPage) -> RecentRouteDecisionsPage {
    RecentRouteDecisionsPage {
        page_version: RECENT_ROUTE_DECISION_PAGE_VERSION,
        decisions: page.rows.into_iter().map(summary_from_decision).collect(),
        next_cursor: encoded_cursor(page.next_cursor.as_ref()),
        read_model_status: RouteDecisionReadModelStatus::Available,
    }
}

pub fn decision_trace_from_decision(
    summary: RoutingDecisionSummaryRow,
    candidates: Vec<RouteCandidateDecisionRow>,
) -> RequestDecisionTrace {
    let selected = summary.selected_station_key_id.clone();
    let timeline = vec![RequestDecisionTimelineItem {
        ordinal: 1,
        kind: RequestDecisionTimelineKind::PlanningRound,
        status: RequestDecisionTimelineStatus::Available,
        title: "Routing decision".to_string(),
        summary: format!(
            "{} selected {} candidate(s).",
            summary.ordering_profile, summary.candidate_count
        ),
        detail_code: "routing_decision_persisted".to_string(),
        route_policy: Some(summary.ordering_profile.clone()),
        route_reason: Some(summary.trace_status.clone()),
        station_key_id: selected.clone(),
        station_id: summary.selected_station_id.clone(),
        attempt_count: Some(
            candidates
                .iter()
                .filter(|candidate| candidate.attempted)
                .count() as i64,
        ),
        fallback_count: Some(0),
        duration_ms: None,
        cost_status: None,
        estimated_total_cost: None,
        cost_currency: None,
    }];
    let planning_rounds = candidates
        .into_iter()
        .map(|candidate| {
            serde_json::json!({
                "stationKeyId": candidate.station_key_id,
                "stationId": candidate.station_id,
                "selected": candidate.selected,
                "attempted": candidate.attempted,
                "retainedReason": candidate.retained_reason,
                "hardRejectionCode": candidate.hard_rejection_code,
                "costBasis": candidate.cost_basis,
                "evidence": candidate.evidence,
            })
        })
        .collect();
    RequestDecisionTrace {
        trace_version: REQUEST_DECISION_TRACE_VERSION,
        request_log_id: summary.id,
        status: if summary.trace_status == "complete" {
            RequestDecisionTraceStatus::LegacySummary
        } else {
            RequestDecisionTraceStatus::TraceUnavailable
        },
        reason: "routing_decision_store".to_string(),
        legacy_summary: None,
        timeline,
        planning_rounds,
    }
}

fn summary_from_decision(row: RoutingDecisionSummaryRow) -> RecentRouteDecisionSummary {
    let timestamp = row.decided_at_ms.to_string();
    RecentRouteDecisionSummary {
        request_log_id: row.id,
        request_id: Some(row.request_id),
        created_at: timestamp.clone(),
        started_at: timestamp,
        finished_at: None,
        duration_ms: None,
        endpoint: "routing_decision".to_string(),
        model: None,
        status: row.trace_status.clone(),
        lifecycle_status: None,
        station_key_id: row.selected_station_key_id,
        station_id: row.selected_station_id,
        route_policy: Some(row.ordering_profile),
        route_reason: Some(row.trace_status),
        fallback_count: 0,
        cost_status: None,
        estimated_total_cost: None,
        cost_currency: None,
    }
}
