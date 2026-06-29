use crate::{
    models::routing::{
        RouteCandidateExplanation, RouteEndpointKind, RoutingPolicy, StationKeyCapabilities,
        StationKeyHealth,
    },
    services::proxy::RouteCandidate,
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
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct RouteCandidateEconomics {
    pub pricing_rule_id: Option<String>,
    pub pricing_model: Option<String>,
    pub estimated_input_price: Option<f64>,
    pub estimated_output_price: Option<f64>,
    pub fixed_price: Option<f64>,
    pub price_currency: Option<String>,
    pub pricing_source: Option<String>,
    pub balance_status: Option<String>,
    pub balance_value: Option<f64>,
    pub low_balance_threshold: Option<f64>,
    pub balance_currency: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RouteSelection {
    pub accepted: Vec<RichRouteCandidate>,
    pub explanations: Vec<RouteCandidateExplanation>,
    pub mapped_model: Option<String>,
}

pub fn select_route_candidates(
    request: &RouteRequest,
    candidates: Vec<RichRouteCandidate>,
    aliases: &[(String, String)],
) -> Result<RouteSelection, String> {
    let mapped_model = mapped_model(request.model.as_deref(), aliases);
    let mut accepted = Vec::new();
    let mut explanations = Vec::new();

    for candidate in candidates {
        let mut reasons = Vec::new();
        let mut rejection_reasons = Vec::new();

        collect_rejections(
            request,
            &candidate,
            mapped_model.as_deref(),
            &mut reasons,
            &mut rejection_reasons,
        );

        let score = if rejection_reasons.is_empty() {
            candidate_score(request, &candidate, mapped_model.as_deref())
        } else {
            i64::MAX
        };

        let explanation = RouteCandidateExplanation {
            station_key_id: candidate.candidate.station_key_id.clone(),
            station_id: candidate.candidate.station_id.clone(),
            station_name: candidate.station_name.clone(),
            key_name: candidate.key_name.clone(),
            accepted: rejection_reasons.is_empty(),
            score,
            reasons,
            rejection_reasons,
            mapped_model: mapped_model.clone(),
            pricing_rule_id: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.pricing_rule_id.clone()),
            estimated_input_price: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.estimated_input_price),
            estimated_output_price: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.estimated_output_price),
            price_currency: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.price_currency.clone()),
            balance_status: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.balance_status.clone()),
            balance_value: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.balance_value),
            economic_reasons: candidate_economic_reasons(&candidate, request),
        };

        if explanation.accepted {
            accepted.push((score, candidate));
        }
        explanations.push(explanation);
    }

    accepted.sort_by_key(|(score, candidate)| {
        (
            *score,
            candidate.candidate.priority,
            candidate.candidate.station_key_id.clone(),
        )
    });

    Ok(RouteSelection {
        accepted: accepted
            .into_iter()
            .map(|(_, candidate)| candidate)
            .collect(),
        explanations,
        mapped_model,
    })
}

fn mapped_model(model: Option<&str>, aliases: &[(String, String)]) -> Option<String> {
    let model = model?;
    aliases
        .iter()
        .find_map(|(client_model, upstream_model)| {
            (client_model == model).then(|| upstream_model.clone())
        })
        .or_else(|| Some(model.to_string()))
}

fn collect_rejections(
    request: &RouteRequest,
    candidate: &RichRouteCandidate,
    mapped_model: Option<&str>,
    reasons: &mut Vec<String>,
    rejection_reasons: &mut Vec<String>,
) {
    match request.endpoint {
        RouteEndpointKind::Models => {
            reasons.push("models endpoint does not require model capability".to_string())
        }
        RouteEndpointKind::Responses if !candidate.capabilities.supports_responses => {
            rejection_reasons.push("does not support responses".to_string());
        }
        RouteEndpointKind::ChatCompletions if !candidate.capabilities.supports_chat_completions => {
            rejection_reasons.push("does not support chat completions".to_string());
        }
        RouteEndpointKind::Embeddings if !candidate.capabilities.supports_embeddings => {
            rejection_reasons.push("does not support embeddings".to_string());
        }
        RouteEndpointKind::Responses => reasons.push("supports responses".to_string()),
        RouteEndpointKind::ChatCompletions => reasons.push("supports chat completions".to_string()),
        RouteEndpointKind::Embeddings => reasons.push("supports embeddings".to_string()),
    }

    if request.stream {
        if candidate.capabilities.supports_stream {
            reasons.push("supports stream".to_string());
        } else {
            rejection_reasons.push("does not support stream".to_string());
        }
    }
    if request.uses_tools && !candidate.capabilities.supports_tools {
        rejection_reasons.push("does not support tools".to_string());
    }
    if request.uses_vision && !candidate.capabilities.supports_vision {
        rejection_reasons.push("does not support vision".to_string());
    }
    if request.uses_reasoning && !candidate.capabilities.supports_reasoning {
        rejection_reasons.push("does not support reasoning".to_string());
    }

    if let Some(model) = request.model.as_deref() {
        let mapped = mapped_model.unwrap_or(model);
        if candidate
            .capabilities
            .model_blocklist
            .iter()
            .any(|item| item == model || item == mapped)
        {
            rejection_reasons.push(format!("model {model} is blocklisted"));
        }
        if !candidate.capabilities.model_allowlist.is_empty()
            && !candidate
                .capabilities
                .model_allowlist
                .iter()
                .any(|item| item == mapped)
        {
            rejection_reasons.push(format!("model {mapped} is not in allowlist"));
        } else {
            reasons.push(format!("model {mapped} allowed"));
        }
    }

    if let Some(health) = &candidate.health {
        if let Some(cooldown_until) = &health.cooldown_until {
            if cooldown_until
                .parse::<i64>()
                .map(|until| until > request.now_ms)
                .unwrap_or(false)
            {
                rejection_reasons.push(format!("cooldown active until {cooldown_until}"));
            }
        }
    }
}

fn candidate_score(
    request: &RouteRequest,
    candidate: &RichRouteCandidate,
    mapped_model: Option<&str>,
) -> i64 {
    let mut score = candidate.candidate.priority * 1000;

    if matches!(request.policy, RoutingPolicy::StableFirst) {
        let health = candidate.health.as_ref();
        score += health.map(|item| item.consecutive_failures).unwrap_or(0) * 500;
        score += health.and_then(|item| item.avg_latency_ms).unwrap_or(5_000) / 10;
        score -= health.map(|item| item.success_count.min(100)).unwrap_or(0) * 5;
    }

    if matches!(request.policy, RoutingPolicy::BackupOnly)
        && candidate.capabilities.only_use_as_backup
    {
        score += 100_000;
    }

    if let Some(economics) = candidate.economics.as_ref() {
        if matches!(request.policy, RoutingPolicy::CheapFirst) {
            score += cheap_first_score(economics);
        } else {
            score += balance_penalty(economics);
        }
    }

    if let Some(model) = request.model.as_deref() {
        let mapped = mapped_model.unwrap_or(model);
        if candidate
            .capabilities
            .preferred_models
            .iter()
            .any(|item| item == model || item == mapped)
        {
            score -= 250;
        }
    }

    score
}

fn cheap_first_score(economics: &RouteCandidateEconomics) -> i64 {
    let estimated_cost = estimated_cost(economics);
    let mut score = (estimated_cost * 1_000_000.0).round() as i64;
    score += balance_penalty(economics);
    score
}

fn balance_penalty(economics: &RouteCandidateEconomics) -> i64 {
    let mut penalty = 0_i64;
    match economics.balance_status.as_deref() {
        Some("depleted") => penalty += 200_000,
        Some("low") => penalty += 40_000,
        _ => {}
    }
    if let (Some(value), Some(threshold)) =
        (economics.balance_value, economics.low_balance_threshold)
    {
        if value <= threshold {
            penalty += 20_000;
        }
    }
    penalty
}

fn estimated_cost(economics: &RouteCandidateEconomics) -> f64 {
    if let Some(fixed_price) = economics.fixed_price {
        return fixed_price;
    }
    let input = economics.estimated_input_price.unwrap_or(0.0);
    let output = economics.estimated_output_price.unwrap_or(0.0);
    if input > 0.0 || output > 0.0 {
        input + output
    } else {
        1.0
    }
}

fn candidate_economic_reasons(
    candidate: &RichRouteCandidate,
    request: &RouteRequest,
) -> Vec<String> {
    let Some(economics) = candidate.economics.as_ref() else {
        return Vec::new();
    };

    let mut reasons = Vec::new();
    if let Some(rule_id) = economics.pricing_rule_id.as_deref() {
        reasons.push(format!("pricing rule {rule_id}"));
    }
    if let Some(currency) = economics.price_currency.as_deref() {
        reasons.push(format!("price currency {currency}"));
    }

    let estimated_cost = estimated_cost(economics);
    if matches!(request.policy, RoutingPolicy::CheapFirst) {
        reasons.push(format!("lower estimated cost {:.4}", estimated_cost));
    }

    match economics.balance_status.as_deref() {
        Some("depleted") => reasons.push("balance depleted".to_string()),
        Some("low") => reasons.push("balance low".to_string()),
        Some("normal") => reasons.push("balance normal".to_string()),
        Some(other) => reasons.push(format!("balance {other}")),
        None => {}
    }
    if let Some(value) = economics.balance_value {
        reasons.push(format!("balance value {:.2}", value));
    }
    if let Some(threshold) = economics.low_balance_threshold {
        reasons.push(format!("low balance threshold {:.2}", threshold));
    }

    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::proxy::UpstreamApiFormat;

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
            now_ms: 1_800_000_000_000,
        }
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
            },
            station_name: format!("Station {id}"),
            key_name: format!("Key {id}"),
            capabilities,
            health,
            economics: None,
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
            estimated_input_price,
            estimated_output_price,
            fixed_price: None,
            price_currency: Some("USD".to_string()),
            pricing_source: Some("manual".to_string()),
            balance_status: Some(balance_status.to_string()),
            balance_value,
            low_balance_threshold,
            balance_currency: Some("USD".to_string()),
        }
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
