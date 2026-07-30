use std::sync::Arc;

use crate::{
    application::{
        collectors::CollectorService, credentials::CredentialService, error::ApplicationError,
        settings::SettingsService,
    },
    background_tasks::{BlockingExecutor, BlockingExecutorError},
    models::remote_keys::{
        CreateLocalStationKeyFromRemoteResult, CreateRemoteStationKeyInput,
        CreateRemoteStationKeyResult, DeleteRemoteStationKeyResult, RemoteKeyScanResult,
    },
    observability::correlation,
    outbound::AsyncOutboundClient,
    services::{
        collectors::{orchestration::ProviderRegistry, V2CollectorSourceAdapter},
        remote_keys::{self, RemoteKeyOperationError},
    },
};

#[derive(Clone)]
pub(crate) struct RemoteKeysCommandFacade {
    collectors: Arc<CollectorService>,
    credentials: Arc<CredentialService>,
    settings: Arc<SettingsService>,
    blocking: BlockingExecutor,
    outbound: AsyncOutboundClient,
    providers: Arc<ProviderRegistry>,
}

impl RemoteKeysCommandFacade {
    pub(crate) fn new(
        collectors: Arc<CollectorService>,
        credentials: Arc<CredentialService>,
        settings: Arc<SettingsService>,
        blocking: BlockingExecutor,
        outbound: AsyncOutboundClient,
        providers: Arc<ProviderRegistry>,
    ) -> Self {
        Self {
            collectors,
            credentials,
            settings,
            blocking,
            outbound,
            providers,
        }
    }

    pub(crate) async fn scan_remote_station_keys(
        &self,
        station_id: String,
    ) -> Result<RemoteKeyScanResult, RemoteKeyOperationError> {
        let station_id_for_probe = station_id.clone();
        let newapi_prepared = self
            .prepare_remote_key_context("remote_key_prepare_newapi_scan", move |source| {
                remote_keys::prepare_newapi_remote_key_driver_context_v2(
                    &source,
                    station_id_for_probe,
                )
            })
            .await?;
        if let Some(prepared) = newapi_prepared {
            let prepared = remote_keys::prepare_newapi_remote_key_scan_v2(
                self.providers.as_ref(),
                &self.outbound,
                prepared,
                tokio_util::sync::CancellationToken::new(),
                current_correlation_id(),
            )
            .await?;
            return remote_keys::finish_remote_key_scan_v2(self.credentials.as_ref(), prepared)
                .await;
        }
        let station_id_for_probe = station_id.clone();
        let sub2api_prepared = self
            .prepare_remote_key_context("remote_key_prepare_sub2api_scan", move |source| {
                remote_keys::prepare_sub2api_remote_key_driver_context_v2(
                    &source,
                    station_id_for_probe,
                )
            })
            .await?;
        if let Some(prepared) = sub2api_prepared {
            let prepared = remote_keys::prepare_sub2api_remote_key_scan_v2(
                self.providers.as_ref(),
                &self.outbound,
                prepared,
                tokio_util::sync::CancellationToken::new(),
                current_correlation_id(),
            )
            .await?;
            return remote_keys::finish_remote_key_scan_v2(self.credentials.as_ref(), prepared)
                .await;
        }
        let prepared = self
            .prepare_remote_key_context("remote_key_prepare_unsupported_scan", move |source| {
                remote_keys::prepare_unsupported_remote_key_scan_v2(&source, station_id)
            })
            .await?;
        remote_keys::finish_remote_key_scan_v2(self.credentials.as_ref(), prepared).await
    }

    pub(crate) async fn create_remote_station_key(
        &self,
        input: CreateRemoteStationKeyInput,
    ) -> Result<CreateRemoteStationKeyResult, RemoteKeyOperationError> {
        let station_id = input.station_id.clone();
        let newapi_prepared = self
            .prepare_remote_key_context("remote_key_prepare_newapi_create", move |source| {
                remote_keys::prepare_newapi_remote_key_driver_context_v2(&source, station_id)
            })
            .await?;
        if let Some(prepared) = newapi_prepared {
            let prepared = remote_keys::prepare_newapi_remote_key_creation_v2(
                self.providers.as_ref(),
                &self.outbound,
                prepared,
                input,
                tokio_util::sync::CancellationToken::new(),
                current_correlation_id(),
            )
            .await?;
            return remote_keys::finish_remote_key_creation_v2(self.credentials.as_ref(), prepared)
                .await;
        }
        let station_id = input.station_id.clone();
        let sub2api_prepared = self
            .prepare_remote_key_context("remote_key_prepare_sub2api_create", move |source| {
                remote_keys::prepare_sub2api_remote_key_driver_context_v2(&source, station_id)
            })
            .await?;
        if let Some(prepared) = sub2api_prepared {
            let prepared = remote_keys::prepare_sub2api_remote_key_creation_v2(
                self.providers.as_ref(),
                &self.outbound,
                prepared,
                input,
                tokio_util::sync::CancellationToken::new(),
                current_correlation_id(),
            )
            .await?;
            return remote_keys::finish_remote_key_creation_v2(self.credentials.as_ref(), prepared)
                .await;
        }
        let _ = input;
        Err(RemoteKeyOperationError::Unsupported)
    }

    pub(crate) async fn create_local_station_key_from_remote(
        &self,
        station_id: String,
        remote_key_id: String,
    ) -> Result<CreateLocalStationKeyFromRemoteResult, RemoteKeyOperationError> {
        if self
            .credentials
            .list_remote_station_keys(station_id.clone())
            .await?
            .into_iter()
            .find(|key| key.id == remote_key_id)
            .is_some_and(|key| key.matched_station_key_id.is_some())
        {
            return Err(RemoteKeyOperationError::Application(
                ApplicationError::ConstraintViolation,
            ));
        }
        let station_id_for_probe = station_id.clone();
        let newapi_prepared = self
            .prepare_remote_key_context("remote_key_prepare_newapi_reveal", move |source| {
                remote_keys::prepare_newapi_remote_key_driver_context_v2(
                    &source,
                    station_id_for_probe,
                )
            })
            .await?;
        if let Some(prepared) = newapi_prepared {
            let prepared = remote_keys::prepare_newapi_local_key_from_remote_v2(
                self.providers.as_ref(),
                &self.outbound,
                prepared,
                remote_key_id,
                tokio_util::sync::CancellationToken::new(),
                current_correlation_id(),
            )
            .await?;
            return remote_keys::finish_local_key_from_remote_v2(
                self.credentials.as_ref(),
                prepared,
            )
            .await;
        }
        let station_id_for_probe = station_id.clone();
        let sub2api_prepared = self
            .prepare_remote_key_context("remote_key_prepare_sub2api_reveal", move |source| {
                remote_keys::prepare_sub2api_remote_key_driver_context_v2(
                    &source,
                    station_id_for_probe,
                )
            })
            .await?;
        if let Some(prepared) = sub2api_prepared {
            let prepared = remote_keys::prepare_sub2api_local_key_from_remote_v2(
                self.providers.as_ref(),
                &self.outbound,
                prepared,
                remote_key_id,
                tokio_util::sync::CancellationToken::new(),
                current_correlation_id(),
            )
            .await?;
            return remote_keys::finish_local_key_from_remote_v2(
                self.credentials.as_ref(),
                prepared,
            )
            .await;
        }
        let _ = (station_id, remote_key_id);
        Err(RemoteKeyOperationError::Unsupported)
    }

    pub(crate) async fn delete_remote_station_key(
        &self,
        station_id: String,
        remote_key_id: String,
    ) -> Result<DeleteRemoteStationKeyResult, RemoteKeyOperationError> {
        let matched_station_key_id = self
            .credentials
            .list_remote_station_keys(station_id.clone())
            .await?
            .into_iter()
            .find(|key| key.id == remote_key_id)
            .and_then(|key| key.matched_station_key_id);

        let station_id_for_probe = station_id.clone();
        let newapi_prepared = self
            .prepare_remote_key_context("remote_key_prepare_newapi_delete", move |source| {
                remote_keys::prepare_newapi_remote_key_driver_context_v2(
                    &source,
                    station_id_for_probe,
                )
            })
            .await?;
        if let Some(prepared) = newapi_prepared {
            let prepared = remote_keys::prepare_newapi_remote_key_deletion_v2(
                self.providers.as_ref(),
                &self.outbound,
                prepared,
                remote_key_id,
                matched_station_key_id,
                tokio_util::sync::CancellationToken::new(),
                current_correlation_id(),
            )
            .await?;
            return remote_keys::finish_remote_key_deletion_v2(self.credentials.as_ref(), prepared)
                .await;
        }

        let station_id_for_probe = station_id.clone();
        let sub2api_prepared = self
            .prepare_remote_key_context("remote_key_prepare_sub2api_delete", move |source| {
                remote_keys::prepare_sub2api_remote_key_driver_context_v2(
                    &source,
                    station_id_for_probe,
                )
            })
            .await?;
        if let Some(prepared) = sub2api_prepared {
            let prepared = remote_keys::prepare_sub2api_remote_key_deletion_v2(
                self.providers.as_ref(),
                &self.outbound,
                prepared,
                remote_key_id,
                matched_station_key_id,
                tokio_util::sync::CancellationToken::new(),
                current_correlation_id(),
            )
            .await?;
            return remote_keys::finish_remote_key_deletion_v2(self.credentials.as_ref(), prepared)
                .await;
        }
        Err(RemoteKeyOperationError::Unsupported)
    }

    fn source(&self) -> V2CollectorSourceAdapter {
        V2CollectorSourceAdapter::new(
            Arc::clone(&self.collectors),
            Arc::clone(&self.credentials),
            Arc::clone(&self.settings),
        )
    }

    async fn prepare_remote_key_context<T, F>(
        &self,
        kind: &'static str,
        prepare: F,
    ) -> Result<T, RemoteKeyOperationError>
    where
        T: Send + 'static,
        F: FnOnce(V2CollectorSourceAdapter) -> Result<T, RemoteKeyOperationError> + Send + 'static,
    {
        let source = self.source();
        self.blocking
            .submit(kind, None, current_correlation_id(), None, move |_| {
                Ok(prepare(source))
            })
            .map_err(remote_key_blocking_error)?
            .result()
            .await
            .map_err(remote_key_blocking_error)?
    }
}

fn current_correlation_id() -> Option<String> {
    correlation::current().map(|id| id.as_str().to_string())
}

fn remote_key_blocking_error(_error: BlockingExecutorError) -> RemoteKeyOperationError {
    RemoteKeyOperationError::Internal
}
