use crate::{
    models::routing::{RouteCandidateExplanation, RouteEndpointKind, RoutingPolicy},
    services::proxy::router::{
        RichRouteCandidate, RouteCandidateEconomics, RouteRequest, RouteSelection,
    },
};

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
            group_binding_id: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.group_binding_id.clone()),
            rate_multiplier: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.rate_multiplier),
            normalization_status: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.normalization_status.clone()),
            price_confidence: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.price_confidence),
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
            balance_scope: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.balance_scope.clone()),
            balance_collected_at: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.balance_collected_at.clone()),
            economic_freshness: candidate
                .economics
                .as_ref()
                .and_then(|economics| economics.economic_freshness.clone()),
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

    if !request.allow_depleted_fallback
        && candidate
            .economics
            .as_ref()
            .and_then(|economics| economics.balance_status.as_deref())
            == Some("depleted")
    {
        rejection_reasons.push("balance depleted and depleted fallback is disabled".to_string());
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
    if economics.normalization_status.as_deref() != Some("complete") {
        return 50_000_000 + balance_penalty(economics);
    }
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
    if let Some(group_binding_id) = economics.group_binding_id.as_deref() {
        reasons.push(format!("group binding {group_binding_id}"));
    }
    if let Some(rate_multiplier) = economics.rate_multiplier {
        reasons.push(format!("rate multiplier {:.4}", rate_multiplier));
    }
    match economics.normalization_status.as_deref() {
        Some("complete") => reasons.push("complete normalized price".to_string()),
        Some("group_rate_only") => {
            reasons.push("only group rate is available; exact price unknown".to_string())
        }
        Some(other) => reasons.push(format!("pricing normalization {other}")),
        None => {}
    }
    if let Some(currency) = economics.price_currency.as_deref() {
        reasons.push(format!("price currency {currency}"));
    }

    let estimated_cost = estimated_cost(economics);
    if matches!(request.policy, RoutingPolicy::CheapFirst)
        && economics.normalization_status.as_deref() == Some("complete")
    {
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
    if let Some(scope) = economics.balance_scope.as_deref() {
        reasons.push(format!("balance scope {scope}"));
    }
    if let Some(freshness) = economics.economic_freshness.as_deref() {
        reasons.push(format!("economic freshness {freshness}"));
    }

    reasons
}
