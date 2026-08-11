use crate::{
    models::pricing::{PricingStatus, ResolvedPricingContext},
    persistence::stores::pricing_store::{
        SelectedModelBasePriceRow, StationKeyPricingResolutionRow,
    },
    services::pricing::{pricing_context_from_pricing_parts, RequestPricingParts},
};

#[cfg(test)]
pub(crate) const PRICING_PROJECTOR_VERSION: &str = "pricing_match_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PricingRouteKind {
    Inference,
    ModelCatalog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoutingCostBasis {
    ExactPrice,
    MultiplierProxy,
    Unpriced,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
pub(crate) enum PricingVerdict {
    Exact,
    MultiplierProxy,
    Unpriced,
    NotApplicable,
    Ambiguous,
    Stale,
    Invalid,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg(test)]
pub(crate) struct PricingProjection {
    pub(crate) verdict: PricingVerdict,
    pub(crate) basis: RoutingCostBasis,
    pub(crate) comparison_value: Option<f64>,
    pub(crate) reason_code: &'static str,
    pub(crate) source_refs: Vec<String>,
    pub(crate) observed_at: Option<String>,
    pub(crate) confidence: Option<f64>,
    pub(crate) projector_version: &'static str,
}

#[cfg(test)]
pub(crate) fn reduce_pricing(
    route_kind: PricingRouteKind,
    pricing: Option<&ResolvedPricingContext>,
) -> PricingProjection {
    if route_kind == PricingRouteKind::ModelCatalog {
        return PricingProjection {
            verdict: PricingVerdict::NotApplicable,
            basis: RoutingCostBasis::NotApplicable,
            comparison_value: None,
            reason_code: "model_catalog_has_no_request_cost",
            source_refs: Vec::new(),
            observed_at: None,
            confidence: None,
            projector_version: PRICING_PROJECTOR_VERSION,
        };
    }
    let context = request_cost_comparison_context(route_kind, pricing);
    let (verdict, reason_code) = match context.basis {
        RoutingCostBasis::ExactPrice => (PricingVerdict::Exact, "pricing_exact"),
        RoutingCostBasis::MultiplierProxy => {
            (PricingVerdict::MultiplierProxy, "pricing_multiplier_proxy")
        }
        RoutingCostBasis::NotApplicable => {
            (PricingVerdict::NotApplicable, "pricing_not_applicable")
        }
        RoutingCostBasis::Unpriced => match context.reason {
            Some("pricing_context_missing") => (PricingVerdict::Invalid, "pricing_missing"),
            Some("missing_rate") => (PricingVerdict::Ambiguous, "pricing_missing_rate"),
            Some("pricing_not_available") => (PricingVerdict::Stale, "pricing_stale"),
            _ => (PricingVerdict::Unpriced, "pricing_unpriced"),
        },
    };
    PricingProjection {
        verdict,
        basis: context.basis,
        comparison_value: context.comparison_value,
        reason_code,
        source_refs: context.source_chain,
        observed_at: context.observed_at,
        confidence: context.confidence,
        projector_version: PRICING_PROJECTOR_VERSION,
    }
}

impl RoutingCostBasis {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ExactPrice => "exact_price",
            Self::MultiplierProxy => "multiplier_proxy",
            Self::Unpriced => "unpriced",
            Self::NotApplicable => "not_applicable",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RequestCostComparisonContext {
    pub(crate) route_kind: PricingRouteKind,
    pub(crate) basis: RoutingCostBasis,
    pub(crate) comparison_value: Option<f64>,
    pub(crate) reason: Option<&'static str>,
    pub(crate) currency: Option<String>,
    pub(crate) unit: Option<String>,
    pub(crate) estimated_input_price: Option<f64>,
    pub(crate) estimated_output_price: Option<f64>,
    pub(crate) estimated_fixed_price: Option<f64>,
    pub(crate) status_label: String,
    pub(crate) source_chain: Vec<String>,
    pub(crate) observed_at: Option<String>,
    pub(crate) confidence: Option<f64>,
}

pub(crate) fn request_cost_comparison_context(
    route_kind: PricingRouteKind,
    pricing: Option<&ResolvedPricingContext>,
) -> RequestCostComparisonContext {
    if route_kind == PricingRouteKind::ModelCatalog {
        return RequestCostComparisonContext {
            route_kind,
            basis: RoutingCostBasis::NotApplicable,
            comparison_value: None,
            reason: Some("model_catalog_has_no_request_cost"),
            currency: None,
            unit: None,
            estimated_input_price: None,
            estimated_output_price: None,
            estimated_fixed_price: None,
            status_label: "not_applicable".to_string(),
            source_chain: Vec::new(),
            observed_at: None,
            confidence: None,
        };
    }
    let Some(pricing) = pricing else {
        return RequestCostComparisonContext {
            route_kind,
            basis: RoutingCostBasis::Unpriced,
            comparison_value: None,
            reason: Some("pricing_context_missing"),
            currency: None,
            unit: None,
            estimated_input_price: None,
            estimated_output_price: None,
            estimated_fixed_price: None,
            status_label: "unpriced".to_string(),
            source_chain: Vec::new(),
            observed_at: None,
            confidence: None,
        };
    };
    let basis = if pricing.estimated_fixed_price.is_some()
        || pricing.estimated_input_price.is_some()
        || pricing.estimated_output_price.is_some()
    {
        RoutingCostBasis::ExactPrice
    } else if pricing.effective_rate_multiplier.is_some() {
        RoutingCostBasis::MultiplierProxy
    } else {
        RoutingCostBasis::Unpriced
    };
    let reason = match (basis, &pricing.pricing_status) {
        (RoutingCostBasis::ExactPrice, _) => None,
        (RoutingCostBasis::MultiplierProxy, _) => Some("cost_first_multiplier_proxy"),
        (_, PricingStatus::MissingRate) => Some("missing_rate"),
        (_, PricingStatus::UnsupportedBillingMode) => Some("unsupported_billing_mode"),
        (_, PricingStatus::Unpriced) => Some("pricing_not_available"),
        _ => Some("pricing_incomplete"),
    };
    let comparison_value = match basis {
        RoutingCostBasis::ExactPrice => exact_comparison_value(pricing),
        RoutingCostBasis::MultiplierProxy => pricing.effective_rate_multiplier,
        RoutingCostBasis::Unpriced | RoutingCostBasis::NotApplicable => None,
    };
    RequestCostComparisonContext {
        route_kind,
        basis,
        comparison_value,
        reason,
        currency: known_field(&pricing.currency),
        unit: known_field(&pricing.unit),
        estimated_input_price: pricing.estimated_input_price,
        estimated_output_price: pricing.estimated_output_price,
        estimated_fixed_price: pricing.estimated_fixed_price,
        status_label: pricing.pricing_status.as_str().to_string(),
        source_chain: pricing.source_chain.clone(),
        observed_at: pricing
            .rate_collected_at
            .clone()
            .or_else(|| known_field(&pricing.resolved_at)),
        confidence: Some(pricing.confidence),
    }
}

fn exact_comparison_value(pricing: &ResolvedPricingContext) -> Option<f64> {
    if let Some(fixed_price) = pricing.estimated_fixed_price {
        return Some(fixed_price);
    }
    match (
        pricing.estimated_input_price,
        pricing.estimated_output_price,
    ) {
        (Some(input_price), None) => Some(input_price),
        (None, Some(output_price)) => Some(output_price),
        _ => None,
    }
}

fn known_field(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("unknown") {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn pricing_context_from_resolution(
    station_key_id: &str,
    requested_model: &str,
    resolution: Option<&StationKeyPricingResolutionRow>,
) -> ResolvedPricingContext {
    let station_id = resolution.map(|row| row.station_id.as_str());
    let owned = pricing_parts_from_resolution(resolution);

    pricing_context_from_pricing_parts(&owned.as_parts(station_key_id, station_id, requested_model))
}

fn pricing_parts_from_resolution(
    resolution: Option<&StationKeyPricingResolutionRow>,
) -> OwnedPricingParts {
    let mut owned = OwnedPricingParts::default();

    if let Some(resolution) = resolution {
        let base_price = resolution.model_base_price.as_ref();
        if let Some(rule) = resolution.pricing_rule.as_ref() {
            let has_rule_price = rule.input_price.is_some()
                || rule.output_price.is_some()
                || rule.fixed_price.is_some();
            if !has_rule_price {
                if let Some(base_price) = base_price {
                    let group_binding_id = rule
                        .group_binding_id
                        .clone()
                        .or_else(|| resolution.group_binding_id.clone());
                    let raw_multiplier = rule.rate_multiplier.or(resolution.group_rate_multiplier);
                    if let Some(multiplier) =
                        effective_rate_multiplier(raw_multiplier, resolution.credit_per_cny)
                    {
                        owned.rate_multiplier = Some(multiplier);
                        owned.normalization_status = Some("base_price_with_group_rate".to_string());
                        owned.estimated_input_price =
                            base_price.input_price.map(|price| price * multiplier);
                        owned.estimated_output_price =
                            base_price.output_price.map(|price| price * multiplier);
                    } else if group_binding_id.is_some() {
                        owned.pricing_rule_id = Some(rule.id.clone());
                        owned.normalization_status = Some("missing_rate".to_string());
                    }
                    if owned.rate_multiplier.is_some() || owned.pricing_rule_id.is_some() {
                        owned.pricing_model = Some(base_price.model.clone());
                        owned.group_binding_id = group_binding_id;
                        owned.price_confidence =
                            Some(rule.confidence.min(base_price_confidence(base_price)));
                        owned.base_input_price = base_price.input_price;
                        owned.base_output_price = base_price.output_price;
                        owned.price_currency = Some(base_price.currency.clone());
                        owned.pricing_source = Some("model_base_price".to_string());
                        owned.collected_at = rule
                            .collected_at
                            .clone()
                            .or_else(|| base_price.source_checked_at.clone());
                    }
                }
            }

            if owned.pricing_model.is_none() {
                owned.pricing_rule_id = Some(rule.id.clone());
                owned.pricing_model = Some(rule.model.clone());
                owned.group_binding_id = rule.group_binding_id.clone();
                owned.rate_multiplier = rule.rate_multiplier;
                owned.normalization_status = Some(rule.normalization_status.clone());
                owned.price_confidence = Some(rule.confidence);
                owned.base_input_price = rule.input_price;
                owned.base_output_price = rule.output_price;
                owned.base_fixed_price = rule.fixed_price;
                owned.estimated_input_price = rule.input_price;
                owned.estimated_output_price = rule.output_price;
                owned.fixed_price = rule.fixed_price;
                owned.price_currency = Some(rule.currency.clone());
                owned.pricing_source = Some(rule.source.clone());
                owned.collected_at = rule.collected_at.clone();
            }
        } else if let Some(base_price) = base_price {
            if let Some(multiplier) = effective_rate_multiplier(
                resolution.group_rate_multiplier,
                resolution.credit_per_cny,
            ) {
                owned.group_binding_id = resolution.group_binding_id.clone();
                owned.rate_multiplier = Some(multiplier);
                owned.normalization_status = Some("base_price_with_group_rate".to_string());
                owned.price_confidence = Some(
                    resolution
                        .group_confidence
                        .unwrap_or(0.8)
                        .min(base_price_confidence(base_price)),
                );
                owned.estimated_input_price =
                    base_price.input_price.map(|price| price * multiplier);
                owned.estimated_output_price =
                    base_price.output_price.map(|price| price * multiplier);
                owned.collected_at = resolution
                    .group_collected_at
                    .clone()
                    .or_else(|| base_price.source_checked_at.clone());
            } else if resolution.group_binding_id.is_some() {
                owned.group_binding_id = resolution.group_binding_id.clone();
                owned.normalization_status = Some("missing_rate".to_string());
                owned.price_confidence = Some(
                    resolution
                        .group_confidence
                        .unwrap_or(0.8)
                        .min(base_price_confidence(base_price)),
                );
                owned.collected_at = resolution
                    .group_collected_at
                    .clone()
                    .or_else(|| base_price.source_checked_at.clone());
            } else {
                owned.rate_multiplier = Some(1.0);
                owned.normalization_status = Some("base_price_only".to_string());
                owned.price_confidence = Some(base_price_confidence(base_price));
                owned.estimated_input_price = base_price.input_price;
                owned.estimated_output_price = base_price.output_price;
                owned.collected_at = base_price.source_checked_at.clone();
            }
            owned.pricing_model = Some(base_price.model.clone());
            owned.base_input_price = base_price.input_price;
            owned.base_output_price = base_price.output_price;
            owned.price_currency = Some(base_price.currency.clone());
            owned.pricing_source = Some("model_base_price".to_string());
        }
    }

    owned
}

fn positive(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value > 0.0)
}

/// Converts a station/group multiplier expressed in station credits into the
/// normalized multiplier used by pricing, routing policy, and UI read models.
/// The station exchange rate is credits per CNY, so normalization divides by it.
pub(crate) fn effective_rate_multiplier(
    raw_multiplier: Option<f64>,
    credit_per_cny: f64,
) -> Option<f64> {
    let raw_multiplier = positive(raw_multiplier)?;
    let credit_per_cny = positive(Some(credit_per_cny)).unwrap_or(1.0);
    Some(raw_multiplier / credit_per_cny)
}

fn base_price_confidence(price: &SelectedModelBasePriceRow) -> f64 {
    if price.built_in {
        0.95
    } else {
        0.85
    }
}

#[derive(Default)]
struct OwnedPricingParts {
    pricing_rule_id: Option<String>,
    pricing_model: Option<String>,
    group_binding_id: Option<String>,
    rate_multiplier: Option<f64>,
    normalization_status: Option<String>,
    price_confidence: Option<f64>,
    base_input_price: Option<f64>,
    base_output_price: Option<f64>,
    base_fixed_price: Option<f64>,
    estimated_input_price: Option<f64>,
    estimated_output_price: Option<f64>,
    fixed_price: Option<f64>,
    price_currency: Option<String>,
    pricing_source: Option<String>,
    collected_at: Option<String>,
}

impl OwnedPricingParts {
    fn as_parts<'a>(
        &'a self,
        station_key_id: &'a str,
        station_id: Option<&'a str>,
        requested_model: &'a str,
    ) -> RequestPricingParts<'a> {
        RequestPricingParts {
            station_key_id,
            station_id,
            model: Some(requested_model),
            pricing_rule_id: self.pricing_rule_id.as_deref(),
            pricing_model: self.pricing_model.as_deref(),
            group_binding_id: self.group_binding_id.as_deref(),
            rate_multiplier: self.rate_multiplier,
            normalization_status: self.normalization_status.as_deref(),
            price_confidence: self.price_confidence,
            base_input_price: self.base_input_price,
            base_output_price: self.base_output_price,
            base_fixed_price: self.base_fixed_price,
            estimated_input_price: self.estimated_input_price,
            estimated_output_price: self.estimated_output_price,
            fixed_price: self.fixed_price,
            price_currency: self.price_currency.as_deref(),
            pricing_source: self.pricing_source.as_deref(),
            collected_at: self.collected_at.as_deref(),
        }
    }
}

#[cfg(test)]
mod effective_rate_multiplier_tests {
    use super::effective_rate_multiplier;

    #[test]
    fn divides_station_native_multiplier_by_exchange_rate() {
        assert_eq!(effective_rate_multiplier(Some(2.0), 27.0), Some(2.0 / 27.0));
    }

    #[test]
    fn falls_back_to_one_for_invalid_exchange_rate() {
        assert_eq!(effective_rate_multiplier(Some(0.5), 0.0), Some(0.5));
        assert_eq!(effective_rate_multiplier(Some(0.5), f64::NAN), Some(0.5));
    }

    #[test]
    fn rejects_missing_or_non_positive_raw_multiplier() {
        assert_eq!(effective_rate_multiplier(None, 1.0), None);
        assert_eq!(effective_rate_multiplier(Some(0.0), 1.0), None);
        assert_eq!(effective_rate_multiplier(Some(-1.0), 1.0), None);
    }
}
