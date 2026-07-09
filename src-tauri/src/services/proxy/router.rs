use crate::{
    models::routing::{
        RouteCandidateExplanation, RouteEndpointKind, RoutingPolicy, StationKeyCapabilities,
        StationKeyHealth,
    },
    services::proxy::RouteCandidate,
};

pub use crate::services::proxy::routing_policy::select_route_candidates;

#[derive(Debug, Clone)]
pub struct RouteRequest {
    pub endpoint: RouteEndpointKind,
    pub model: Option<String>,
    pub stream: bool,
    pub uses_tools: bool,
    pub uses_vision: bool,
    pub uses_reasoning: bool,
    pub policy: RoutingPolicy,
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
            allow_depleted_fallback: false,
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
            group_binding_id: None,
            rate_multiplier: None,
            normalization_status: Some("complete".to_string()),
            price_confidence: Some(0.9),
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
