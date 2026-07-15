use crate::{
    models::{
        proxy::{ProxyStatus, RequestLog},
        routing::{
            PricingGroupType, RouteEndpointKind, RoutingGroupFilter, StationKeyCapabilities,
            StationKeyHealth,
        },
    },
    services::{
        database::{now_millis_for_services, AppDatabase},
        proxy::{
            router::RouteCandidateEconomics,
            routing_health::{error_summary_indicates_offline, health_is_blocked},
            routing_types::{
                DecisionFact, DecisionFactKind, DecisionFactSeverity, LocalRoutingCandidateRow,
                LocalRoutingPreviewKind, LocalRoutingSettingsView, LocalRoutingSummary,
                LocalRoutingWorkspace, RouteDecisionEvent, RouteDecisionStatus,
                RouteDecisionSummary, RouteHealthState,
            },
            scheduler::{
                eligibility::evaluate_candidate,
                explanation::rejection_code_label,
                types::{
                    EffectiveMultiplierFact, MultiplierRejectReason, ScheduleRequest,
                    SchedulerCandidate,
                },
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
    pub(crate) schedulable: bool,
    pub(crate) capabilities: StationKeyCapabilities,
    pub(crate) health: Option<StationKeyHealth>,
    pub(crate) economics: Option<RouteCandidateEconomics>,
    pub(crate) scheduler_group_binding_id: Option<String>,
    pub(crate) scheduler_group_id_hash: Option<String>,
    pub(crate) scheduler_group_type: Option<PricingGroupType>,
    pub(crate) scheduler_effective_multiplier: Option<EffectiveMultiplierFact>,
    pub(crate) scheduler_multiplier_reject_reason: Option<MultiplierRejectReason>,
}

pub fn load_local_routing_workspace(
    database: &AppDatabase,
    proxy_status: ProxyStatus,
) -> Result<LocalRoutingWorkspace, String> {
    let settings = database.get_settings()?;
    let candidates = database.local_routing_read_candidates()?;
    let request_logs = database.list_local_proxy_request_logs()?;
    let latest_log = request_logs.first();
    let now_ms = now_millis_for_services() as i64;
    let preview_request = settings
        .max_rate_multiplier
        .filter(|limit| limit.is_finite() && *limit >= 0.0)
        .map(|limit| {
            preview_schedule_request(settings.default_routing_group_filter.clone(), limit, now_ms)
        });
    let rows = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            candidate_row(
                index,
                candidate,
                &settings.default_routing_group_filter,
                preview_request.as_ref(),
                now_ms,
            )
        })
        .collect::<Vec<_>>();

    let latest_decision = latest_log.map(|log| latest_decision(log, &rows));

    Ok(LocalRoutingWorkspace {
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
    })
}

fn candidate_row(
    index: usize,
    candidate: &LocalRoutingReadCandidate,
    routing_group_filter: &RoutingGroupFilter,
    preview_request: Option<&ScheduleRequest>,
    now_ms: i64,
) -> LocalRoutingCandidateRow {
    let health_state = health_state(candidate, now_ms);
    let routing_group_match = local_candidate_group_matches(routing_group_filter, candidate);
    let scheduler_candidate = scheduler_candidate_from_read_candidate(candidate, now_ms);
    let (preview_eligible, preview_reject_reasons) =
        preview_decision(preview_request, &scheduler_candidate);
    let preview_reject_reasons = if candidate.schedulable {
        preview_reject_reasons
    } else {
        let mut reasons = preview_reject_reasons;
        if !reasons.iter().any(|reason| reason == "asset_unavailable") {
            reasons.insert(0, "asset_unavailable".to_string());
        }
        reasons
    };
    let preview_eligible = candidate.schedulable && preview_eligible;
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
    if let Some(multiplier) = &candidate.scheduler_effective_multiplier {
        facts.push(DecisionFact {
            kind: DecisionFactKind::Pricing,
            label: "Effective multiplier".to_string(),
            value: format!("{:.4}x via {}", multiplier.value, multiplier.source),
            severity: DecisionFactSeverity::Info,
        });
    } else if let Some(reason) = candidate.scheduler_multiplier_reject_reason {
        facts.push(DecisionFact {
            kind: DecisionFactKind::Pricing,
            label: "Multiplier evidence".to_string(),
            value: multiplier_reject_reason_label(reason).to_string(),
            severity: DecisionFactSeverity::Warning,
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
        score: None,
        effective_multiplier: candidate
            .scheduler_effective_multiplier
            .as_ref()
            .map(|multiplier| multiplier.value),
        effective_multiplier_source: candidate
            .scheduler_effective_multiplier
            .as_ref()
            .map(|multiplier| multiplier.source.clone()),
        effective_multiplier_confidence: candidate
            .scheduler_effective_multiplier
            .as_ref()
            .map(|multiplier| multiplier.confidence),
        routing_group_scope: routing_group_filter.clone(),
        routing_group_match,
        scheduler_reject_reason: candidate
            .scheduler_multiplier_reject_reason
            .map(|reason| multiplier_reject_reason_label(reason).to_string()),
        preview_eligible,
        preview_reject_reasons,
        facts,
    }
}

fn preview_schedule_request(
    filter: RoutingGroupFilter,
    max_rate_multiplier: f64,
    now_ms: i64,
) -> ScheduleRequest {
    ScheduleRequest {
        endpoint: RouteEndpointKind::ChatCompletions,
        requested_model: None,
        mapped_model: None,
        routing_group_filter: filter,
        stream: false,
        uses_tools: false,
        uses_vision: false,
        uses_reasoning: false,
        max_rate_multiplier,
        session_hash: None,
        previous_response_id: None,
        excluded_key_ids: Vec::new(),
        now_ms,
    }
}

fn preview_decision(
    request: Option<&ScheduleRequest>,
    candidate: &SchedulerCandidate,
) -> (bool, Vec<String>) {
    let Some(request) = request else {
        return (
            false,
            vec!["routing_multiplier_limit_not_configured".to_string()],
        );
    };

    match evaluate_candidate(request, candidate) {
        Ok(()) => (true, Vec::new()),
        Err(rejection) => (
            false,
            rejection
                .reasons
                .into_iter()
                .map(rejection_code_label)
                .map(str::to_string)
                .collect(),
        ),
    }
}

fn scheduler_candidate_from_read_candidate(
    candidate: &LocalRoutingReadCandidate,
    now_ms: i64,
) -> SchedulerCandidate {
    SchedulerCandidate {
        station_key_id: candidate.station_key_id.clone(),
        station_id: candidate.station_id.clone(),
        priority: 0,
        max_concurrency: 0,
        load_factor: None,
        group_binding_id: candidate.scheduler_group_binding_id.clone().or_else(|| {
            candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.group_binding_id.clone())
        }),
        group_id_hash: candidate.scheduler_group_id_hash.clone(),
        group_type: candidate.scheduler_group_type.clone(),
        station_enabled: true,
        key_enabled: true,
        schedulable: candidate.schedulable,
        supports_chat_completions: candidate.capabilities.supports_chat_completions,
        supports_responses: candidate.capabilities.supports_responses,
        supports_embeddings: candidate.capabilities.supports_embeddings,
        supports_stream: candidate.capabilities.supports_stream,
        supports_tools: candidate.capabilities.supports_tools,
        supports_vision: candidate.capabilities.supports_vision,
        supports_reasoning: candidate.capabilities.supports_reasoning,
        model_allowlist: candidate.capabilities.model_allowlist.clone(),
        model_blocklist: candidate.capabilities.model_blocklist.clone(),
        health_blocked: health_is_blocked(candidate.health.as_ref(), now_ms),
        balance_depleted: candidate
            .economics
            .as_ref()
            .and_then(|economics| economics.balance_status.as_deref())
            .map(|status| matches!(status, "depleted" | "insufficient" | "blocked"))
            .unwrap_or(false),
        effective_multiplier: candidate
            .scheduler_effective_multiplier
            .clone()
            .or_else(|| {
                let economics = candidate.economics.as_ref()?;
                let value = economics.rate_multiplier?;
                Some(EffectiveMultiplierFact {
                    station_key_id: candidate.station_key_id.clone(),
                    value,
                    source: economics
                        .pricing_source
                        .clone()
                        .unwrap_or_else(|| "economics".to_string()),
                    collected_at_ms: economics
                        .balance_collected_at
                        .as_deref()
                        .and_then(|value| value.parse::<i64>().ok()),
                    valid_until_ms: None,
                    confidence: economics.price_confidence.unwrap_or(1.0),
                    group_binding_id: economics.group_binding_id.clone(),
                })
            }),
        multiplier_reject_reason: candidate.scheduler_multiplier_reject_reason,
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
    candidate: &LocalRoutingReadCandidate,
) -> bool {
    match filter {
        RoutingGroupFilter::AllGroups => true,
        RoutingGroupFilter::UngroupedOnly => {
            candidate.scheduler_group_binding_id.is_none()
                && candidate.scheduler_group_id_hash.is_none()
        }
        RoutingGroupFilter::GroupBindingId(expected) => {
            candidate.scheduler_group_binding_id.as_deref() == Some(expected.as_str())
        }
        RoutingGroupFilter::GroupIdHash(expected) => {
            candidate.scheduler_group_id_hash.as_deref() == Some(expected.as_str())
        }
        RoutingGroupFilter::GroupType(expected) => {
            candidate.scheduler_group_type.as_ref() == Some(expected)
        }
    }
}

fn multiplier_reject_reason_label(reason: MultiplierRejectReason) -> &'static str {
    match reason {
        MultiplierRejectReason::Missing => "missing",
        MultiplierRejectReason::Invalid => "invalid",
        MultiplierRejectReason::Negative => "negative",
        MultiplierRejectReason::Expired => "expired",
        MultiplierRejectReason::UnboundGroup => "unbound_group",
        MultiplierRejectReason::LowConfidence => "low_confidence",
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

    #[test]
    fn preview_summary_counts_the_same_decisions_exposed_on_rows() {
        let rows = preview_rows_for_test(
            vec![
                preview_candidate("eligible", PreviewFixture::Eligible),
                preview_candidate("group-mismatch", PreviewFixture::GroupMismatch),
                preview_candidate("low-confidence", PreviewFixture::LowConfidence),
                preview_candidate("cooldown", PreviewFixture::Cooldown),
            ],
            Some(1.0),
        );

        assert!(rows[0].preview_eligible);
        assert_eq!(
            rows[1].preview_reject_reasons,
            vec!["routing_group_mismatch"]
        );
        assert_eq!(
            rows[2].preview_reject_reasons,
            vec!["multiplier_evidence_low_confidence"],
        );
        assert_eq!(rows[3].preview_reject_reasons, vec!["health_blocked"]);

        let summary = build_local_routing_summary(&rows, None);
        assert_eq!(summary.candidate_count, 4);
        assert_eq!(summary.preview_eligible_candidate_count, 1);
        assert_eq!(summary.preview_excluded_candidate_count, 3);
        assert_eq!(summary.cooldown_candidate_count, 1);
    }

    #[test]
    fn missing_multiplier_limit_blocks_preview_without_guessing() {
        let rows = preview_rows_for_test(
            vec![
                preview_candidate("eligible", PreviewFixture::Eligible),
                preview_candidate("group-mismatch", PreviewFixture::GroupMismatch),
            ],
            None,
        );

        assert!(rows.iter().all(|row| !row.preview_eligible));
        assert!(rows.iter().all(|row| {
            row.preview_reject_reasons == vec!["routing_multiplier_limit_not_configured"]
        }));
    }

    #[test]
    fn unschedulable_candidate_preview_is_paused_once() {
        let rows = preview_rows_for_test(
            vec![preview_candidate("paused", PreviewFixture::Unschedulable)],
            Some(1.0),
        );

        assert!(!rows[0].schedulable);
        assert!(!rows[0].preview_eligible);
        assert_eq!(rows[0].preview_reject_reasons, vec!["asset_unavailable"]);
    }

    #[derive(Debug, Clone, Copy)]
    enum PreviewFixture {
        Eligible,
        GroupMismatch,
        LowConfidence,
        Cooldown,
        Unschedulable,
    }

    fn preview_rows_for_test(
        candidates: Vec<LocalRoutingReadCandidate>,
        max_rate_multiplier: Option<f64>,
    ) -> Vec<LocalRoutingCandidateRow> {
        let request = max_rate_multiplier.map(|limit| {
            preview_schedule_request(
                RoutingGroupFilter::GroupType(PricingGroupType::Gpt),
                limit,
                60_000,
            )
        });

        candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                candidate_row(
                    index,
                    candidate,
                    &RoutingGroupFilter::GroupType(PricingGroupType::Gpt),
                    request.as_ref(),
                    60_000,
                )
            })
            .collect()
    }

    fn preview_candidate(id: &str, fixture: PreviewFixture) -> LocalRoutingReadCandidate {
        let mut candidate = LocalRoutingReadCandidate {
            station_key_id: id.to_string(),
            station_id: format!("station-{id}"),
            station_name: format!("Station {id}"),
            key_name: format!("Key {id}"),
            schedulable: true,
            capabilities: station_key_capabilities(id),
            health: Some(station_key_health(id)),
            economics: Some(RouteCandidateEconomics {
                balance_status: Some("normal".to_string()),
                ..Default::default()
            }),
            scheduler_group_binding_id: Some(format!("binding-{id}")),
            scheduler_group_id_hash: Some(format!("hash-{id}")),
            scheduler_group_type: Some(PricingGroupType::Gpt),
            scheduler_effective_multiplier: Some(EffectiveMultiplierFact {
                station_key_id: id.to_string(),
                value: 0.5,
                source: "test".to_string(),
                collected_at_ms: Some(1_000),
                valid_until_ms: Some(120_000),
                confidence: 1.0,
                group_binding_id: Some(format!("binding-{id}")),
            }),
            scheduler_multiplier_reject_reason: None,
        };

        match fixture {
            PreviewFixture::Eligible => {}
            PreviewFixture::GroupMismatch => {
                candidate.scheduler_group_type = Some(PricingGroupType::Claude);
            }
            PreviewFixture::LowConfidence => {
                candidate.scheduler_effective_multiplier = None;
                candidate.scheduler_multiplier_reject_reason =
                    Some(MultiplierRejectReason::LowConfidence);
            }
            PreviewFixture::Cooldown => {
                if let Some(health) = &mut candidate.health {
                    health.cooldown_until = Some("61000".to_string());
                }
            }
            PreviewFixture::Unschedulable => {
                candidate.schedulable = false;
            }
        }

        candidate
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
