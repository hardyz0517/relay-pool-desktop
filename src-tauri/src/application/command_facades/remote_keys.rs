use std::sync::Arc;

use crate::{
    application::{
        collectors::CollectorService, credentials::CredentialService, settings::SettingsService,
    },
    models::remote_keys::{
        CreateLocalStationKeyFromRemoteResult, CreateRemoteStationKeyInput,
        CreateRemoteStationKeyResult, RemoteKeyScanResult,
    },
    services::{
        collectors::V2CollectorSourceAdapter,
        remote_keys::{self, PreparedRemoteKeySave, RemoteKeyOperationError},
    },
};

#[derive(Clone)]
pub(crate) struct RemoteKeysCommandFacade {
    collectors: Arc<CollectorService>,
    credentials: Arc<CredentialService>,
    settings: Arc<SettingsService>,
}

impl RemoteKeysCommandFacade {
    pub(crate) fn new(
        collectors: Arc<CollectorService>,
        credentials: Arc<CredentialService>,
        settings: Arc<SettingsService>,
    ) -> Self {
        Self {
            collectors,
            credentials,
            settings,
        }
    }

    pub(crate) async fn scan_remote_station_keys(
        &self,
        station_id: String,
    ) -> Result<RemoteKeyScanResult, RemoteKeyOperationError> {
        let source = self.source();
        let prepared = tokio::task::spawn_blocking(move || {
            remote_keys::prepare_remote_key_scan_v2(&source, station_id)
        })
        .await
        .map_err(|_| RemoteKeyOperationError::Internal)??;
        remote_keys::finish_remote_key_scan_v2(self.credentials.as_ref(), prepared).await
    }

    pub(crate) async fn create_remote_station_key(
        &self,
        input: CreateRemoteStationKeyInput,
    ) -> Result<CreateRemoteStationKeyResult, RemoteKeyOperationError> {
        let prepared = self
            .prepare_remote_key_save(move |source| {
                remote_keys::prepare_remote_key_creation_v2(&source, input)
            })
            .await?;
        remote_keys::finish_remote_key_creation_v2(self.credentials.as_ref(), prepared).await
    }

    pub(crate) async fn create_local_station_key_from_remote(
        &self,
        station_id: String,
        remote_key_id: String,
    ) -> Result<CreateLocalStationKeyFromRemoteResult, RemoteKeyOperationError> {
        let prepared = self
            .prepare_remote_key_save(move |source| {
                remote_keys::prepare_local_key_from_remote_v2(&source, station_id, remote_key_id)
            })
            .await?;
        remote_keys::finish_local_key_from_remote_v2(self.credentials.as_ref(), prepared).await
    }

    fn source(&self) -> V2CollectorSourceAdapter {
        V2CollectorSourceAdapter::new(
            Arc::clone(&self.collectors),
            Arc::clone(&self.credentials),
            Arc::clone(&self.settings),
        )
    }

    async fn prepare_remote_key_save<F>(
        &self,
        prepare: F,
    ) -> Result<PreparedRemoteKeySave, RemoteKeyOperationError>
    where
        F: FnOnce(
                V2CollectorSourceAdapter,
            ) -> Result<PreparedRemoteKeySave, RemoteKeyOperationError>
            + Send
            + 'static,
    {
        let source = self.source();
        tokio::task::spawn_blocking(move || prepare(source))
            .await
            .map_err(|_| RemoteKeyOperationError::Internal)?
    }
}
