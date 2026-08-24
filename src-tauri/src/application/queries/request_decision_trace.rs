use crate::observability::decision_trace::RequestDecisionTraceV1;
use crate::persistence::stores::request_outcome_store::{
    RoutingAttemptTraceRow, RoutingDecisionEventRow, RoutingOutcomeSummaryRow,
};
use crate::persistence::stores::routing_decisions::queries::{
    RouteCandidateDecisionRow, RoutingDecisionCursor, RoutingDecisionPage,
    RoutingDecisionSummaryRow,
};

pub(crate) const REQUEST_DECISION_TRACE_VERSION: &str = "request_decision_trace_v2";
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
    DurableSummary,
    RuntimeTrace,
    LegacySummary,
    TraceUnavailable,
}

/// Describes how much evidence the read model can honestly expose.  A
/// durable outcome survives restart but does not contain the in-memory
/// attempt timeline; callers must not infer missing events from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequestDecisionDetailAvailability {
    Detailed,
    SummaryOnly,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequestDecisionAction {
    RetrySameTarget,
    WaitThenReplan,
    TryDifferentFailureDomain,
    StopRequest,
}

pub(crate) fn decision_trace_from_durable_outcome(
    summary: RoutingOutcomeSummaryRow,
) -> RequestDecisionTrace {
    let detail_code = summary.terminal_code.clone();
    let terminal_kind = summary.terminal_kind.clone();
    RequestDecisionTrace {
        trace_version: REQUEST_DECISION_TRACE_VERSION,
        request_log_id: summary.request_id,
        status: RequestDecisionTraceStatus::DurableSummary,
        detail_availability: RequestDecisionDetailAvailability::SummaryOnly,
        reason: "request_routing_outcome_summary".to_string(),
        explanation_key: Some("request_routing_outcome_summary".to_string()),
        policy_revision: None,
        legacy_summary: None,
        timeline: vec![RequestDecisionTimelineItem {
            ordinal: 1,
            kind: RequestDecisionTimelineKind::DownstreamDelivery,
            status: RequestDecisionTimelineStatus::Available,
            title: "Routing outcome".to_string(),
            summary: format!(
                "terminal={terminal_kind}; classification={}; confidence={}; evidence={}; acceptance={}; send_phase={}; replay={}; billing={}; retry={}; effect={}",
                summary.classification,
                summary.confidence,
                summary.evidence_source,
                summary.request_accepted,
                summary.send_phase,
                summary.replay_disposition,
                summary.billing_state,
                summary.retry_disposition,
                summary.effect_summary,
            ),
            detail_code,
            detail_availability: RequestDecisionDetailAvailability::SummaryOnly,
            explanation_key: Some("request_routing_outcome_summary".to_string()),
            action: None,
            attempt_ordinal: Some(summary.attempt_count.max(0) as u32),
            remaining_attempts: Some(0),
            remaining_wait_budget_ms: None,
            policy_revision: None,
            failure_domain: None,
            route_policy: None,
            route_reason: Some(summary.profile_version),
            station_key_id: None,
            station_id: None,
            attempt_count: Some(summary.attempt_count),
            fallback_count: Some(summary.fallback_count),
            duration_ms: None,
            cost_status: None,
            estimated_total_cost: None,
            cost_currency: None,
            occurred_at_ms: None,
        }],
        planning_rounds: Vec::new(),
    }
}

/// Append the bounded attempt lifecycle projection to a durable terminal
/// summary. The projection deliberately remains summary-only: after restart
/// it can explain which attempts terminated and why, but it cannot recreate
/// runtime scheduling details that were never persisted.
pub(crate) fn append_durable_attempt_trace(
    mut durable: RequestDecisionTrace,
    attempts: Vec<RoutingAttemptTraceRow>,
) -> RequestDecisionTrace {
    let has_attempts = !attempts.is_empty();
    let offset = durable.timeline.len() as u32;
    for (index, attempt) in attempts.into_iter().enumerate() {
        let ordinal = u32::try_from(attempt.ordinal.max(0)).unwrap_or(u32::MAX);
        let code = attempt
            .public_code
            .as_deref()
            .filter(|value| is_safe_trace_token(value))
            .unwrap_or("attempt_terminal");
        let retry = attempt
            .retry_disposition
            .as_deref()
            .filter(|value| is_safe_trace_token(value))
            .unwrap_or("none");
        let duration_ms = attempt
            .terminal_at_ms
            .checked_sub(attempt.started_at_ms)
            .map(|value| value.max(0));
        durable.timeline.push(RequestDecisionTimelineItem {
            ordinal: offset.saturating_add(index as u32).saturating_add(1),
            kind: RequestDecisionTimelineKind::AttemptProtocol,
            status: RequestDecisionTimelineStatus::Available,
            title: "Durable attempt".to_string(),
            summary: format!(
                "attempt={}; terminal={}; code={}; lifecycle={}; output_committed={}",
                ordinal.saturating_add(1),
                attempt.terminal_kind,
                code,
                retry,
                attempt.output_committed,
            ),
            detail_code: code.to_string(),
            detail_availability: RequestDecisionDetailAvailability::SummaryOnly,
            explanation_key: Some("durable_attempt_terminal".to_string()),
            action: None,
            attempt_ordinal: Some(ordinal.saturating_add(1)),
            remaining_attempts: None,
            remaining_wait_budget_ms: None,
            policy_revision: None,
            failure_domain: None,
            route_policy: None,
            route_reason: Some("durable_attempt_lifecycle".to_string()),
            station_key_id: None,
            station_id: None,
            attempt_count: Some(ordinal.saturating_add(1) as i64),
            fallback_count: Some(ordinal as i64),
            duration_ms,
            cost_status: None,
            estimated_total_cost: None,
            cost_currency: None,
            occurred_at_ms: None,
        });
    }
    if has_attempts {
        durable.reason = "request_routing_outcome_summary_with_durable_attempts".to_string();
        durable.explanation_key =
            Some("request_routing_outcome_summary_with_durable_attempts".to_string());
    }
    durable
}

/// Replace the compatibility attempt projection with the durable event
/// timeline when the new event table is available. The terminal summary is
/// still the authoritative outcome; events only add ordered, redacted
/// lifecycle evidence and never recreate runtime scheduling details.
pub(crate) fn append_durable_decision_events(
    mut durable: RequestDecisionTrace,
    events: Vec<RoutingDecisionEventRow>,
) -> RequestDecisionTrace {
    if events.is_empty() {
        return durable;
    }
    let offset = durable.timeline.len() as u32;
    let event_timeline = events
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            let kind = match event.event_kind.as_str() {
                "request_started" | "request_finalized" => {
                    RequestDecisionTimelineKind::DownstreamDelivery
                }
                "attempt_started" | "attempt_succeeded" | "failure_classified" => {
                    RequestDecisionTimelineKind::AttemptProtocol
                }
                "retry_scheduled" => RequestDecisionTimelineKind::Fallback,
                _ => RequestDecisionTimelineKind::Unavailable,
            };
            let retry_label = event
                .retry_disposition
                .as_deref()
                .map(|value| format!(" retry={value}"))
                .unwrap_or_default();
            RequestDecisionTimelineItem {
                ordinal: offset
                    .saturating_add(u32::try_from(index).unwrap_or(u32::MAX))
                    .saturating_add(1),
                kind,
                status: RequestDecisionTimelineStatus::Available,
                title: event.event_kind.replace('_', " "),
                summary: format!("{}{}", event.detail_code, retry_label),
                detail_code: event.detail_code,
                detail_availability: RequestDecisionDetailAvailability::SummaryOnly,
                explanation_key: Some(event.event_kind),
                action: None,
                attempt_ordinal: event
                    .attempt_ordinal
                    .and_then(|value| u32::try_from(value).ok())
                    .map(|value| value.saturating_add(1)),
                remaining_attempts: None,
                remaining_wait_budget_ms: None,
                policy_revision: None,
                failure_domain: None,
                route_policy: None,
                route_reason: Some("durable_decision_event".to_string()),
                station_key_id: None,
                station_id: None,
                attempt_count: None,
                fallback_count: None,
                duration_ms: None,
                cost_status: None,
                estimated_total_cost: None,
                cost_currency: None,
                occurred_at_ms: Some(event.occurred_at_ms),
            }
        })
        .collect::<Vec<_>>();
    // Keep the terminal outcome visible alongside the ordered lifecycle
    // events.  The event table is an evidence supplement, not a replacement
    // for the authoritative durable terminal summary.
    durable.timeline.extend(event_timeline);
    durable.reason = "request_routing_outcome_summary_with_durable_events".to_string();
    durable.explanation_key =
        Some("request_routing_outcome_summary_with_durable_events".to_string());
    durable.detail_availability = RequestDecisionDetailAvailability::SummaryOnly;
    durable
}

fn is_safe_trace_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

/// Durable terminal facts are authoritative after restart. When the current
/// process still retains the bounded runtime trace, expose it as supplemental
/// diagnostic context instead of making callers choose one source.
pub(crate) fn append_runtime_trace(
    mut durable: RequestDecisionTrace,
    runtime: RequestDecisionTraceV1,
) -> RequestDecisionTrace {
    let runtime = decision_trace_from_runtime(runtime);
    let runtime_detail_availability = runtime.detail_availability;
    let offset = durable.timeline.len() as u32;
    durable
        .timeline
        .extend(runtime.timeline.into_iter().map(|mut item| {
            item.ordinal = item.ordinal.saturating_add(offset);
            item
        }));
    durable.planning_rounds.extend(runtime.planning_rounds);
    durable.reason = "request_routing_outcome_summary_with_runtime_events".to_string();
    durable.detail_availability = match runtime_detail_availability {
        RequestDecisionDetailAvailability::Detailed => RequestDecisionDetailAvailability::Detailed,
        RequestDecisionDetailAvailability::SummaryOnly => {
            RequestDecisionDetailAvailability::SummaryOnly
        }
        RequestDecisionDetailAvailability::Unavailable => durable.detail_availability,
    };
    durable.explanation_key =
        Some("request_routing_outcome_summary_with_runtime_events".to_string());
    durable
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestDecisionTrace {
    pub(crate) trace_version: &'static str,
    pub(crate) request_log_id: String,
    pub(crate) status: RequestDecisionTraceStatus,
    pub(crate) detail_availability: RequestDecisionDetailAvailability,
    pub(crate) reason: String,
    pub(crate) explanation_key: Option<String>,
    pub(crate) policy_revision: Option<u64>,
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
    pub(crate) detail_availability: RequestDecisionDetailAvailability,
    pub(crate) explanation_key: Option<String>,
    pub(crate) action: Option<RequestDecisionAction>,
    /// One-based ordinal for display. Runtime trace ordinals are zero-based
    /// execution indexes, so this conversion happens only at the read-model
    /// boundary.
    pub(crate) attempt_ordinal: Option<u32>,
    pub(crate) remaining_attempts: Option<u32>,
    pub(crate) remaining_wait_budget_ms: Option<u64>,
    pub(crate) policy_revision: Option<u64>,
    /// Coarse, non-secret failure domain category (never a provider/account
    /// identifier or endpoint).
    pub(crate) failure_domain: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) occurred_at_ms: Option<i64>,
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
    // The public trace key is the request-log ID used by deep links. Legacy
    // decision rows without a matching log retain their logical request ID.
    let request_log_id = summary
        .request_log_id
        .clone()
        .unwrap_or_else(|| summary.request_id.clone());
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
        detail_availability: if summary.trace_status == "complete" {
            RequestDecisionDetailAvailability::SummaryOnly
        } else {
            RequestDecisionDetailAvailability::Unavailable
        },
        explanation_key: Some("routing_decision_store".to_string()),
        action: None,
        attempt_ordinal: None,
        remaining_attempts: None,
        remaining_wait_budget_ms: None,
        policy_revision: None,
        failure_domain: None,
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
        occurred_at_ms: None,
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
        request_log_id,
        status: if summary.trace_status == "complete" {
            RequestDecisionTraceStatus::LegacySummary
        } else {
            RequestDecisionTraceStatus::TraceUnavailable
        },
        detail_availability: if summary.trace_status == "complete" {
            RequestDecisionDetailAvailability::SummaryOnly
        } else {
            RequestDecisionDetailAvailability::Unavailable
        },
        reason: "routing_decision_store".to_string(),
        explanation_key: Some("routing_decision_store".to_string()),
        policy_revision: None,
        legacy_summary: None,
        timeline,
        planning_rounds,
    }
}

pub(crate) fn decision_trace_from_runtime(trace: RequestDecisionTraceV1) -> RequestDecisionTrace {
    let timeline: Vec<RequestDecisionTimelineItem> = trace
        .events
        .into_iter()
        .map(|event| {
            let structured = parse_action_detail(event.detail.as_deref());
            let detail_availability = if structured.is_some() {
                RequestDecisionDetailAvailability::Detailed
            } else {
                RequestDecisionDetailAvailability::SummaryOnly
            };
            RequestDecisionTimelineItem {
                ordinal: event.ordinal,
                kind: match event.kind.as_str() {
                    "attempt_start" | "canonical_failure" | "sse_error_before_semantic_commit" => {
                        RequestDecisionTimelineKind::AttemptProtocol
                    }
                    "same_target_retry"
                    | "same_domain_fallback_suppressed"
                    | "cross_domain_fallback" => RequestDecisionTimelineKind::Fallback,
                    "committed_stop" | "request_terminal" => {
                        RequestDecisionTimelineKind::DownstreamDelivery
                    }
                    "saturation"
                    | "fail_closed"
                    | "profile_version_mismatch"
                    | "trace_truncated" => RequestDecisionTimelineKind::Unavailable,
                    _ => RequestDecisionTimelineKind::Unavailable,
                },
                status: if event.kind.as_str() == "trace_truncated" {
                    RequestDecisionTimelineStatus::Skipped
                } else {
                    RequestDecisionTimelineStatus::Available
                },
                title: event.kind.as_str().replace('_', " "),
                summary: event.code.clone(),
                detail_code: event.code.clone(),
                detail_availability,
                explanation_key: Some(event.code),
                action: structured.as_ref().and_then(|detail| detail.action),
                attempt_ordinal: structured
                    .as_ref()
                    .and_then(|detail| detail.attempt_ordinal)
                    .or(Some(event.ordinal.saturating_add(1))),
                remaining_attempts: structured
                    .as_ref()
                    .and_then(|detail| detail.remaining_attempts),
                remaining_wait_budget_ms: structured
                    .as_ref()
                    .and_then(|detail| detail.remaining_wait_budget_ms),
                policy_revision: structured
                    .as_ref()
                    .and_then(|detail| detail.policy_revision),
                failure_domain: structured
                    .as_ref()
                    .and_then(|detail| detail.failure_domain.clone()),
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
                occurred_at_ms: None,
            }
        })
        .collect();
    let detail_availability = if timeline
        .iter()
        .any(|item| item.detail_availability == RequestDecisionDetailAvailability::Detailed)
    {
        RequestDecisionDetailAvailability::Detailed
    } else if timeline.is_empty() {
        RequestDecisionDetailAvailability::Unavailable
    } else {
        RequestDecisionDetailAvailability::SummaryOnly
    };
    RequestDecisionTrace {
        trace_version: trace.profile_version,
        request_log_id: trace.request_id,
        status: RequestDecisionTraceStatus::RuntimeTrace,
        detail_availability,
        reason: "decision_trace_ring".to_string(),
        explanation_key: Some("decision_trace_ring".to_string()),
        policy_revision: timeline.iter().find_map(|item| item.policy_revision),
        legacy_summary: None,
        timeline,
        planning_rounds: Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedActionDetail {
    action: Option<RequestDecisionAction>,
    attempt_ordinal: Option<u32>,
    remaining_attempts: Option<u32>,
    remaining_wait_budget_ms: Option<u64>,
    policy_revision: Option<u64>,
    failure_domain: Option<String>,
}

fn parse_action_detail(detail: Option<&str>) -> Option<ParsedActionDetail> {
    let detail = detail?.strip_prefix("action_")?;
    let mut action = None;
    let mut attempt_ordinal = None;
    let mut remaining_attempts = None;
    let mut remaining_wait_budget_ms = None;
    let mut policy_revision = None;
    let mut failure_domain = None;
    let parts: Vec<&str> = detail.split('_').collect();
    let mut index = 0;
    while index < parts.len() {
        match parts[index] {
            "retry"
                if parts.get(index + 1) == Some(&"same")
                    && parts.get(index + 2) == Some(&"target") =>
            {
                action = Some(RequestDecisionAction::RetrySameTarget);
                index += 3;
            }
            "wait"
                if parts.get(index + 1) == Some(&"then")
                    && parts.get(index + 2) == Some(&"replan") =>
            {
                action = Some(RequestDecisionAction::WaitThenReplan);
                index += 3;
            }
            "try"
                if parts.get(index + 1) == Some(&"different")
                    && parts.get(index + 2) == Some(&"failure")
                    && parts.get(index + 3) == Some(&"domain") =>
            {
                action = Some(RequestDecisionAction::TryDifferentFailureDomain);
                index += 4;
            }
            "stop" if parts.get(index + 1) == Some(&"request") => {
                action = Some(RequestDecisionAction::StopRequest);
                index += 2;
            }
            "attempt" => {
                attempt_ordinal = parts.get(index + 1).and_then(|value| value.parse().ok());
                index += 2;
            }
            "remaining" => {
                remaining_attempts = parts.get(index + 1).and_then(|value| value.parse().ok());
                index += 2;
            }
            "budget" => {
                remaining_wait_budget_ms =
                    parts.get(index + 1).and_then(|value| value.parse().ok());
                index += 2;
            }
            "policy" => {
                policy_revision = parts.get(index + 1).and_then(|value| value.parse().ok());
                index += 2;
            }
            "failure" => {
                // The failure code is intentionally reduced to a coarse
                // category; no target/provider identifier crosses this API.
                let code = parts.get(index + 1).copied().unwrap_or_default();
                failure_domain = Some(if code.contains("capacity") || code.contains("rate") {
                    "capacity_domain".to_string()
                } else if code.contains("endpoint") || code.contains("upstream") {
                    "upstream".to_string()
                } else {
                    "unknown".to_string()
                });
                index += 2;
            }
            // A delay suggestion is not the remaining request deadline.
            // Keep the public budget field empty until the producer includes
            // an explicit budget token.
            "delay" => index += 2,
            _ => index += 1,
        }
    }
    Some(ParsedActionDetail {
        action,
        attempt_ordinal,
        remaining_attempts,
        remaining_wait_budget_ms,
        policy_revision,
        failure_domain,
    })
}

fn summary_from_decision(row: RoutingDecisionSummaryRow) -> RecentRouteDecisionSummary {
    let timestamp = row.decided_at_ms.to_string();
    RecentRouteDecisionSummary {
        request_log_id: row.request_log_id.unwrap_or_else(|| row.request_id.clone()),
        request_id: Some(row.request_id),
        created_at: timestamp.clone(),
        started_at: timestamp,
        finished_at: None,
        duration_ms: None,
        endpoint: "routing_decision".to_string(),
        model: row.model,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_retry_action_detail_without_exposing_target_data() {
        let parsed = parse_action_detail(Some(
            "action_wait_then_replan_failure_capacity_exhausted_attempt_1_remaining_2_policy_7_excluded_1_delay_250",
        ))
        .expect("valid bounded action detail");
        assert_eq!(parsed.action, Some(RequestDecisionAction::WaitThenReplan));
        assert_eq!(parsed.attempt_ordinal, Some(1));
        assert_eq!(parsed.remaining_attempts, Some(2));
        assert_eq!(parsed.policy_revision, Some(7));
        assert_eq!(parsed.failure_domain.as_deref(), Some("capacity_domain"));
        assert_eq!(parsed.remaining_wait_budget_ms, None);
    }

    #[test]
    fn recent_decision_summary_keeps_request_log_identity_and_model() {
        let page = recent_route_decisions_from_page(RoutingDecisionPage {
            rows: vec![RoutingDecisionSummaryRow {
                id: "decision-a".to_string(),
                request_id: "logical-request-a".to_string(),
                request_log_id: Some("log-a".to_string()),
                model: Some("gpt-test".to_string()),
                decided_at_ms: 10_000,
                ordering_profile: "cost_first".to_string(),
                selected_station_key_id: None,
                selected_station_id: None,
                candidate_count: 0,
                candidate_detail_count: 0,
                candidate_detail_truncated: false,
                rejection_counts: serde_json::json!({}),
                trace_status: "complete".to_string(),
            }],
            next_cursor: None,
        });
        assert_eq!(page.decisions[0].request_log_id, "log-a");
        assert_eq!(
            page.decisions[0].request_id.as_deref(),
            Some("logical-request-a")
        );
        assert_eq!(page.decisions[0].model.as_deref(), Some("gpt-test"));
    }

    #[test]
    fn missing_runtime_detail_is_not_promoted_to_detailed() {
        let trace =
            decision_trace_from_runtime(RequestDecisionTraceV1 {
                request_id: "request-1".to_string(),
                profile_version: "DecisionTraceProfileV1",
                events: vec![crate::observability::decision_trace::DecisionTraceEvent::new(
                crate::observability::decision_trace::DecisionTraceEventKind::RequestTerminal,
                "request_completed",
                0,
                None,
            )
            .expect("event")],
                trace_truncated: false,
                serialized_bytes_estimate: 1,
            });
        assert_eq!(
            trace.detail_availability,
            RequestDecisionDetailAvailability::SummaryOnly
        );
        assert_eq!(
            trace.timeline[0].detail_availability,
            RequestDecisionDetailAvailability::SummaryOnly
        );
        assert!(trace.timeline[0].action.is_none());
    }

    #[test]
    fn durable_attempt_projection_is_bounded_and_summary_only() {
        let summary = RoutingOutcomeSummaryRow {
            request_id: "request-1".to_string(),
            profile_version: "routing_outcome_v1".to_string(),
            terminal_kind: "failed".to_string(),
            terminal_code: "route_deadline_exceeded".to_string(),
            classification: "timeout".to_string(),
            confidence: "confirmed".to_string(),
            evidence_source: "timeout".to_string(),
            request_accepted: "not_accepted".to_string(),
            send_phase: "not_connected".to_string(),
            replay_disposition: "completed".to_string(),
            billing_state: "not_billed".to_string(),
            retry_disposition: "fail_closed".to_string(),
            effect_summary: "none".to_string(),
            failure_domain_commitment_version: None,
            failure_domain_commitment_digest: None,
            attempt_count: 2,
            fallback_count: 1,
            terminal_at_ms: 200,
        };
        let trace = append_durable_attempt_trace(
            decision_trace_from_durable_outcome(summary),
            vec![
                RoutingAttemptTraceRow {
                    ordinal: 0,
                    terminal_kind: "failed".to_string(),
                    retry_disposition: Some("TryNextCandidate".to_string()),
                    public_code: Some("upstream_timeout".to_string()),
                    output_committed: false,
                    started_at_ms: 100,
                    terminal_at_ms: 150,
                },
                RoutingAttemptTraceRow {
                    ordinal: 1,
                    terminal_kind: "failed".to_string(),
                    retry_disposition: Some("StopRequest".to_string()),
                    public_code: Some("route_deadline_exceeded".to_string()),
                    output_committed: false,
                    started_at_ms: 160,
                    terminal_at_ms: 200,
                },
            ],
        );
        assert_eq!(
            trace.detail_availability,
            RequestDecisionDetailAvailability::SummaryOnly
        );
        assert_eq!(trace.timeline.len(), 3);
        assert_eq!(trace.timeline[1].attempt_ordinal, Some(1));
        assert_eq!(trace.timeline[2].attempt_ordinal, Some(2));
        assert_eq!(
            trace.timeline[1].detail_availability,
            RequestDecisionDetailAvailability::SummaryOnly
        );
        assert_eq!(trace.timeline[1].action, None);
        assert_eq!(trace.timeline[1].duration_ms, Some(50));
        assert!(trace.timeline[1].summary.contains("upstream_timeout"));
    }

    #[test]
    fn durable_attempt_projection_redacts_unsafe_tokens() {
        let summary = RoutingOutcomeSummaryRow {
            request_id: "request-1".to_string(),
            profile_version: "routing_outcome_v1".to_string(),
            terminal_kind: "failed".to_string(),
            terminal_code: "route_failed".to_string(),
            classification: "generic".to_string(),
            confidence: "unknown".to_string(),
            evidence_source: "local".to_string(),
            request_accepted: "unknown".to_string(),
            send_phase: "unknown".to_string(),
            replay_disposition: "stopped_uncertain".to_string(),
            billing_state: "possibly_billed".to_string(),
            retry_disposition: "fail_closed".to_string(),
            effect_summary: "none".to_string(),
            failure_domain_commitment_version: None,
            failure_domain_commitment_digest: None,
            attempt_count: 1,
            fallback_count: 0,
            terminal_at_ms: 100,
        };
        let trace = append_durable_attempt_trace(
            decision_trace_from_durable_outcome(summary),
            vec![RoutingAttemptTraceRow {
                ordinal: 0,
                terminal_kind: "failed".to_string(),
                retry_disposition: Some("https://secret.invalid".to_string()),
                public_code: Some("AuthorizationBearerSecret".to_string()),
                output_committed: false,
                started_at_ms: 0,
                terminal_at_ms: 100,
            }],
        );
        assert!(trace.timeline[1].summary.contains("attempt_terminal"));
        assert!(!trace.timeline[1].summary.contains("https://"));
        assert!(!trace.timeline[1].summary.contains("Authorization"));
    }

    #[test]
    fn durable_event_projection_keeps_authoritative_terminal_summary() {
        let summary = RoutingOutcomeSummaryRow {
            request_id: "request-1".to_string(),
            profile_version: "routing_outcome_v1".to_string(),
            terminal_kind: "interrupted".to_string(),
            terminal_code: "startup_interrupted".to_string(),
            classification: "local".to_string(),
            confidence: "confirmed".to_string(),
            evidence_source: "local".to_string(),
            request_accepted: "unknown".to_string(),
            send_phase: "unknown".to_string(),
            replay_disposition: "stopped_uncertain".to_string(),
            billing_state: "possibly_billed".to_string(),
            retry_disposition: "fail_closed".to_string(),
            effect_summary: "none".to_string(),
            failure_domain_commitment_version: None,
            failure_domain_commitment_digest: None,
            attempt_count: 1,
            fallback_count: 0,
            terminal_at_ms: 100,
        };
        let trace = append_durable_decision_events(
            decision_trace_from_durable_outcome(summary),
            vec![RoutingDecisionEventRow {
                event_key: "request_started".to_string(),
                sequence: 0,
                occurred_at_ms: 1,
                event_kind: "request_started".to_string(),
                detail_code: "request_started".to_string(),
                attempt_ordinal: None,
                retry_disposition: None,
                output_committed: None,
            }],
        );
        assert_eq!(trace.timeline.len(), 2);
        assert_eq!(trace.timeline[0].detail_code, "startup_interrupted");
        assert_eq!(trace.timeline[1].detail_code, "request_started");
        assert_eq!(trace.timeline[0].ordinal, 1);
        assert_eq!(trace.timeline[1].ordinal, 2);
        assert_eq!(
            trace.detail_availability,
            RequestDecisionDetailAvailability::SummaryOnly
        );
    }
}
