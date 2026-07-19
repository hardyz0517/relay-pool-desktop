use crate::{
    application::error::ApplicationError,
    models::{
        pricing::BalanceSnapshot,
        routing::{ModelAlias, RoutingProxyDefaults, RuntimeRoutingCandidate, StationKeyHealth},
    },
    persistence::{runtime::PersistenceHandle, stores::routing_store::RoutingStore},
};

#[derive(Clone)]
pub(crate) struct RoutingService {
    runtime: PersistenceHandle,
    store: RoutingStore,
}

impl RoutingService {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            store: RoutingStore,
        }
    }

    pub(crate) async fn load_runtime_candidates(
        &self,
    ) -> Result<Vec<RuntimeRoutingCandidate>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .load_runtime_candidates(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn load_proxy_defaults(
        &self,
    ) -> Result<RoutingProxyDefaults, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .load_proxy_defaults(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_model_alias_pairs(
        &self,
    ) -> Result<Vec<(String, String)>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_model_alias_pairs(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_model_aliases(&self) -> Result<Vec<ModelAlias>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_model_aliases(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_balance_snapshots(
        &self,
    ) -> Result<Vec<BalanceSnapshot>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_balance_snapshots(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_balance_snapshots_for_station(
        &self,
        station_id: &str,
    ) -> Result<Vec<BalanceSnapshot>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_balance_snapshots_for_station(&mut read, station_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_station_key_health(
        &self,
    ) -> Result<Vec<StationKeyHealth>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_station_key_health(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn station_key_health_by_id(
        &self,
        station_key_id: &str,
    ) -> Result<StationKeyHealth, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .station_key_health_by_id(&mut read, station_key_id)
            .await
            .map_err(Into::into)
    }
}
