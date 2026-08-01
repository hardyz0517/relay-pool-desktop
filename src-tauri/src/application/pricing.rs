use std::sync::Arc;

use crate::{
    application::{
        clock::Clock, error::ApplicationError, ids::IdGenerator,
        operational_facts::pricing_projector::pricing_context_from_resolution,
        pagination::PageLimit,
    },
    models::pricing::{
        BalanceSnapshot, ModelBasePrice, PricingRule, RequestKind, ResolvedPricingContext,
        UpsertBalanceSnapshotInput, UpsertModelBasePriceInput, UpsertPricingRuleInput,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::pricing_store::{
            NewBalanceSnapshotRow, NewModelBasePriceRow, NewPricingRuleRow, PricingStore,
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::pricing::PricingStatus,
        persistence::stores::pricing_store::{
            SelectedModelBasePriceRow, SelectedPricingRuleRow, StationKeyPricingResolutionRow,
        },
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
