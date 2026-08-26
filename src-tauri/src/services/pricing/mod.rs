use crate::models::pricing::{PricingStatus, RequestKind, ResolvedPricingContext};

pub struct RequestPricingParts<'a> {
    pub station_key_id: &'a str,
    pub station_id: Option<&'a str>,
    pub model: Option<&'a str>,
    pub pricing_model: Option<&'a str>,
    pub group_binding_id: Option<&'a str>,
    pub rate_multiplier: Option<f64>,
    pub normalization_status: Option<&'a str>,
    pub price_confidence: Option<f64>,
    pub base_input_price: Option<f64>,
    pub base_output_price: Option<f64>,
    pub base_cache_creation_price: Option<f64>,
    pub base_cache_read_price: Option<f64>,
    pub estimated_input_price: Option<f64>,
    pub estimated_output_price: Option<f64>,
    pub estimated_cache_creation_price: Option<f64>,
    pub estimated_cache_read_price: Option<f64>,
    pub price_currency: Option<&'a str>,
    pub pricing_source: Option<&'a str>,
    pub collected_at: Option<&'a str>,
}

pub fn pricing_context_from_pricing_parts(
    parts: &RequestPricingParts<'_>,
) -> ResolvedPricingContext {
    let requested_model = parts
        .model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(parts.pricing_model)
        .unwrap_or("unknown");
    let pricing_status = pricing_status_from_parts(parts);
    ResolvedPricingContext {
        station_key_id: parts.station_key_id.to_string(),
        station_id: parts.station_id.unwrap_or("unknown").to_string(),
        requested_model: requested_model.to_string(),
        resolved_model: parts.pricing_model.unwrap_or(requested_model).to_string(),
        request_kind: RequestKind::Text,
        group_binding_id: parts.group_binding_id.map(ToString::to_string),
        base_input_price: parts.base_input_price,
        base_output_price: parts.base_output_price,
        base_cache_creation_price: parts.base_cache_creation_price,
        base_cache_read_price: parts.base_cache_read_price,
        currency: parts.price_currency.unwrap_or("unknown").to_string(),
        unit: "per_1m_tokens".to_string(),
        base_price_source: parts.pricing_source.map(ToString::to_string),
        effective_rate_multiplier: parts.rate_multiplier,
        rate_source: parts.pricing_source.map(ToString::to_string),
        rate_collected_at: parts.collected_at.map(ToString::to_string),
        estimated_input_price: parts.estimated_input_price,
        estimated_output_price: parts.estimated_output_price,
        estimated_cache_creation_price: parts.estimated_cache_creation_price,
        estimated_cache_read_price: parts.estimated_cache_read_price,
        pricing_status,
        confidence: parts.price_confidence.unwrap_or(0.0),
        source_chain: pricing_parts_source_chain(parts),
        reason: pricing_parts_reason(parts),
        resolved_at: parts.collected_at.unwrap_or("unknown").to_string(),
    }
}

fn pricing_status_from_parts(parts: &RequestPricingParts<'_>) -> PricingStatus {
    match parts.normalization_status {
        Some("base_price_only") => PricingStatus::BasePriceOnly,
        Some("base_price_with_group_rate") => PricingStatus::Priced,
        Some("complete") => PricingStatus::Priced,
        Some("group_rate_only")
            if parts.estimated_input_price.is_none() && parts.estimated_output_price.is_none() =>
        {
            PricingStatus::MissingModelPrice
        }
        _ if parts.group_binding_id.is_some()
            && parts.rate_multiplier.is_none()
            && parts.estimated_input_price.is_none()
            && parts.estimated_output_price.is_none() =>
        {
            PricingStatus::MissingRate
        }
        _ if parts.estimated_input_price.is_some() || parts.estimated_output_price.is_some() => {
            PricingStatus::Priced
        }
        _ => PricingStatus::Unpriced,
    }
}

fn pricing_parts_source_chain(parts: &RequestPricingParts<'_>) -> Vec<String> {
    let mut chain = Vec::new();
    if let Some(model) = parts.pricing_model {
        chain.push(format!("model:{model}"));
    }
    if let Some(group_binding_id) = parts.group_binding_id {
        chain.push(format!("group_binding:{group_binding_id}"));
    }
    if let Some(source) = parts.pricing_source {
        chain.push(format!("pricing_source:{source}"));
    }
    chain
}

fn pricing_parts_reason(parts: &RequestPricingParts<'_>) -> Option<String> {
    if parts.estimated_input_price.is_some() || parts.estimated_output_price.is_some() {
        return None;
    }
    Some(
        match parts.normalization_status {
            Some("group_rate_only") => "model_base_price_not_found",
            Some("missing_rate") => "missing_rate",
            _ => "pricing_not_available",
        }
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pricing_diagnostics_context_returns_source_chain() {
        let parts = RequestPricingParts {
            station_key_id: "key-1",
            station_id: Some("station-1"),
            model: Some("gpt-5.4-mini"),
            pricing_model: Some("gpt-5.4-mini"),
            group_binding_id: Some("binding-1"),
            rate_multiplier: Some(0.8),
            normalization_status: Some("base_price_with_group_rate"),
            price_confidence: Some(0.9),
            base_input_price: None,
            base_output_price: None,
            base_cache_creation_price: None,
            base_cache_read_price: None,
            estimated_input_price: Some(0.3),
            estimated_output_price: Some(1.8),
            estimated_cache_creation_price: None,
            estimated_cache_read_price: None,
            price_currency: Some("USD"),
            pricing_source: Some("model_base_price"),
            collected_at: Some("1000"),
        };

        let context = pricing_context_from_pricing_parts(&parts);

        assert_eq!(context.pricing_status, PricingStatus::Priced);
        assert_eq!(
            context.source_chain,
            vec![
                "model:gpt-5.4-mini".to_string(),
                "group_binding:binding-1".to_string(),
                "pricing_source:model_base_price".to_string(),
            ]
        );
    }

    #[test]
    fn pricing_diagnostics_context_marks_missing_expected_rate() {
        let parts = RequestPricingParts {
            station_key_id: "key-1",
            station_id: Some("station-1"),
            model: Some("gpt-5.4-mini"),
            pricing_model: Some("gpt-5.4-mini"),
            group_binding_id: Some("binding-1"),
            rate_multiplier: None,
            normalization_status: None,
            price_confidence: Some(0.5),
            base_input_price: None,
            base_output_price: None,
            base_cache_creation_price: None,
            base_cache_read_price: None,
            estimated_input_price: None,
            estimated_output_price: None,
            estimated_cache_creation_price: None,
            estimated_cache_read_price: None,
            price_currency: Some("USD"),
            pricing_source: None,
            collected_at: Some("1000"),
        };

        let context = pricing_context_from_pricing_parts(&parts);

        assert_eq!(context.pricing_status, PricingStatus::MissingRate);
        assert_eq!(context.reason.as_deref(), Some("pricing_not_available"));
    }

    #[test]
    fn pricing_diagnostics_preserves_explicit_missing_rate_reason() {
        let parts = RequestPricingParts {
            station_key_id: "key-1",
            station_id: Some("station-1"),
            model: Some("gpt-5.4-mini"),
            pricing_model: Some("gpt-5.4-mini"),
            group_binding_id: Some("binding-1"),
            rate_multiplier: None,
            normalization_status: Some("missing_rate"),
            price_confidence: Some(0.5),
            base_input_price: Some(0.25),
            base_output_price: Some(2.0),
            base_cache_creation_price: None,
            base_cache_read_price: None,
            estimated_input_price: None,
            estimated_output_price: None,
            estimated_cache_creation_price: None,
            estimated_cache_read_price: None,
            price_currency: Some("USD"),
            pricing_source: Some("model_base_price"),
            collected_at: Some("1000"),
        };

        let context = pricing_context_from_pricing_parts(&parts);

        assert_eq!(context.pricing_status, PricingStatus::MissingRate);
        assert_eq!(context.reason.as_deref(), Some("missing_rate"));
    }

    #[test]
    fn pricing_diagnostics_context_marks_base_price_only() {
        let parts = RequestPricingParts {
            station_key_id: "key-1",
            station_id: Some("station-1"),
            model: Some("gpt-5.4-mini"),
            pricing_model: Some("gpt-5.4-mini"),
            group_binding_id: None,
            rate_multiplier: Some(1.0),
            normalization_status: Some("base_price_only"),
            price_confidence: Some(0.8),
            base_input_price: Some(0.375),
            base_output_price: Some(2.25),
            base_cache_creation_price: Some(0.46875),
            base_cache_read_price: Some(0.0375),
            estimated_input_price: Some(0.375),
            estimated_output_price: Some(2.25),
            estimated_cache_creation_price: Some(0.46875),
            estimated_cache_read_price: Some(0.0375),
            price_currency: Some("USD"),
            pricing_source: Some("model_base_price"),
            collected_at: Some("1000"),
        };

        let context = pricing_context_from_pricing_parts(&parts);

        assert_eq!(context.pricing_status, PricingStatus::BasePriceOnly);
        assert_eq!(
            context.source_chain,
            vec![
                "model:gpt-5.4-mini".to_string(),
                "pricing_source:model_base_price".to_string(),
            ]
        );
    }
}
