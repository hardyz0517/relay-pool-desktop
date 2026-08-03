#[path = "../src/models/pricing.rs"]
mod pricing_model;

mod models {
    pub(crate) mod pricing {
        pub(crate) use crate::pricing_model::*;
    }
}

#[path = "../src/services/pricing/mod.rs"]
mod pricing_service;

mod services {
    pub(crate) mod pricing {
        pub(crate) use crate::pricing_service::*;
    }
}

mod persistence {
    pub(crate) mod stores {
        pub(crate) mod pricing_store {
            #[derive(Debug, Clone)]
            pub(crate) struct SelectedPricingRuleRow {
                pub(crate) id: String,
                pub(crate) model: String,
                pub(crate) input_price: Option<f64>,
                pub(crate) output_price: Option<f64>,
                pub(crate) fixed_price: Option<f64>,
                pub(crate) currency: String,
                pub(crate) source: String,
                pub(crate) group_binding_id: Option<String>,
                pub(crate) rate_multiplier: Option<f64>,
                pub(crate) normalization_status: String,
                pub(crate) confidence: f64,
                pub(crate) collected_at: Option<String>,
            }

            #[derive(Debug, Clone)]
            pub(crate) struct SelectedModelBasePriceRow {
                pub(crate) model: String,
                pub(crate) input_price: Option<f64>,
                pub(crate) output_price: Option<f64>,
                pub(crate) currency: String,
                pub(crate) source_checked_at: Option<String>,
                pub(crate) built_in: bool,
            }

            #[derive(Debug, Clone)]
            pub(crate) struct StationKeyPricingResolutionRow {
                pub(crate) station_id: String,
                pub(crate) group_binding_id: Option<String>,
                pub(crate) group_rate_multiplier: Option<f64>,
                pub(crate) group_confidence: Option<f64>,
                pub(crate) group_collected_at: Option<String>,
                pub(crate) pricing_rule: Option<SelectedPricingRuleRow>,
                pub(crate) model_base_price: Option<SelectedModelBasePriceRow>,
            }
        }
    }
}

#[path = "../src/application/operational_facts/pricing_projector.rs"]
mod pricing_projector;

use models::pricing::{
    PricingStatus, UpsertBalanceSnapshotInput, UpsertModelBasePriceInput, UpsertPricingRuleInput,
};
use persistence::stores::pricing_store::{
    SelectedModelBasePriceRow, SelectedPricingRuleRow, StationKeyPricingResolutionRow,
};
use pricing_projector::{
    pricing_context_from_resolution, request_cost_comparison_context, PricingRouteKind,
    RoutingCostBasis,
};

fn base_price() -> SelectedModelBasePriceRow {
    SelectedModelBasePriceRow {
        model: "gpt-5-mini".to_string(),
        input_price: Some(0.25),
        output_price: Some(2.0),
        currency: "USD".to_string(),
        source_checked_at: Some("100".to_string()),
        built_in: true,
    }
}

fn resolution() -> StationKeyPricingResolutionRow {
    StationKeyPricingResolutionRow {
        station_id: "station-1".to_string(),
        group_binding_id: None,
        group_rate_multiplier: None,
        group_confidence: None,
        group_collected_at: None,
        pricing_rule: None,
        model_base_price: Some(base_price()),
    }
}

fn group_rate_only_rule() -> SelectedPricingRuleRow {
    SelectedPricingRuleRow {
        id: "rule-rate-only".to_string(),
        model: "gpt-5-mini".to_string(),
        input_price: None,
        output_price: None,
        fixed_price: None,
        currency: "USD".to_string(),
        source: "collector".to_string(),
        group_binding_id: Some("binding-1".to_string()),
        rate_multiplier: Some(0.42),
        normalization_status: "group_rate_only".to_string(),
        confidence: 0.8,
        collected_at: Some("300".to_string()),
    }
}

#[test]
fn pricing_mutation_inputs_keep_stable_camel_case_contract() {
    let pricing_rule = UpsertPricingRuleInput {
        id: Some("rule-1".to_string()),
        station_id: "station-1".to_string(),
        station_key_id: Some("key-1".to_string()),
        group_binding_id: Some("binding-1".to_string()),
        group_name: Some("Group".to_string()),
        tier_label: None,
        model: "gpt-5-mini".to_string(),
        input_price: Some(0.25),
        output_price: Some(2.0),
        fixed_price: None,
        rate_multiplier: Some(1.25),
        currency: "USD".to_string(),
        unit: "per_1m_tokens".to_string(),
        price_type: "token".to_string(),
        base_price_source: Some("builtin".to_string()),
        normalization_status: Some("exact".to_string()),
        source: "manual".to_string(),
        confidence: 0.9,
        enabled: true,
        note: None,
        collected_at: Some("100".to_string()),
        valid_from: None,
        valid_until: None,
    };
    let base_price = UpsertModelBasePriceInput {
        id: None,
        provider: "openai".to_string(),
        model: "gpt-5-mini".to_string(),
        input_price: Some(0.25),
        output_price: Some(2.0),
        currency: "USD".to_string(),
        unit: "per_1m_tokens".to_string(),
        source_url: "https://example.invalid/pricing".to_string(),
        source_label: "fixture".to_string(),
        source_checked_at: Some("100".to_string()),
        enabled: true,
        built_in: false,
        note: None,
    };
    let balance = UpsertBalanceSnapshotInput {
        id: None,
        station_id: "station-1".to_string(),
        station_key_id: Some("key-1".to_string()),
        scope: "station_key".to_string(),
        value: Some(10.0),
        currency: "USD".to_string(),
        credit_unit: Some("credit".to_string()),
        used_value: Some(1.0),
        total_value: Some(11.0),
        today_request_count: Some(1),
        total_request_count: Some(10),
        today_consumption: Some(0.1),
        total_consumption: Some(1.0),
        today_base_consumption: Some(0.1),
        total_base_consumption: Some(1.0),
        today_token_count: Some(100),
        total_token_count: Some(1_000),
        today_input_token_count: Some(40),
        today_output_token_count: Some(60),
        total_input_token_count: Some(400),
        total_output_token_count: Some(600),
        account_concurrency_limit: Some(8),
        low_balance_threshold: Some(2.0),
        status: "healthy".to_string(),
        source: "collector".to_string(),
        confidence: 0.9,
        collected_at: Some("100".to_string()),
    };

    let json = serde_json::json!({
        "pricingRule": pricing_rule,
        "basePrice": base_price,
        "balance": balance,
    });

    assert_eq!(json["pricingRule"]["stationKeyId"], "key-1");
    assert_eq!(json["pricingRule"]["basePriceSource"], "builtin");
    assert_eq!(json["basePrice"]["sourceCheckedAt"], "100");
    assert_eq!(json["balance"]["accountConcurrencyLimit"], 8);
    assert_eq!(json["balance"]["todayInputTokenCount"], 40);
}

#[test]
fn routing_cost_basis_labels_are_stable() {
    assert_eq!(RoutingCostBasis::ExactPrice.as_str(), "exact_price");
    assert_eq!(
        RoutingCostBasis::MultiplierProxy.as_str(),
        "multiplier_proxy"
    );
    assert_eq!(RoutingCostBasis::Unpriced.as_str(), "unpriced");
    assert_eq!(RoutingCostBasis::NotApplicable.as_str(), "not_applicable");
}

#[test]
fn base_price_and_group_multiplier_project_to_the_same_resolved_context_shape() {
    let mut resolution = resolution();
    resolution.group_binding_id = Some("binding-1".to_string());
    resolution.group_rate_multiplier = Some(1.5);
    resolution.group_confidence = Some(0.9);
    resolution.group_collected_at = Some("200".to_string());

    let context = pricing_context_from_resolution("key-1", "gpt-5-mini", Some(&resolution));

    assert_eq!(context.station_id, "station-1");
    assert_eq!(context.pricing_status, PricingStatus::Priced);
    assert_eq!(context.group_binding_id.as_deref(), Some("binding-1"));
    assert_eq!(context.effective_rate_multiplier, Some(1.5));
    assert_eq!(context.estimated_input_price, Some(0.375));
    assert_eq!(context.estimated_output_price, Some(3.0));
    assert_eq!(context.confidence, 0.9);
    assert_eq!(context.rate_collected_at.as_deref(), Some("200"));
}

#[test]
fn explicit_pricing_rule_takes_precedence_without_creating_a_second_cost_formula() {
    let mut resolution = resolution();
    resolution.pricing_rule = Some(SelectedPricingRuleRow {
        id: "rule-1".to_string(),
        model: "gpt-5-mini".to_string(),
        input_price: Some(0.4),
        output_price: Some(3.2),
        fixed_price: None,
        currency: "CNY".to_string(),
        source: "collector".to_string(),
        group_binding_id: None,
        rate_multiplier: None,
        normalization_status: "complete".to_string(),
        confidence: 0.8,
        collected_at: Some("300".to_string()),
    });

    let context = pricing_context_from_resolution("key-1", "gpt-5-mini", Some(&resolution));
    let basis = request_cost_comparison_context(PricingRouteKind::Inference, Some(&context));

    assert_eq!(context.pricing_status, PricingStatus::Priced);
    assert_eq!(context.base_input_price, Some(0.4));
    assert_eq!(context.estimated_output_price, Some(3.2));
    assert_eq!(context.currency, "CNY");
    assert_eq!(basis.basis, RoutingCostBasis::ExactPrice);
    assert_eq!(basis.reason, None);
    assert_eq!(basis.currency.as_deref(), Some("CNY"));
    assert_eq!(basis.unit.as_deref(), Some("per_1m_tokens"));
    assert_eq!(basis.observed_at.as_deref(), Some("300"));
    assert_eq!(basis.confidence, Some(0.8));
    assert_eq!(
        basis.source_chain,
        vec![
            "pricing_rule:rule-1".to_string(),
            "model:gpt-5-mini".to_string(),
            "pricing_source:collector".to_string(),
        ]
    );
}

#[test]
fn cost_first_uses_multiplier_proxy_when_exact_prices_are_missing() {
    let mut resolution = resolution();
    resolution.model_base_price = None;
    resolution.pricing_rule = Some(group_rate_only_rule());

    let context = pricing_context_from_resolution("key-1", "gpt-5-mini", Some(&resolution));
    let basis = request_cost_comparison_context(PricingRouteKind::Inference, Some(&context));

    assert_eq!(context.pricing_status, PricingStatus::MissingModelPrice);
    assert_eq!(context.effective_rate_multiplier, Some(0.42));
    assert_eq!(context.estimated_input_price, None);
    assert_eq!(context.estimated_output_price, None);
    assert_eq!(basis.basis, RoutingCostBasis::MultiplierProxy);
    assert_eq!(basis.reason, Some("cost_first_multiplier_proxy"));
    assert_eq!(basis.currency.as_deref(), Some("USD"));
    assert_eq!(basis.observed_at.as_deref(), Some("300"));
    assert_eq!(basis.confidence, Some(0.8));
}

#[test]
fn model_catalog_pricing_is_not_applicable_not_an_empty_context() {
    let basis = request_cost_comparison_context(PricingRouteKind::ModelCatalog, None);

    assert_eq!(basis.basis, RoutingCostBasis::NotApplicable);
    assert_eq!(basis.reason, Some("model_catalog_has_no_request_cost"));
    assert!(basis.currency.is_none());
    assert!(basis.source_chain.is_empty());
}

#[test]
fn missing_rate_and_missing_context_are_data_states_not_zero_cost() {
    let mut resolution = resolution();
    resolution.group_binding_id = Some("binding-1".to_string());
    resolution.group_confidence = Some(0.9);
    let context = pricing_context_from_resolution("key-1", "gpt-5-mini", Some(&resolution));
    let basis = request_cost_comparison_context(PricingRouteKind::Inference, Some(&context));

    assert_eq!(context.pricing_status, PricingStatus::MissingRate);
    assert_eq!(context.estimated_input_price, None);
    assert_eq!(basis.basis, RoutingCostBasis::Unpriced);
    assert_eq!(basis.reason, Some("missing_rate"));

    let basis = request_cost_comparison_context(PricingRouteKind::Inference, None);
    assert_eq!(basis.basis, RoutingCostBasis::Unpriced);
    assert_eq!(basis.reason, Some("pricing_context_missing"));
}
