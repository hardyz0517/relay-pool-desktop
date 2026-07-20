use std::sync::Arc;

use crate::{
    application::{clock::Clock, error::ApplicationError, ids::IdGenerator, pagination::PageLimit},
    models::pricing::{
        BalanceSnapshot, ModelBasePrice, PricingRule, UpsertBalanceSnapshotInput,
        UpsertModelBasePriceInput, UpsertPricingRuleInput,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::pricing_store::{
            NewBalanceSnapshotRow, NewModelBasePriceRow, NewPricingRuleRow, PricingStore,
        },
    },
};

#[derive(Clone)]
pub(crate) struct PricingService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    store: PricingStore,
}

impl PricingService {
    pub(crate) fn new(
        runtime: PersistenceHandle,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            runtime,
            clock,
            ids,
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

    pub(crate) async fn select_pricing_rule(
        &self,
        station_id: &str,
        station_key_id: Option<&str>,
        group_binding_id: Option<&str>,
        model: &str,
    ) -> Result<Option<PricingRule>, ApplicationError> {
        if station_id.trim().is_empty() || model.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.store
            .select_pricing_rule(
                &mut read,
                station_id,
                station_key_id,
                group_binding_id,
                model,
                &self.now_ms_string(),
            )
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn select_model_base_price(
        &self,
        model: &str,
    ) -> Result<Option<ModelBasePrice>, ApplicationError> {
        if model.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.store
            .select_model_base_price(&mut read, model)
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
