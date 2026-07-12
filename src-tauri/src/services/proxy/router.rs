use std::collections::HashMap;

use crate::{
    models::routing::{
        RouteCandidateExplanation, RouteEndpointKind, RoutingGroupFilter, RoutingPolicy,
        StationKeyCapabilities, StationKeyHealth,
    },
    services::proxy::RouteCandidate,
};

use crate::services::proxy::scheduler::{
    affinity::AffinityStore,
    capacity::CapacityRegistry,
    metrics::RuntimeMetricsRegistry,
    schedule_once,
    types::{
        EffectiveMultiplierFact, ScheduleRequest, SchedulerCandidate, SchedulerCandidateDecision,
    },
};

#[derive(Debug, Clone)]
pub struct RouteRequest {
    pub endpoint: RouteEndpointKind,
    pub model: Option<String>,
    pub stream: bool,
    pub uses_tools: bool,
    pub uses_vision: bool,
    pub uses_reasoning: bool,
    pub policy: RoutingPolicy,
    pub max_rate_multiplier: Option<f64>,
    pub routing_group_filter: RoutingGroupFilter,
    pub session_hash: Option<String>,
    pub previous_response_id: Option<String>,
    pub excluded_key_ids: Vec<String>,
    pub current_station_key_id: Option<String>,
    pub allow_depleted_fallback: bool,
    pub now_ms: i64,
}

#[derive(Debug, Clone)]
pub struct RichRouteCandidate {
    pub candidate: RouteCandidate,
    pub station_name: String,
    pub key_name: String,
    pub capabilities: StationKeyCapabilities,
    pub health: Option<StationKeyHealth>,
    pub economics: Option<RouteCandidateEconomics>,
    pub scheduler_group_binding_id: Option<String>,
    pub scheduler_group_id_hash: Option<String>,
    pub scheduler_group_type: Option<crate::models::routing::PricingGroupType>,
    pub scheduler_effective_multiplier: Option<EffectiveMultiplierFact>,
    pub scheduler_multiplier_reject_reason:
        Option<crate::services::proxy::scheduler::types::MultiplierRejectReason>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct RouteCandidateEconomics {
    pub pricing_rule_id: Option<String>,
    pub pricing_model: Option<String>,
    pub group_binding_id: Option<String>,
    pub rate_multiplier: Option<f64>,
    pub normalization_status: Option<String>,
    pub price_confidence: Option<f64>,
    pub base_input_price: Option<f64>,
    pub base_output_price: Option<f64>,
    pub base_fixed_price: Option<f64>,
    pub estimated_input_price: Option<f64>,
    pub estimated_output_price: Option<f64>,
    pub fixed_price: Option<f64>,
    pub price_currency: Option<String>,
    pub pricing_source: Option<String>,
    pub balance_status: Option<String>,
    pub balance_value: Option<f64>,
    pub low_balance_threshold: Option<f64>,
    pub balance_currency: Option<String>,
    pub balance_scope: Option<String>,
    pub balance_collected_at: Option<String>,
    pub economic_freshness: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RouteSelection {
    pub accepted: Vec<RichRouteCandidate>,
    pub explanations: Vec<RouteCandidateExplanation>,
    pub mapped_model: Option<String>,
    pub scheduler_error_code: Option<String>,
}

pub fn select_route_candidates(
    request: &RouteRequest,
    candidates: Vec<RichRouteCandidate>,
    aliases: &[(String, String)],
) -> Result<RouteSelection, String> {
    if !matches!(request.policy, RoutingPolicy::AutomaticBalanced) {
        return crate::services::proxy::routing_policy::select_route_candidates(
            request, candidates, aliases,
        );
    }

    let max_rate_multiplier = request
        .max_rate_multiplier
        .ok_or_else(|| "routing_multiplier_limit_not_configured".to_string())?;
    if !max_rate_multiplier.is_finite() || max_rate_multiplier < 0.0 {
        return Err("routing_multiplier_limit_not_configured".to_string());
    }

    let mapped_model =
        crate::services::proxy::routing_policy::mapped_model(request.model.as_deref(), aliases);
    let scheduler_request = ScheduleRequest {
        endpoint: request.endpoint.clone(),
        requested_model: request.model.clone(),
        mapped_model: mapped_model.clone(),
        routing_group_filter: request.routing_group_filter.clone(),
        stream: request.stream,
        uses_tools: request.uses_tools,
        uses_vision: request.uses_vision,
        uses_reasoning: request.uses_reasoning,
        max_rate_multiplier,
        session_hash: request.session_hash.clone(),
        previous_response_id: request.previous_response_id.clone(),
        excluded_key_ids: request.excluded_key_ids.clone(),
        now_ms: request.now_ms,
    };
    let scheduler_candidates = candidates
        .iter()
        .map(rich_candidate_to_scheduler_candidate)
        .collect::<Vec<_>>();
    let metrics = RuntimeMetricsRegistry::default();
    let capacity = CapacityRegistry::default();
    let mut affinity = AffinityStore::default();
    let advanced = crate::models::routing::SchedulerAdvancedSettings::default();

    let decision = schedule_once(
        &scheduler_request,
        &scheduler_candidates,
        &metrics,
        &capacity,
        &mut affinity,
        &advanced,
    );

    let candidates_by_id = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.candidate.station_key_id.clone(),
                candidate.clone(),
            )
        })
        .collect::<HashMap<_, _>>();
    let scheduler_candidates_by_id = scheduler_candidates
        .iter()
        .map(|candidate| (candidate.station_key_id.clone(), candidate.clone()))
        .collect::<HashMap<_, _>>();
    let (ordered_station_key_ids, candidate_decisions, scheduler_error_code) = match decision {
        Ok(decision) => (
            decision.ordered_station_key_ids,
            decision.candidate_decisions,
            None,
        ),
        Err(error) => (
            Vec::new(),
            error.candidate_decisions,
            Some(error.code.to_string()),
        ),
    };
    let accepted = ordered_station_key_ids
        .iter()
        .filter_map(|station_key_id| candidates_by_id.get(station_key_id).cloned())
        .collect::<Vec<_>>();
    let explanations = candidate_decisions
        .iter()
        .filter_map(|candidate_decision| {
            let rich_candidate = candidates_by_id.get(&candidate_decision.station_key_id)?;
            let scheduler_candidate =
                scheduler_candidates_by_id.get(&candidate_decision.station_key_id)?;
            Some(automatic_candidate_explanation(
                request,
                rich_candidate,
                scheduler_candidate,
                candidate_decision,
                mapped_model.clone(),
            ))
        })
        .collect();

    Ok(RouteSelection {
        accepted,
        explanations,
        mapped_model,
        scheduler_error_code,
    })
}

fn rich_candidate_to_scheduler_candidate(candidate: &RichRouteCandidate) -> SchedulerCandidate {
    SchedulerCandidate {
        station_key_id: candidate.candidate.station_key_id.clone(),
        station_id: candidate.candidate.station_id.clone(),
        priority: candidate.candidate.priority,
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
        schedulable: true,
        supports_chat_completions: candidate.capabilities.supports_chat_completions,
        supports_responses: candidate.capabilities.supports_responses,
        supports_embeddings: candidate.capabilities.supports_embeddings,
        supports_stream: candidate.capabilities.supports_stream,
        supports_tools: candidate.capabilities.supports_tools,
        supports_vision: candidate.capabilities.supports_vision,
        supports_reasoning: candidate.capabilities.supports_reasoning,
        model_allowlist: candidate.capabilities.model_allowlist.clone(),
        model_blocklist: candidate.capabilities.model_blocklist.clone(),
        health_blocked: candidate
            .health
            .as_ref()
            .and_then(|health| health.cooldown_until.as_deref())
            .and_then(|value| value.parse::<i64>().ok())
            .is_some_and(|cooldown_until| cooldown_until > 1_800_000_000_000),
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
                    station_key_id: candidate.candidate.station_key_id.clone(),
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

fn automatic_candidate_explanation(
    request: &RouteRequest,
    candidate: &RichRouteCandidate,
    scheduler_candidate: &SchedulerCandidate,
    decision: &SchedulerCandidateDecision,
    mapped_model: Option<String>,
) -> RouteCandidateExplanation {
    let economics = candidate.economics.as_ref();
    RouteCandidateExplanation {
        station_key_id: candidate.candidate.station_key_id.clone(),
        station_id: candidate.candidate.station_id.clone(),
        station_name: candidate.station_name.clone(),
        key_name: candidate.key_name.clone(),
        accepted: decision.accepted,
        score: decision
            .score
            .map(|score| (score * 1000.0).round() as i64)
            .unwrap_or(i64::MAX),
        reasons: crate::services::proxy::scheduler::explanation::decision_reasons(decision),
        rejection_reasons: crate::services::proxy::scheduler::explanation::rejection_reason_codes(
            decision,
        ),
        mapped_model,
        pricing_rule_id: economics.and_then(|economics| economics.pricing_rule_id.clone()),
        group_binding_id: scheduler_candidate.group_binding_id.clone(),
        rate_multiplier: decision
            .effective_multiplier
            .as_ref()
            .map(|multiplier| multiplier.value),
        normalization_status: economics
            .and_then(|economics| economics.normalization_status.clone()),
        price_confidence: economics.and_then(|economics| economics.price_confidence),
        estimated_input_price: economics.and_then(|economics| economics.estimated_input_price),
        estimated_output_price: economics.and_then(|economics| economics.estimated_output_price),
        price_currency: economics.and_then(|economics| economics.price_currency.clone()),
        balance_status: economics.and_then(|economics| economics.balance_status.clone()),
        balance_value: economics.and_then(|economics| economics.balance_value),
        balance_scope: economics.and_then(|economics| economics.balance_scope.clone()),
        balance_collected_at: economics
            .and_then(|economics| economics.balance_collected_at.clone()),
        economic_freshness: economics.and_then(|economics| economics.economic_freshness.clone()),
        economic_reasons: Vec::new(),
        routing_group_scope: Some(request.routing_group_filter.clone()),
        routing_group_match: decision.routing_group_match,
        group_id_hash: scheduler_candidate.group_id_hash.clone(),
        group_type: scheduler_candidate.group_type.clone(),
        effective_multiplier_source: decision
            .effective_multiplier
            .as_ref()
            .map(|multiplier| multiplier.source.clone()),
        effective_multiplier_confidence: decision
            .effective_multiplier
            .as_ref()
            .map(|multiplier| multiplier.confidence),
        scheduler_score: decision.score,
        scheduler_factors: decision.factors.clone(),
        top_k_rank: decision.top_k_rank.map(|rank| rank as i64),
        slot_result: decision.slot_result.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::proxy::UpstreamApiFormat;
    use crate::models::routing::{PricingGroupType, RoutingGroupFilter};

    #[test]
    fn selector_rejects_protocol_mismatch() {
        let request = route_request(
            RouteEndpointKind::Responses,
            Some("gpt-4o-mini"),
            true,
            RoutingPolicy::PriorityFallback,
        );
        let candidates = vec![
            rich_candidate(
                "chat-only",
                0,
                capabilities(|capabilities| {
                    capabilities.supports_responses = false;
                    capabilities.supports_chat_completions = true;
                }),
            ),
            rich_candidate(
                "responses",
                10,
                capabilities(|capabilities| {
                    capabilities.supports_responses = true;
                }),
            ),
        ];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

        assert_eq!(selected.accepted[0].candidate.station_key_id, "responses");
        assert!(selected.explanations.iter().any(|item| {
            item.station_key_id == "chat-only"
                && item
                    .rejection_reasons
                    .iter()
                    .any(|reason| reason.contains("does not support responses"))
        }));
    }

    #[test]
    fn legacy_priority_fallback_keeps_existing_candidate_order() {
        let request = route_request(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-4o-mini"),
            false,
            RoutingPolicy::PriorityFallback,
        );
        let candidates = vec![
            rich_candidate("second", 2, capabilities(|_| {})),
            rich_candidate("first", 1, capabilities(|_| {})),
        ];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

        assert_eq!(selected.accepted[0].candidate.station_key_id, "first");
        assert_eq!(selected.accepted[1].candidate.station_key_id, "second");
    }

    #[test]
    fn selector_applies_alias_and_allowlist() {
        let request = route_request(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-4o-mini"),
            false,
            RoutingPolicy::PriorityFallback,
        );
        let aliases = vec![("gpt-4o-mini".to_string(), "openai/gpt-4o-mini".to_string())];
        let candidates = vec![
            rich_candidate(
                "blocked",
                0,
                capabilities(|capabilities| {
                    capabilities.model_allowlist = vec!["other-model".to_string()];
                }),
            ),
            rich_candidate(
                "allowed",
                10,
                capabilities(|capabilities| {
                    capabilities.model_allowlist = vec!["openai/gpt-4o-mini".to_string()];
                }),
            ),
        ];

        let selected = select_route_candidates(&request, candidates, &aliases).expect("selection");

        assert_eq!(selected.mapped_model.as_deref(), Some("openai/gpt-4o-mini"));
        assert_eq!(selected.accepted[0].candidate.station_key_id, "allowed");
    }

    #[test]
    fn selector_skips_cooldown_keys() {
        let request = route_request(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-4o-mini"),
            false,
            RoutingPolicy::PriorityFallback,
        );
        let candidates = vec![
            rich_candidate_with_health(
                "cooldown",
                0,
                capabilities(|_| {}),
                health(|health| {
                    health.cooldown_until = Some("9999999999999".to_string());
                }),
            ),
            rich_candidate("ready", 10, capabilities(|_| {})),
        ];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

        assert_eq!(selected.accepted[0].candidate.station_key_id, "ready");
    }

    #[test]
    fn selector_stable_first_uses_health_signals() {
        let request = route_request(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-4o-mini"),
            false,
            RoutingPolicy::StableFirst,
        );
        let candidates = vec![
            rich_candidate_with_health(
                "flaky",
                0,
                capabilities(|_| {}),
                health(|health| {
                    health.consecutive_failures = 5;
                    health.avg_latency_ms = Some(8_000);
                    health.success_count = 1;
                }),
            ),
            rich_candidate_with_health(
                "stable",
                1,
                capabilities(|_| {}),
                health(|health| {
                    health.consecutive_failures = 0;
                    health.avg_latency_ms = Some(80);
                    health.success_count = 100;
                }),
            ),
        ];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

        assert_eq!(selected.accepted[0].candidate.station_key_id, "stable");
    }

    #[test]
    fn selector_orders_backup_only_after_primary_candidates() {
        let request = route_request(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-4o-mini"),
            false,
            RoutingPolicy::BackupOnly,
        );
        let candidates = vec![
            rich_candidate(
                "backup",
                0,
                capabilities(|capabilities| {
                    capabilities.only_use_as_backup = true;
                }),
            ),
            rich_candidate("primary", 10, capabilities(|_| {})),
        ];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

        assert_eq!(selected.accepted[0].candidate.station_key_id, "primary");
        assert_eq!(selected.accepted[1].candidate.station_key_id, "backup");
    }

    #[test]
    fn selector_cheap_first_prefers_lower_estimated_cost_when_priority_matches() {
        let request = route_request(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-5.4"),
            false,
            RoutingPolicy::CheapFirst,
        );
        let candidates = vec![
            rich_candidate_with_economics(
                "expensive",
                0,
                capabilities(|_| {}),
                economics(Some(0.45), Some(1.80), Some(6.0), Some(2.0), "normal"),
            ),
            rich_candidate_with_economics(
                "cheap",
                0,
                capabilities(|_| {}),
                economics(Some(0.08), Some(0.22), Some(28.0), Some(1.0), "normal"),
            ),
        ];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

        assert_eq!(selected.accepted[0].candidate.station_key_id, "cheap");
        assert!(selected
            .explanations
            .iter()
            .any(|item| item.station_key_id == "cheap"
                && item
                    .economic_reasons
                    .iter()
                    .any(|reason| reason.contains("lower estimated cost"))));
    }

    #[test]
    fn cheap_first_does_not_treat_group_rate_only_as_complete_pricing() {
        let request = route_request(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-5.4"),
            false,
            RoutingPolicy::CheapFirst,
        );
        let mut group_rate_only = economics(None, None, Some(50.0), Some(1.0), "normal");
        group_rate_only.normalization_status = Some("group_rate_only".to_string());
        group_rate_only.rate_multiplier = Some(0.1);
        group_rate_only.group_binding_id = Some("group-pro".to_string());

        let candidates = vec![
            rich_candidate_with_economics("rate-only", 0, capabilities(|_| {}), group_rate_only),
            rich_candidate_with_economics(
                "complete-price",
                0,
                capabilities(|_| {}),
                economics(Some(0.20), Some(0.40), Some(20.0), Some(1.0), "normal"),
            ),
        ];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

        assert_eq!(
            selected.accepted[0].candidate.station_key_id,
            "complete-price"
        );
        assert!(selected.explanations.iter().any(|item| {
            item.station_key_id == "rate-only"
                && item.normalization_status.as_deref() == Some("group_rate_only")
                && item.economic_reasons.iter().any(|reason| {
                    reason.contains("only group rate is available; exact price unknown")
                })
        }));
    }

    #[test]
    fn balance_depleted_is_rejected_unless_fallback_allowed() {
        let request = route_request(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-5.4"),
            false,
            RoutingPolicy::PriorityFallback,
        );
        let candidates = vec![rich_candidate_with_economics(
            "empty",
            0,
            capabilities(|_| {}),
            economics(Some(0.20), Some(0.40), Some(0.0), Some(1.0), "depleted"),
        )];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

        assert!(selected.accepted.is_empty());
        assert!(selected.explanations.iter().any(|item| {
            item.station_key_id == "empty"
                && item.rejection_reasons.iter().any(|reason| {
                    reason.contains("balance depleted and depleted fallback is disabled")
                })
        }));

        let mut allowed_request = request.clone();
        allowed_request.allow_depleted_fallback = true;
        let selected = select_route_candidates(&allowed_request, selected_candidate_fixture(), &[])
            .expect("selection");
        assert_eq!(selected.accepted[0].candidate.station_key_id, "empty");
    }

    #[test]
    fn cost_stable_first_keeps_current_key_when_price_delta_is_small() {
        let mut request = route_request(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-5.4"),
            false,
            RoutingPolicy::CostStableFirst,
        );
        request.current_station_key_id = Some("current".to_string());
        let candidates = vec![
            rich_candidate_with_economics(
                "cheaper",
                1,
                capabilities(|_| {}),
                economics(Some(0.005), Some(0.006), Some(20.0), Some(1.0), "normal"),
            ),
            rich_candidate_with_economics(
                "current",
                2,
                capabilities(|_| {}),
                economics(Some(0.006), Some(0.006), Some(20.0), Some(1.0), "normal"),
            ),
        ];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

        assert_eq!(selected.accepted[0].candidate.station_key_id, "current");
        assert!(selected.explanations.iter().any(|item| {
            item.station_key_id == "current"
                && item
                    .economic_reasons
                    .iter()
                    .any(|reason| reason.contains("stability"))
        }));
    }

    #[test]
    fn cost_stable_first_switches_when_current_key_has_hard_failure() {
        let mut request = route_request(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-5.4"),
            false,
            RoutingPolicy::CostStableFirst,
        );
        request.current_station_key_id = Some("current".to_string());
        let candidates = vec![
            rich_candidate_with_economics(
                "fallback",
                1,
                capabilities(|_| {}),
                economics(Some(0.007), Some(0.004), Some(20.0), Some(1.0), "normal"),
            ),
            rich_candidate_with_economics(
                "current",
                2,
                capabilities(|capabilities| {
                    capabilities.model_blocklist = vec!["gpt-5.4".to_string()];
                }),
                economics(Some(0.006), Some(0.006), Some(20.0), Some(1.0), "normal"),
            ),
        ];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

        assert_eq!(selected.accepted[0].candidate.station_key_id, "fallback");
        assert!(selected.explanations.iter().any(|item| {
            item.station_key_id == "current"
                && item
                    .rejection_reasons
                    .iter()
                    .any(|reason| reason.contains("blocklisted"))
        }));
    }

    #[test]
    fn cost_stable_first_switches_when_price_delta_is_significant() {
        let mut request = route_request(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-5.4"),
            false,
            RoutingPolicy::CostStableFirst,
        );
        request.current_station_key_id = Some("current".to_string());
        let candidates = vec![
            rich_candidate_with_economics(
                "current",
                1,
                capabilities(|_| {}),
                economics(Some(0.020), Some(0.020), Some(20.0), Some(1.0), "normal"),
            ),
            rich_candidate_with_economics(
                "cheaper",
                2,
                capabilities(|_| {}),
                economics(Some(0.005), Some(0.006), Some(20.0), Some(1.0), "normal"),
            ),
        ];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

        assert_eq!(selected.accepted[0].candidate.station_key_id, "cheaper");
    }

    #[test]
    fn automatic_router_rejects_all_keys_above_multiplier_ceiling() {
        let request = automatic_route_request(|request| {
            request.max_rate_multiplier = None;
        });

        let error = select_route_candidates(&request, selected_candidate_fixture(), &[])
            .expect_err("missing route/request multiplier limit should reject");

        assert_eq!(error, "routing_multiplier_limit_not_configured");

        let request = automatic_route_request(|request| {
            request.max_rate_multiplier = Some(1.0);
        });
        let candidates = vec![
            rich_scheduler_candidate("expensive-a", 0, Some(2.0), Some(PricingGroupType::Gpt)),
            rich_scheduler_candidate("expensive-b", 1, Some(3.0), Some(PricingGroupType::Gpt)),
        ];

        let selected = select_route_candidates(&request, candidates, &[])
            .expect("over-ceiling automatic candidates should return structured rejection");

        assert!(selected.accepted.is_empty());
        assert_eq!(
            selected.scheduler_error_code.as_deref(),
            Some("routing_no_candidate_within_multiplier_limit")
        );
        assert_eq!(selected.explanations.len(), 2);
    }

    #[test]
    fn automatic_router_group_type_filter_does_not_cross_groups() {
        let request = automatic_route_request(|request| {
            request.max_rate_multiplier = Some(2.0);
            request.routing_group_filter = RoutingGroupFilter::GroupType(PricingGroupType::Gpt);
        });
        let candidates = vec![
            rich_scheduler_candidate("claude-cheap", 0, Some(0.1), Some(PricingGroupType::Claude)),
            rich_scheduler_candidate("gpt-ok", 1, Some(1.0), Some(PricingGroupType::Gpt)),
        ];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

        assert_eq!(selected.accepted[0].candidate.station_key_id, "gpt-ok");
        assert!(selected.explanations.iter().any(|candidate| {
            candidate.station_key_id == "claude-cheap"
                && !candidate.accepted
                && !candidate.routing_group_match
                && candidate
                    .rejection_reasons
                    .iter()
                    .any(|reason| reason == "routing_group_mismatch")
        }));
    }

    #[test]
    fn automatic_router_unknown_multiplier_evidence_rejects() {
        let request = automatic_route_request(|request| {
            request.max_rate_multiplier = Some(1.0);
        });
        let candidates = vec![rich_scheduler_candidate(
            "unknown-multiplier",
            0,
            None,
            Some(PricingGroupType::Gpt),
        )];

        let selected = select_route_candidates(&request, candidates, &[])
            .expect("missing multiplier evidence should return structured rejection");

        assert!(selected.accepted.is_empty());
        assert_eq!(
            selected.scheduler_error_code.as_deref(),
            Some("routing_no_multiplier_evidence")
        );
        assert!(selected.explanations.iter().any(|candidate| {
            candidate.station_key_id == "unknown-multiplier"
                && candidate
                    .rejection_reasons
                    .iter()
                    .any(|reason| reason == "no_multiplier_evidence")
        }));
    }

    #[test]
    fn automatic_router_simulation_explanation_contains_scheduler_facts_without_api_key() {
        let request = automatic_route_request(|request| {
            request.max_rate_multiplier = Some(2.0);
            request.routing_group_filter = RoutingGroupFilter::GroupType(PricingGroupType::Gpt);
        });
        let candidates = vec![rich_scheduler_candidate(
            "secret-bearing",
            0,
            Some(0.5),
            Some(PricingGroupType::Gpt),
        )];

        let selected = select_route_candidates(&request, candidates, &[]).expect("selection");
        let explanation = selected
            .explanations
            .iter()
            .find(|candidate| candidate.station_key_id == "secret-bearing")
            .expect("candidate explanation");

        assert!(explanation.accepted);
        assert!(explanation.routing_group_match);
        assert_eq!(explanation.rate_multiplier, Some(0.5));
        assert_eq!(
            explanation.effective_multiplier_source.as_deref(),
            Some("test")
        );
        assert!(explanation.scheduler_score.is_some());
        assert!(explanation.top_k_rank.is_some());

        let serialized = serde_json::to_string(explanation).expect("serialize explanation");
        assert!(!serialized.contains("sk-secret-bearing"));
        assert!(!serialized.contains("api_key"));
    }

    #[test]
    fn automatic_router_group_scope_without_matching_candidate_rejects_stable_code() {
        let request = automatic_route_request(|request| {
            request.max_rate_multiplier = Some(2.0);
            request.routing_group_filter = RoutingGroupFilter::GroupType(PricingGroupType::Gpt);
        });
        let candidates = vec![rich_scheduler_candidate(
            "claude-only",
            0,
            Some(0.1),
            Some(PricingGroupType::Claude),
        )];

        let selected = select_route_candidates(&request, candidates, &[])
            .expect("group-scoped automatic routing should return structured rejection");

        assert!(selected.accepted.is_empty());
        assert_eq!(
            selected.scheduler_error_code.as_deref(),
            Some("routing_no_candidate_in_group_scope")
        );
    }

    #[test]
    fn automatic_router_preserves_over_ceiling_code_after_request_exclusions() {
        let request = automatic_route_request(|request| {
            request.max_rate_multiplier = Some(1.0);
            request.excluded_key_ids = vec!["failed-key".to_string()];
        });
        let selected = select_route_candidates(
            &request,
            vec![
                rich_scheduler_candidate("failed-key", 0, Some(0.5), Some(PricingGroupType::Gpt)),
                rich_scheduler_candidate(
                    "expensive-key",
                    10,
                    Some(2.0),
                    Some(PricingGroupType::Gpt),
                ),
            ],
            &[],
        )
        .expect("automatic route should return structured rejection");

        assert!(selected.accepted.is_empty());
        assert_eq!(
            selected.scheduler_error_code.as_deref(),
            Some("routing_no_candidate_within_multiplier_limit")
        );
    }

    #[test]
    fn automatic_router_applies_same_model_alias_without_substituting_logical_model() {
        let request = automatic_route_request(|request| {
            request.max_rate_multiplier = Some(2.0);
            request.model = Some("client-model".to_string());
        });
        let aliases = vec![("client-model".to_string(), "upstream-model".to_string())];
        let candidates = vec![rich_scheduler_candidate_with_capabilities(
            "client-allowlisted",
            0,
            Some(0.5),
            Some(PricingGroupType::Gpt),
            capabilities(|capabilities| {
                capabilities.model_allowlist = vec!["upstream-model".to_string()];
            }),
        )];

        let selected = select_route_candidates(&request, candidates, &aliases).expect("selection");

        assert_eq!(
            selected.accepted[0].candidate.station_key_id,
            "client-allowlisted"
        );
        assert_eq!(selected.mapped_model.as_deref(), Some("upstream-model"));
        assert!(selected
            .explanations
            .iter()
            .all(|explanation| { explanation.mapped_model.as_deref() == Some("upstream-model") }));
    }

    fn route_request(
        endpoint: RouteEndpointKind,
        model: Option<&str>,
        stream: bool,
        policy: RoutingPolicy,
    ) -> RouteRequest {
        RouteRequest {
            endpoint,
            model: model.map(ToString::to_string),
            stream,
            uses_tools: false,
            uses_vision: false,
            uses_reasoning: false,
            policy,
            max_rate_multiplier: None,
            routing_group_filter: RoutingGroupFilter::AllGroups,
            session_hash: None,
            previous_response_id: None,
            excluded_key_ids: Vec::new(),
            current_station_key_id: None,
            allow_depleted_fallback: false,
            now_ms: 1_800_000_000_000,
        }
    }

    fn automatic_route_request(configure: impl FnOnce(&mut RouteRequest)) -> RouteRequest {
        let mut request = route_request(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-4o"),
            false,
            RoutingPolicy::AutomaticBalanced,
        );
        request.max_rate_multiplier = Some(1.0);
        request.routing_group_filter = RoutingGroupFilter::AllGroups;
        configure(&mut request);
        request
    }

    fn rich_scheduler_candidate(
        id: &str,
        priority: i64,
        multiplier: Option<f64>,
        group_type: Option<PricingGroupType>,
    ) -> RichRouteCandidate {
        rich_scheduler_candidate_with_capabilities(
            id,
            priority,
            multiplier,
            group_type,
            capabilities(|_| {}),
        )
    }

    fn rich_scheduler_candidate_with_capabilities(
        id: &str,
        priority: i64,
        multiplier: Option<f64>,
        group_type: Option<PricingGroupType>,
        capabilities: StationKeyCapabilities,
    ) -> RichRouteCandidate {
        let mut candidate = rich_candidate_with_health(id, priority, capabilities, None);
        candidate.scheduler_group_binding_id = Some(format!("binding-{id}"));
        candidate.scheduler_group_id_hash = Some(format!("hash-{id}"));
        candidate.scheduler_group_type = group_type;
        candidate.scheduler_effective_multiplier =
            multiplier.map(|value| EffectiveMultiplierFact {
                station_key_id: id.to_string(),
                value,
                source: "test".to_string(),
                collected_at_ms: Some(1_700_000_000_000),
                valid_until_ms: Some(1_900_000_000_000),
                confidence: 1.0,
                group_binding_id: Some(format!("binding-{id}")),
            });
        candidate
    }

    fn rich_candidate(
        id: &str,
        priority: i64,
        capabilities: StationKeyCapabilities,
    ) -> RichRouteCandidate {
        rich_candidate_with_health(id, priority, capabilities, None)
    }

    fn rich_candidate_with_health(
        id: &str,
        priority: i64,
        capabilities: StationKeyCapabilities,
        health: Option<StationKeyHealth>,
    ) -> RichRouteCandidate {
        RichRouteCandidate {
            candidate: RouteCandidate {
                station_key_id: id.to_string(),
                station_id: format!("station-{id}"),
                upstream_base_url: "https://example.test/v1".to_string(),
                api_key: format!("sk-{id}"),
                upstream_api_format: UpstreamApiFormat::Auto,
                priority,
                collector_proxy_mode: "direct".to_string(),
                collector_proxy_url: None,
            },
            station_name: format!("Station {id}"),
            key_name: format!("Key {id}"),
            capabilities,
            health,
            economics: None,
            scheduler_group_binding_id: None,
            scheduler_group_id_hash: None,
            scheduler_group_type: None,
            scheduler_effective_multiplier: None,
            scheduler_multiplier_reject_reason: None,
        }
    }

    fn rich_candidate_with_economics(
        id: &str,
        priority: i64,
        capabilities: StationKeyCapabilities,
        economics: RouteCandidateEconomics,
    ) -> RichRouteCandidate {
        let mut candidate = rich_candidate_with_health(id, priority, capabilities, None);
        candidate.economics = Some(economics);
        candidate
    }

    fn capabilities(configure: impl FnOnce(&mut StationKeyCapabilities)) -> StationKeyCapabilities {
        let mut capabilities = StationKeyCapabilities {
            station_key_id: "key".to_string(),
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
        };
        configure(&mut capabilities);
        capabilities
    }

    fn economics(
        estimated_input_price: Option<f64>,
        estimated_output_price: Option<f64>,
        balance_value: Option<f64>,
        low_balance_threshold: Option<f64>,
        balance_status: &'static str,
    ) -> RouteCandidateEconomics {
        RouteCandidateEconomics {
            pricing_rule_id: Some("price-test".to_string()),
            pricing_model: Some("gpt-5.4".to_string()),
            group_binding_id: None,
            rate_multiplier: None,
            normalization_status: Some("complete".to_string()),
            price_confidence: Some(0.9),
            base_input_price: estimated_input_price,
            base_output_price: estimated_output_price,
            base_fixed_price: None,
            estimated_input_price,
            estimated_output_price,
            fixed_price: None,
            price_currency: Some("USD".to_string()),
            pricing_source: Some("manual".to_string()),
            balance_status: Some(balance_status.to_string()),
            balance_value,
            low_balance_threshold,
            balance_currency: Some("USD".to_string()),
            balance_scope: Some("station".to_string()),
            balance_collected_at: Some("1000".to_string()),
            economic_freshness: Some("fresh".to_string()),
        }
    }

    fn selected_candidate_fixture() -> Vec<RichRouteCandidate> {
        vec![rich_candidate_with_economics(
            "empty",
            0,
            capabilities(|_| {}),
            economics(Some(0.20), Some(0.40), Some(0.0), Some(1.0), "depleted"),
        )]
    }

    fn health(configure: impl FnOnce(&mut StationKeyHealth)) -> Option<StationKeyHealth> {
        let mut health = StationKeyHealth {
            station_key_id: "key".to_string(),
            last_success_at: None,
            last_failure_at: None,
            consecutive_failures: 0,
            success_count: 0,
            failure_count: 0,
            avg_latency_ms: None,
            last_error_summary: None,
            cooldown_until: None,
            updated_at: "0".to_string(),
        };
        configure(&mut health);
        Some(health)
    }
}
