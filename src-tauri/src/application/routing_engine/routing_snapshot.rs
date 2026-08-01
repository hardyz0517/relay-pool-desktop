use crate::{
    application::operational_facts::{
        candidate_projector::RouteCandidateProjection, capability_projector::CapabilityDecision,
    },
    models::{
        proxy::{ProxyStatus, RequestLog},
        routing::{
            RouteEndpointKind, RoutingGroupFilter, StationKeyCapabilities, StationKeyHealth,
        },
        settings::AppSettings,
    },
};

use super::{
    routing_health::error_summary_indicates_offline,
    routing_types::{
        DecisionFact, DecisionFactKind, DecisionFactSeverity, LocalRoutingCandidateRow,
        LocalRoutingPreviewKind, LocalRoutingSettingsView, LocalRoutingSummary,
        LocalRoutingWorkspace, RouteCandidateEconomics, RouteDecisionEvent, RouteDecisionStatus,
        RouteDecisionSummary, RouteHealthState,
    },
};

#[derive(Debug, Clone)]
pub(crate) struct LocalRoutingReadCandidate {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) station_name: String,
    pub(crate) key_name: String,
    pub(crate) schedulable: bool,
    pub(crate) capabilities: StationKeyCapabilities,
    pub(crate) health: Option<StationKeyHealth>,
    pub(crate) economics: Option<RouteCandidateEconomics>,
    pub(crate) projection: Option<RouteCandidateProjection>,
}

pub(crate) fn build_local_routing_workspace(
    settings: AppSettings,
    candidates: Vec<LocalRoutingReadCandidate>,
    request_logs: Vec<RequestLog>,
    proxy_status: ProxyStatus,
) -> LocalRoutingWorkspace {
    let latest_log = request_logs.first();
    let now_ms = current_time_millis();
    let rows = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            candidate_row(
                index,
                candidate,
                &settings.default_routing_group_filter,
                now_ms,
            )
        })
        .collect::<Vec<_>>();

    let latest_decision = latest_log.map(|log| latest_decision(log, &rows));

    LocalRoutingWorkspace {
        proxy_status,
        settings: LocalRoutingSettingsView {
            enabled: true,
            bind_addr: "127.0.0.1".to_string(),
            port: settings.local_proxy_port,
            endpoint: RouteEndpointKind::ChatCompletions,
            policy: settings.default_routing_strategy,
            max_rate_multiplier: settings.max_rate_multiplier,
            routing_group_filter: settings.default_routing_group_filter.clone(),
            fallback_enabled: settings.allow_depleted_fallback,
            preview_kind: LocalRoutingPreviewKind::BaselineEligibility,
        },
        summary: build_local_routing_summary(&rows, latest_log.map(|log| log.started_at.clone())),
        candidates: rows,
        latest_decision,
        recent_events: recent_events(&request_logs),
    }
}

fn current_time_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

fn candidate_row(
    index: usize,
    candidate: &LocalRoutingReadCandidate,
    routing_group_filter: &RoutingGroupFilter,
    now_ms: i64,
) -> LocalRoutingCandidateRow {
    let health_state = health_state(candidate, now_ms);
    let routing_group_match = candidate
        .projection
        .as_ref()
        .map(|projection| projection.policy.group_matches)
        .unwrap_or_else(|| local_candidate_group_matches(routing_group_filter, candidate));
    let preview_reject_reasons = candidate
        .projection
        .as_ref()
        .map(projection_preview_reject_reasons)
        .unwrap_or_default();
    let preview_reject_reasons = if candidate.schedulable {
        preview_reject_reasons
    } else {
        let mut reasons = preview_reject_reasons;
        if !reasons.iter().any(|reason| reason == "asset_unavailable") {
            reasons.insert(0, "asset_unavailable".to_string());
        }
        reasons
    };
    let preview_eligible = candidate.schedulable && preview_reject_reasons.is_empty();
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
    if let Some(projection) = &candidate.projection {
        if let Some(multiplier) = projection.multiplier.multiplier {
            facts.push(DecisionFact {
                kind: DecisionFactKind::Pricing,
                label: "Effective multiplier".to_string(),
                value: format!(
                    "{:.4}x via {}",
                    multiplier,
                    projection
                        .multiplier
                        .selected_source
                        .unwrap_or(projection.multiplier.reason)
                ),
                severity: DecisionFactSeverity::Info,
            });
        } else {
            facts.push(DecisionFact {
                kind: DecisionFactKind::Pricing,
                label: "Multiplier evidence".to_string(),
                value: projection.multiplier.reason.to_string(),
                severity: DecisionFactSeverity::Warning,
            });
        }
    } else if let Some(multiplier) = candidate
        .economics
        .as_ref()
        .and_then(|economics| economics.rate_multiplier)
    {
        facts.push(DecisionFact {
            kind: DecisionFactKind::Pricing,
            label: "Effective multiplier".to_string(),
            value: format!("{multiplier:.4}x via economics"),
            severity: DecisionFactSeverity::Info,
        });
    }
    facts.push(DecisionFact {
        kind: DecisionFactKind::Policy,
        label: "Routing group".to_string(),
        value: if routing_group_match {
            "matched".to_string()
        } else {
            "out_of_scope".to_string()
        },
        severity: if routing_group_match {
            DecisionFactSeverity::Info
        } else {
            DecisionFactSeverity::Warning
        },
    });

    LocalRoutingCandidateRow {
        station_key_id: candidate.station_key_id.clone(),
        station_id: candidate.station_id.clone(),
        station_name: candidate.station_name.clone(),
        key_name: candidate.key_name.clone(),
        endpoint: RouteEndpointKind::ChatCompletions,
        priority: (index + 1) as i64,
        enabled: true,
        schedulable: candidate.schedulable,
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
        routing_group_scope: routing_group_filter.clone(),
        routing_group_match,
        preview_eligible,
        preview_reject_reasons,
        facts,
    }
}

fn projection_preview_reject_reasons(projection: &RouteCandidateProjection) -> Vec<String> {
    let mut reasons = projection
        .hard_rejection_codes
        .iter()
        .map(|code| preview_reject_reason_code(code, projection).to_string())
        .collect::<Vec<_>>();
    reasons.sort();
    reasons.dedup();
    reasons
}

fn preview_reject_reason_code(
    code: &'static str,
    projection: &RouteCandidateProjection,
) -> &'static str {
    match code {
        "credential_missing" => "asset_unavailable",
        "capability_rejected" => {
            if projection.capability.model == CapabilityDecision::Reject {
                "model_mismatch"
            } else {
                "capability_mismatch"
            }
        }
        "group_mismatch" => "routing_group_mismatch",
        "health_hard_reject" => "health_blocked",
        "multiplier_ceiling" => "multiplier_over_ceiling",
        other => other,
    }
}

fn build_local_routing_summary(
    rows: &[LocalRoutingCandidateRow],
    last_decision_at: Option<String>,
) -> LocalRoutingSummary {
    LocalRoutingSummary {
        candidate_count: rows.len() as i64,
        preview_eligible_candidate_count: rows.iter().filter(|row| row.preview_eligible).count()
            as i64,
        preview_excluded_candidate_count: rows.iter().filter(|row| !row.preview_eligible).count()
            as i64,
        cooldown_candidate_count: rows
            .iter()
            .filter(|row| row.health_state == RouteHealthState::Cooldown)
            .count() as i64,
        last_decision_at,
    }
}

fn local_candidate_group_matches(
    filter: &RoutingGroupFilter,
    _candidate: &LocalRoutingReadCandidate,
) -> bool {
    match filter {
        RoutingGroupFilter::AllGroups => true,
        RoutingGroupFilter::UngroupedOnly
        | RoutingGroupFilter::GroupBindingId(_)
        | RoutingGroupFilter::GroupIdHash(_)
        | RoutingGroupFilter::GroupType(_) => false,
    }
}

fn health_state(candidate: &LocalRoutingReadCandidate, now_ms: i64) -> RouteHealthState {
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
        .map(|until| until > now_ms)
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

fn latest_decision(
    log: &RequestLog,
    candidates: &[LocalRoutingCandidateRow],
) -> RouteDecisionSummary {
    let selected_station_name = log.station_key_id.as_ref().and_then(|station_key_id| {
        candidates
            .iter()
            .find(|candidate| &candidate.station_key_id == station_key_id)
            .map(|candidate| format!("{} / {}", candidate.station_name, candidate.key_name))
    });

    RouteDecisionSummary {
        id: log.id.clone(),
        decided_at: log.started_at.clone(),
        endpoint: endpoint_from_path(&log.path),
        model: log.model.clone(),
        selected_station_key_id: log.station_key_id.clone(),
        selected_station_id: log.station_id.clone(),
        selected_station_name,
        policy: log
            .route_policy
            .clone()
            .unwrap_or_else(|| "cost_stable_first".to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::operational_facts::runtime_candidate_adapter::{
            route_projection_from_runtime_candidate, route_request_facts_for_read_model,
        },
        models::{
            proxy::UpstreamApiFormat,
            routing::{
                PricingGroupType, RoutingPolicy, RuntimeRoutingCandidate, RuntimeRoutingSettings,
            },
        },
    };

    #[test]
    fn preview_summary_counts_the_same_decisions_exposed_on_rows() {
        let rows = preview_rows_for_test(vec![
            preview_candidate("eligible", PreviewFixture::Eligible),
            preview_candidate("group-mismatch", PreviewFixture::GroupMismatch),
            preview_candidate("multiplier-ceiling", PreviewFixture::MultiplierCeiling),
            preview_candidate("cooldown", PreviewFixture::Cooldown),
        ]);

        assert!(rows[0].preview_eligible);
        assert_eq!(
            rows[1].preview_reject_reasons,
            vec!["routing_group_mismatch"]
        );
        assert_eq!(
            rows[2].preview_reject_reasons,
            vec!["multiplier_over_ceiling"],
        );
        assert_eq!(rows[3].preview_reject_reasons, vec!["health_blocked"]);

        let summary = build_local_routing_summary(&rows, None);
        assert_eq!(summary.candidate_count, 4);
        assert_eq!(summary.preview_eligible_candidate_count, 1);
        assert_eq!(summary.preview_excluded_candidate_count, 3);
        assert_eq!(summary.cooldown_candidate_count, 1);
    }

    #[test]
    fn preview_uses_projection_rejections_without_multiplier_limit_guessing() {
        let rows = preview_rows_for_test(vec![
            preview_candidate("eligible", PreviewFixture::Eligible),
            preview_candidate("group-mismatch", PreviewFixture::GroupMismatch),
        ]);

        assert!(rows[0].preview_eligible);
        assert!(rows[0].preview_reject_reasons.is_empty());
        assert!(!rows[1].preview_eligible);
        assert_eq!(
            rows[1].preview_reject_reasons,
            vec!["routing_group_mismatch"]
        );
    }

    #[test]
    fn unschedulable_candidate_preview_is_paused_once() {
        let rows = preview_rows_for_test(vec![preview_candidate(
            "paused",
            PreviewFixture::Unschedulable,
        )]);

        assert!(!rows[0].schedulable);
        assert!(!rows[0].preview_eligible);
        assert_eq!(rows[0].preview_reject_reasons, vec!["asset_unavailable"]);
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum PreviewFixture {
        Eligible,
        GroupMismatch,
        MultiplierCeiling,
        Cooldown,
        Unschedulable,
    }

    fn preview_rows_for_test(
        candidates: Vec<LocalRoutingReadCandidate>,
    ) -> Vec<LocalRoutingCandidateRow> {
        candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                candidate_row(
                    index,
                    candidate,
                    &RoutingGroupFilter::GroupType(PricingGroupType::Gpt),
                    60_000,
                )
            })
            .collect()
    }

    fn preview_candidate(id: &str, fixture: PreviewFixture) -> LocalRoutingReadCandidate {
        let mut runtime = runtime_candidate(id);
        if fixture == PreviewFixture::Cooldown {
            if let Some(health) = &mut runtime.health {
                health.cooldown_until = Some("61000".to_string());
            }
        }

        let request = route_request_facts_for_read_model(
            &RuntimeRoutingSettings {
                policy: RoutingPolicy::PriorityFallback,
                max_rate_multiplier: Some(1.0),
                routing_group_filter: RoutingGroupFilter::AllGroups,
                scheduler_advanced_settings: Default::default(),
                allow_depleted_fallback: false,
            },
            60_000,
        );
        let mut projection =
            route_projection_from_runtime_candidate(&request, runtime.clone()).expect("projection");
        let mut candidate = LocalRoutingReadCandidate {
            station_key_id: id.to_string(),
            station_id: format!("station-{id}"),
            station_name: format!("Station {id}"),
            key_name: format!("Key {id}"),
            schedulable: true,
            capabilities: station_key_capabilities(id),
            health: runtime.health.clone(),
            economics: Some(RouteCandidateEconomics {
                balance_status: Some("normal".to_string()),
                ..Default::default()
            }),
            projection: Some(projection.clone()),
        };

        match fixture {
            PreviewFixture::Eligible => {}
            PreviewFixture::GroupMismatch => {
                projection.policy.group_matches = false;
                projection.hard_rejection_codes = vec!["group_mismatch"];
                candidate.projection = Some(projection);
            }
            PreviewFixture::MultiplierCeiling => {
                projection.multiplier.multiplier = Some(2.0);
                projection.multiplier.selected_source = Some("test");
                projection.multiplier.ceiling_rejected = true;
                projection.hard_rejection_codes = vec!["multiplier_ceiling"];
                candidate.projection = Some(projection);
            }
            PreviewFixture::Cooldown => {
                projection.health.station_key =
                    crate::application::operational_facts::health_projector::HealthAdmission::HardReject;
                projection.hard_rejection_codes = vec!["health_hard_reject"];
                candidate.projection = Some(projection);
            }
            PreviewFixture::Unschedulable => {
                candidate.schedulable = false;
                candidate.projection = Some(projection);
            }
        }

        candidate
    }

    fn runtime_candidate(id: &str) -> RuntimeRoutingCandidate {
        RuntimeRoutingCandidate {
            station_key_id: id.to_string(),
            station_id: format!("station-{id}"),
            station_type: "newapi".to_string(),
            station_account_concurrency_limit: None,
            station_endpoint_revision: 1,
            sanitized_origin: format!("https://{id}.example.test"),
            upstream_api_format: UpstreamApiFormat::CustomOpenAiCompatible,
            routing_order: None,
            priority: 10,
            max_concurrency: 4,
            load_factor: Some(0),
            schedulable: true,
            collector_proxy_mode: "inherit".to_string(),
            collector_proxy_url: None,
            station_name: format!("Station {id}"),
            key_name: format!("Key {id}"),
            capabilities: station_key_capabilities(id),
            health: Some(station_key_health(id)),
            balance_snapshot: None,
            economic_snapshot: None,
            api_key: Some(format!("sk-{id}")),
            api_key_secret: None,
        }
    }

    fn station_key_capabilities(id: &str) -> StationKeyCapabilities {
        StationKeyCapabilities {
            station_key_id: id.to_string(),
            supports_chat_completions: true,
            supports_responses: true,
            supports_embeddings: false,
            supports_stream: true,
            supports_tools: false,
            supports_vision: false,
            supports_reasoning: false,
            model_allowlist: Vec::new(),
            model_blocklist: Vec::new(),
            preferred_models: Vec::new(),
            only_use_as_backup: false,
            routing_tags: Vec::new(),
            updated_at: "0".to_string(),
        }
    }

    fn station_key_health(id: &str) -> StationKeyHealth {
        StationKeyHealth {
            station_key_id: id.to_string(),
            last_success_at: None,
            last_failure_at: None,
            consecutive_failures: 0,
            success_count: 1,
            failure_count: 0,
            avg_latency_ms: None,
            last_error_summary: None,
            cooldown_until: None,
            updated_at: "0".to_string(),
        }
    }
}
