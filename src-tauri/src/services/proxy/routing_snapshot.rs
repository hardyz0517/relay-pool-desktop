use crate::{
    models::{
        proxy::{ProxyStatus, RequestLog},
        routing::{RouteEndpointKind, StationKeyCapabilities, StationKeyHealth},
    },
    services::{
        database::{now_millis_for_services, AppDatabase},
        proxy::{
            router::RouteCandidateEconomics,
            routing_health::error_summary_indicates_offline,
            routing_types::{
                DecisionFact, DecisionFactKind, DecisionFactSeverity, LocalRoutingCandidateRow,
                LocalRoutingSettingsView, LocalRoutingSummary, LocalRoutingWorkspace,
                RouteDecisionEvent, RouteDecisionStatus, RouteDecisionSummary, RouteHealthState,
            },
        },
    },
};

#[derive(Debug, Clone)]
pub(crate) struct LocalRoutingReadCandidate {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) station_name: String,
    pub(crate) key_name: String,
    pub(crate) capabilities: StationKeyCapabilities,
    pub(crate) health: Option<StationKeyHealth>,
    pub(crate) economics: Option<RouteCandidateEconomics>,
}

pub fn load_local_routing_workspace(
    database: &AppDatabase,
    proxy_status: ProxyStatus,
) -> Result<LocalRoutingWorkspace, String> {
    let settings = database.get_settings()?;
    let candidates = database.local_routing_read_candidates()?;
    let request_logs = database.list_request_logs()?;
    let latest_log = request_logs.first();
    let rows = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| candidate_row(index, candidate))
        .collect::<Vec<_>>();

    Ok(LocalRoutingWorkspace {
        proxy_status,
        settings: LocalRoutingSettingsView {
            enabled: true,
            bind_addr: "127.0.0.1".to_string(),
            port: settings.local_proxy_port,
            endpoint: RouteEndpointKind::ChatCompletions,
            policy: settings.default_routing_strategy,
            fallback_enabled: settings.allow_depleted_fallback,
        },
        summary: LocalRoutingSummary {
            enabled_candidate_count: rows.iter().filter(|row| row.enabled).count() as i64,
            healthy_candidate_count: rows
                .iter()
                .filter(|row| row.health_state == RouteHealthState::Ready)
                .count() as i64,
            degraded_candidate_count: rows
                .iter()
                .filter(|row| row.health_state == RouteHealthState::Degraded)
                .count() as i64,
            cooldown_candidate_count: rows
                .iter()
                .filter(|row| row.health_state == RouteHealthState::Cooldown)
                .count() as i64,
            last_decision_at: latest_log.map(|log| log.started_at.clone()),
        },
        candidates: rows,
        latest_decision: latest_log.map(latest_decision),
        recent_events: recent_events(&request_logs),
    })
}

fn candidate_row(index: usize, candidate: &LocalRoutingReadCandidate) -> LocalRoutingCandidateRow {
    let health_state = health_state(candidate);
    let mut facts = Vec::new();
    facts.push(DecisionFact {
        kind: DecisionFactKind::Policy,
        label: "Priority".to_string(),
        value: format!("#{}", index + 1),
        severity: DecisionFactSeverity::Info,
    });
    facts.push(DecisionFact {
        kind: DecisionFactKind::Capability,
        label: "Protocol".to_string(),
        value: capability_label(candidate),
        severity: DecisionFactSeverity::Info,
    });

    if let Some(health) = &candidate.health {
        facts.push(DecisionFact {
            kind: DecisionFactKind::Health,
            label: "Health".to_string(),
            value: if health.consecutive_failures > 0 {
                format!("{} recent failure(s)", health.consecutive_failures)
            } else {
                "No recent failures".to_string()
            },
            severity: if health.consecutive_failures > 0 {
                DecisionFactSeverity::Warning
            } else {
                DecisionFactSeverity::Info
            },
        });
    }

    if let Some(economics) = &candidate.economics {
        if let Some(status) = economics.normalization_status.as_deref() {
            facts.push(DecisionFact {
                kind: DecisionFactKind::Pricing,
                label: "Pricing".to_string(),
                value: status.to_string(),
                severity: DecisionFactSeverity::Info,
            });
        }
        if let Some(status) = economics.balance_status.as_deref() {
            facts.push(DecisionFact {
                kind: DecisionFactKind::Balance,
                label: "Balance".to_string(),
                value: status.to_string(),
                severity: match status {
                    "depleted" => DecisionFactSeverity::Error,
                    "low" => DecisionFactSeverity::Warning,
                    _ => DecisionFactSeverity::Info,
                },
            });
        }
    }

    LocalRoutingCandidateRow {
        station_key_id: candidate.station_key_id.clone(),
        station_id: candidate.station_id.clone(),
        station_name: candidate.station_name.clone(),
        key_name: candidate.key_name.clone(),
        endpoint: RouteEndpointKind::ChatCompletions,
        priority: (index + 1) as i64,
        enabled: true,
        health_state,
        last_success_at: candidate
            .health
            .as_ref()
            .and_then(|health| health.last_success_at.clone()),
        last_failure_at: candidate
            .health
            .as_ref()
            .and_then(|health| health.last_failure_at.clone()),
        cooldown_until: candidate
            .health
            .as_ref()
            .and_then(|health| health.cooldown_until.clone()),
        score: None,
        facts,
    }
}

fn health_state(candidate: &LocalRoutingReadCandidate) -> RouteHealthState {
    let now = now_millis_for_services() as i64;
    let Some(health) = &candidate.health else {
        return RouteHealthState::Unknown;
    };
    if health
        .last_error_summary
        .as_deref()
        .map(error_summary_indicates_offline)
        .unwrap_or(false)
    {
        return RouteHealthState::Offline;
    }
    if health
        .cooldown_until
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .map(|until| until > now)
        .unwrap_or(false)
    {
        return RouteHealthState::Cooldown;
    }
    if health.consecutive_failures > 0 {
        return RouteHealthState::Degraded;
    }
    if health.success_count > 0 || health.last_success_at.is_some() {
        return RouteHealthState::Ready;
    }
    RouteHealthState::Unknown
}

fn capability_label(candidate: &LocalRoutingReadCandidate) -> String {
    let mut protocols = Vec::new();
    if candidate.capabilities.supports_chat_completions {
        protocols.push("chat");
    }
    if candidate.capabilities.supports_responses {
        protocols.push("responses");
    }
    if candidate.capabilities.supports_embeddings {
        protocols.push("embeddings");
    }
    if protocols.is_empty() {
        "No advertised protocol".to_string()
    } else {
        protocols.join(", ")
    }
}

fn latest_decision(log: &RequestLog) -> RouteDecisionSummary {
    RouteDecisionSummary {
        id: log.id.clone(),
        decided_at: log.started_at.clone(),
        endpoint: endpoint_from_path(&log.path),
        model: log.model.clone(),
        selected_station_key_id: log.station_key_id.clone(),
        selected_station_id: log.station_id.clone(),
        selected_station_name: None,
        policy: log
            .route_policy
            .clone()
            .unwrap_or_else(|| "priority_fallback".to_string()),
        status: decision_status(log),
        reason: log
            .route_reason
            .clone()
            .unwrap_or_else(|| "Recorded from latest local proxy request".to_string()),
        fallback_count: log.fallback_count,
    }
}

fn recent_events(logs: &[RequestLog]) -> Vec<RouteDecisionEvent> {
    logs.iter()
        .take(5)
        .map(|log| RouteDecisionEvent {
            id: format!("event-{}", log.id),
            decision_id: log.id.clone(),
            occurred_at: log.started_at.clone(),
            station_key_id: log.station_key_id.clone(),
            station_id: log.station_id.clone(),
            accepted: matches!(log.status.as_str(), "success" | "fallback"),
            facts: Vec::new(),
            message: event_message(log),
        })
        .collect()
}

fn event_message(log: &RequestLog) -> String {
    match log.status.as_str() {
        "success" => "Request completed on selected route".to_string(),
        "fallback" => format!("Request completed after {} fallback(s)", log.fallback_count),
        "failed" => "Request failed before a usable route completed".to_string(),
        "interrupted" => "Request stream was interrupted before completion".to_string(),
        other => format!("Request finished with status {other}"),
    }
}

fn decision_status(log: &RequestLog) -> RouteDecisionStatus {
    match log.status.as_str() {
        "success" => RouteDecisionStatus::Selected,
        "fallback" => RouteDecisionStatus::Fallback,
        "failed" => RouteDecisionStatus::Failed,
        "interrupted" => RouteDecisionStatus::Failed,
        _ if log.station_key_id.is_none() => RouteDecisionStatus::Unavailable,
        _ => RouteDecisionStatus::Selected,
    }
}

fn endpoint_from_path(path: &str) -> RouteEndpointKind {
    if path.contains("/responses") {
        RouteEndpointKind::Responses
    } else if path.contains("/embeddings") {
        RouteEndpointKind::Embeddings
    } else if path.contains("/models") {
        RouteEndpointKind::Models
    } else {
        RouteEndpointKind::ChatCompletions
    }
}
