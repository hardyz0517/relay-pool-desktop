use std::sync::Arc;

use crate::{
    application::{
        credentials::CredentialService, error::ApplicationError, queries::key_pool::KeyPoolQuery,
    },
    models::{
        group_facts::UpdateStationKeyGroupBindingInput,
        remote_keys::{RemoteKeyCapability, RemoteStationKey},
        routing::{StationKeyCapabilities, UpdateStationKeyCapabilitiesInput},
        shared_capabilities::{SaveStationKeyWithDefaultsInput, SaveStationKeyWithDefaultsResult},
        station_keys::{CreateStationKeyInput, KeyPoolItem, StationKey, UpdateStationKeyInput},
    },
};

#[derive(Clone)]
pub(crate) struct KeyPoolCommandFacade {
    credentials: Arc<CredentialService>,
    key_pool: Arc<KeyPoolQuery>,
}

impl KeyPoolCommandFacade {
    pub(crate) fn new(credentials: Arc<CredentialService>, key_pool: Arc<KeyPoolQuery>) -> Self {
        Self {
            credentials,
            key_pool,
        }
    }

    pub(crate) async fn list_station_keys(
        &self,
        station_id: String,
    ) -> Result<Vec<StationKey>, ApplicationError> {
        self.credentials.list_station_keys(station_id).await
    }

    pub(crate) async fn create_station_key(
        &self,
        input: CreateStationKeyInput,
    ) -> Result<StationKey, ApplicationError> {
        self.credentials.create_station_key(input).await
    }

    pub(crate) async fn update_station_key(
        &self,
        input: UpdateStationKeyInput,
    ) -> Result<StationKey, ApplicationError> {
        self.credentials.update_station_key(input).await
    }

    pub(crate) async fn save_station_key_with_defaults(
        &self,
        input: SaveStationKeyWithDefaultsInput,
    ) -> Result<SaveStationKeyWithDefaultsResult, ApplicationError> {
        self.credentials.save_station_key_with_defaults(input).await
    }

    pub(crate) async fn update_station_key_group_binding(
        &self,
        input: UpdateStationKeyGroupBindingInput,
    ) -> Result<StationKey, ApplicationError> {
        self.credentials
            .update_station_key_group_binding(input)
            .await
    }

    pub(crate) async fn delete_station_key(
        &self,
        station_key_id: String,
    ) -> Result<(), ApplicationError> {
        self.credentials.delete_station_key(station_key_id).await
    }

    pub(crate) async fn reorder_station_keys(
        &self,
        station_id: String,
        station_key_ids: Vec<String>,
    ) -> Result<Vec<StationKey>, ApplicationError> {
        self.credentials
            .reorder_station_keys(station_id, station_key_ids)
            .await
    }

    pub(crate) async fn get_remote_key_capability(
        &self,
        station_id: String,
    ) -> Result<RemoteKeyCapability, ApplicationError> {
        self.credentials.get_remote_key_capability(station_id).await
    }

    pub(crate) async fn list_remote_station_keys(
        &self,
        station_id: String,
    ) -> Result<Vec<RemoteStationKey>, ApplicationError> {
        self.credentials.list_remote_station_keys(station_id).await
    }

    pub(crate) async fn bind_remote_station_key(
        &self,
        remote_key_id: String,
        station_key_id: String,
    ) -> Result<Vec<RemoteStationKey>, ApplicationError> {
        self.credentials
            .bind_remote_station_key(remote_key_id, station_key_id)
            .await
    }

    pub(crate) async fn unbind_remote_station_key(
        &self,
        remote_key_id: String,
        station_id: String,
    ) -> Result<Vec<RemoteStationKey>, ApplicationError> {
        self.credentials
            .unbind_remote_station_key(remote_key_id, station_id)
            .await
    }

    pub(crate) async fn list_key_pool_items(&self) -> Result<Vec<KeyPoolItem>, ApplicationError> {
        self.key_pool.load_all().await
    }

    pub(crate) async fn reorder_key_pool(
        &self,
        station_key_ids: Vec<String>,
    ) -> Result<Vec<KeyPoolItem>, ApplicationError> {
        self.credentials.reorder_key_pool(station_key_ids).await?;
        self.key_pool.load_all().await
    }

    pub(crate) async fn get_station_key_capabilities(
        &self,
        station_key_id: String,
    ) -> Result<StationKeyCapabilities, ApplicationError> {
        self.credentials
            .get_station_key_capabilities(station_key_id)
            .await
    }

    pub(crate) async fn update_station_key_capabilities(
        &self,
        input: UpdateStationKeyCapabilitiesInput,
    ) -> Result<StationKeyCapabilities, ApplicationError> {
        self.credentials
            .update_station_key_capabilities(input)
            .await
    }
}
