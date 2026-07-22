use std::sync::Arc;

use crate::{
    application::{clock::Clock, error::ApplicationError, ids::IdGenerator, pagination::PageLimit},
    models::{
        channel_monitors::{MonitorProbeUsageEvidence, MonitorRequestPricingEvidence},
        pricing::{
            BalanceSnapshot, ModelBasePrice, PricingRule, RequestKind, RequestUsage,
            ResolvedPricingContext, UpsertBalanceSnapshotInput, UpsertModelBasePriceInput,
            UpsertPricingRuleInput,
        },
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::pricing_store::{
            NewBalanceSnapshotRow, NewModelBasePriceRow, NewPricingRuleRow, PricingStore,
            SelectedModelBasePriceRow, StationKeyPricingResolutionRow,
        },
    },
    services::pricing::{
        pricing_context_from_pricing_parts, request_cost_from_pricing_parts_and_usage,
        RequestPricingParts,
    },
};

pub(crate) trait BuiltinModelBasePriceCatalog: Send + Sync {
    fn model_base_prices(&self) -> Vec<UpsertModelBasePriceInput>;
}

#[derive(Clone)]
pub(crate) struct PricingService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    catalog: Arc<dyn BuiltinModelBasePriceCatalog>,
    store: PricingStore,
}

impl PricingService {
    pub(crate) fn new(
        runtime: PersistenceHandle,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        catalog: Arc<dyn BuiltinModelBasePriceCatalog>,
    ) -> Self {
        Self {
            runtime,
            clock,
            ids,
            catalog,
            store: PricingStore,
        }
    }

    pub(crate) async fn list_model_base_prices(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ModelBasePrice>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_model_base_prices(&mut read, limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_pricing_rules(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<PricingRule>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_pricing_rules(&mut read, limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn latest_station_balances(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<BalanceSnapshot>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .latest_station_balances(&mut read, limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn upsert_pricing_rule(
        &self,
        input: UpsertPricingRuleInput,
    ) -> Result<PricingRule, ApplicationError> {
        let store = self.store;
        let row = NewPricingRuleRow {
            id: input.id.clone().unwrap_or_else(|| self.ids.next_id()),
            now: self.now_ms_string(),
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.upsert_pricing_rule(write, row).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn delete_pricing_rule(&self, id: String) -> Result<(), ApplicationError> {
        if id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let store = self.store;
        self.runtime
            .write(|write| Box::pin(async move { store.delete_pricing_rule(write, &id).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn upsert_model_base_price(
        &self,
        input: UpsertModelBasePriceInput,
    ) -> Result<ModelBasePrice, ApplicationError> {
        let store = self.store;
        let row = NewModelBasePriceRow {
            id: input.id.clone().unwrap_or_else(|| self.ids.next_id()),
            now: self.now_ms_string(),
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.upsert_model_base_price(write, row).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn reset_model_base_prices_to_builtins(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ModelBasePrice>, ApplicationError> {
        let rows = self.builtin_catalog_rows()?;
        let store = self.store;
        let limit = limit.get();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .reset_model_base_prices_to_builtins(write, &rows, limit)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn ensure_builtin_model_base_prices(&self) -> Result<bool, ApplicationError> {
        let rows = self.builtin_catalog_rows()?;
        let store = self.store;
        self.runtime
            .write(|write| {
                Box::pin(async move { store.ensure_builtin_model_base_prices(write, &rows).await })
            })
            .await
            .map_err(Into::into)
    }

    fn builtin_catalog_rows(&self) -> Result<Vec<NewModelBasePriceRow>, ApplicationError> {
        let entries = self.catalog.model_base_prices();
        if entries.is_empty()
            || entries.iter().any(|entry| {
                !entry.built_in || entry.id.as_deref().map(str::trim).is_none_or(str::is_empty)
            })
        {
            return Err(ApplicationError::ConstraintViolation);
        }

        let now = self.now_ms_string();
        entries
            .into_iter()
            .map(|input| {
                let id = input
                    .id
                    .clone()
                    .ok_or(ApplicationError::ConstraintViolation)?;
                Ok(NewModelBasePriceRow {
                    id,
                    now: now.clone(),
                    input,
                })
            })
            .collect()
    }

    pub(crate) async fn resolve_station_key_pricing_context(
        &self,
        station_key_id: &str,
        requested_model: &str,
        request_kind: Option<RequestKind>,
    ) -> Result<ResolvedPricingContext, ApplicationError> {
        let station_key_id = station_key_id.trim();
        let requested_model = requested_model.trim();
        if station_key_id.is_empty() || requested_model.is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }

        let now = self.now_ms_string();
        let mut read = self.runtime.begin_read().await?;
        let resolution = self
            .store
            .resolve_station_key_pricing(&mut read, station_key_id, requested_model, &now)
            .await?;
        let mut context =
            pricing_context_from_resolution(station_key_id, requested_model, resolution.as_ref());
        context.request_kind = request_kind.unwrap_or(RequestKind::Text);
        if context.resolved_at == "unknown" {
            context.resolved_at = now;
        }
        Ok(context)
    }

    pub(crate) async fn estimate_monitor_request_cost(
        &self,
        station_key_id: &str,
        requested_model: &str,
        usage: Option<&MonitorProbeUsageEvidence>,
    ) -> Result<MonitorRequestPricingEvidence, ApplicationError> {
        let station_key_id = station_key_id.trim();
        let requested_model = requested_model.trim();
        if station_key_id.is_empty() || requested_model.is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }

        let now = self.now_ms_string();
        let mut read = self.runtime.begin_read().await?;
        let resolution = self
            .store
            .resolve_station_key_pricing(&mut read, station_key_id, requested_model, &now)
            .await?;
        let owned = pricing_parts_from_resolution(resolution.as_ref());
        let group_binding_id = owned.group_binding_id.clone();
        let normalization_status = owned.normalization_status.clone();
        let usage = RequestUsage {
            input_tokens: usage.and_then(|usage| usage.prompt_tokens),
            output_tokens: usage.and_then(|usage| usage.completion_tokens),
            total_tokens: usage.and_then(|usage| usage.total_tokens),
            request_count: Some(1),
            cache_creation_tokens: usage.and_then(|usage| usage.cache_creation_tokens),
            cache_read_tokens: usage.and_then(|usage| usage.cache_read_tokens),
            media_count: None,
            duration_seconds: None,
            size_tier: None,
        };
        let estimate = request_cost_from_pricing_parts_and_usage(
            Some(owned.as_parts(station_key_id, None, requested_model)),
            &usage,
        );
        Ok(MonitorRequestPricingEvidence {
            estimate,
            group_binding_id,
            normalization_status,
        })
    }

    pub(crate) async fn upsert_balance_snapshot(
        &self,
        input: UpsertBalanceSnapshotInput,
    ) -> Result<BalanceSnapshot, ApplicationError> {
        let store = self.store;
        let row = NewBalanceSnapshotRow {
            id: input.id.clone().unwrap_or_else(|| self.ids.next_id()),
            now: self.now_ms_string(),
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.upsert_balance_snapshot(write, row).await }))
            .await
            .map_err(Into::into)
    }

    fn now_ms_string(&self) -> String {
        self.clock.now_utc().timestamp_millis().to_string()
    }
}

fn pricing_context_from_resolution(
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
                    if let Some(multiplier) = positive(rule.rate_multiplier) {
                        owned.rate_multiplier = Some(multiplier);
                        owned.normalization_status = Some("base_price_with_group_rate".to_string());
                        owned.estimated_input_price =
                            base_price.input_price.map(|price| price * multiplier);
                        owned.estimated_output_price =
                            base_price.output_price.map(|price| price * multiplier);
                    } else if rule.group_binding_id.is_some() {
                        owned.pricing_rule_id = Some(rule.id.clone());
                        owned.normalization_status = Some("missing_rate".to_string());
                    }
                    if owned.rate_multiplier.is_some() || owned.pricing_rule_id.is_some() {
                        owned.pricing_model = Some(base_price.model.clone());
                        owned.group_binding_id = rule.group_binding_id.clone();
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
            if let Some(multiplier) = positive(resolution.group_rate_multiplier) {
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
mod tests {
    use super::*;
    use crate::{
        models::pricing::PricingStatus, persistence::stores::pricing_store::SelectedPricingRuleRow,
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

    #[test]
    fn direct_builtin_price_uses_existing_context_semantics() {
        let resolution = resolution();
        let context = pricing_context_from_resolution("key-1", "gpt-5-mini", Some(&resolution));

        assert_eq!(context.station_id, "station-1");
        assert_eq!(context.pricing_status, PricingStatus::BasePriceOnly);
        assert_eq!(context.effective_rate_multiplier, Some(1.0));
        assert_eq!(context.estimated_input_price, Some(0.25));
        assert_eq!(context.estimated_output_price, Some(2.0));
        assert_eq!(context.confidence, 0.95);
    }

    #[test]
    fn station_group_multiplier_is_applied_to_builtin_price() {
        let mut resolution = resolution();
        resolution.group_binding_id = Some("binding-1".to_string());
        resolution.group_rate_multiplier = Some(1.5);
        resolution.group_confidence = Some(0.9);
        resolution.group_collected_at = Some("200".to_string());

        let context = pricing_context_from_resolution("key-1", "gpt-5-mini", Some(&resolution));

        assert_eq!(context.pricing_status, PricingStatus::Priced);
        assert_eq!(context.group_binding_id.as_deref(), Some("binding-1"));
        assert_eq!(context.effective_rate_multiplier, Some(1.5));
        assert_eq!(context.estimated_input_price, Some(0.375));
        assert_eq!(context.estimated_output_price, Some(3.0));
        assert_eq!(context.confidence, 0.9);
        assert_eq!(context.rate_collected_at.as_deref(), Some("200"));
    }

    #[test]
    fn explicit_pricing_rule_takes_precedence_over_builtin_price() {
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

        assert_eq!(context.pricing_status, PricingStatus::Priced);
        assert_eq!(context.base_input_price, Some(0.4));
        assert_eq!(context.estimated_output_price, Some(3.2));
        assert_eq!(context.currency, "CNY");
        assert_eq!(
            context.source_chain.first().map(String::as_str),
            Some("pricing_rule:rule-1")
        );
    }

    #[test]
    fn bound_group_without_multiplier_is_reported_as_missing_rate() {
        let mut resolution = resolution();
        resolution.group_binding_id = Some("binding-1".to_string());
        resolution.group_confidence = Some(0.9);

        let context = pricing_context_from_resolution("key-1", "gpt-5-mini", Some(&resolution));

        assert_eq!(context.pricing_status, PricingStatus::MissingRate);
        assert_eq!(context.reason.as_deref(), Some("missing_rate"));
        assert_eq!(context.base_input_price, Some(0.25));
        assert_eq!(context.estimated_input_price, None);
    }

    #[test]
    fn missing_station_key_is_an_unpriced_context() {
        let context = pricing_context_from_resolution("missing", "gpt-5-mini", None);

        assert_eq!(context.station_id, "unknown");
        assert_eq!(context.pricing_status, PricingStatus::Unpriced);
        assert_eq!(context.reason.as_deref(), Some("pricing_not_available"));
    }
}
