use std::sync::Arc;

use zeroize::Zeroizing;

use crate::{
    application::{
        credentials::CredentialService, error::ApplicationError, routing::RoutingService,
    },
    models::{routing::StationKeyCapabilities, station_keys::KeyPoolItem},
    outbound::AsyncOutboundClient,
};

#[derive(Debug)]
pub(crate) enum StationKeyConnectivityCommandError {
    Application(ApplicationError),
    Message(String),
}

impl From<ApplicationError> for StationKeyConnectivityCommandError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

pub(crate) struct StationKeyConnectivityProbeTarget {
    pub(crate) key: KeyPoolItem,
    pub(crate) api_key: Zeroizing<String>,
    pub(crate) capabilities: StationKeyCapabilities,
}

#[derive(Clone)]
pub(crate) struct StationKeyConnectivityCommandFacade {
    credentials: Arc<CredentialService>,
    routing: Arc<RoutingService>,
    outbound: AsyncOutboundClient,
}

impl StationKeyConnectivityCommandFacade {
    pub(crate) fn new(
        credentials: Arc<CredentialService>,
        routing: Arc<RoutingService>,
        outbound: AsyncOutboundClient,
    ) -> Self {
        Self {
            credentials,
            routing,
            outbound,
        }
    }

    pub(crate) fn outbound_client(&self) -> AsyncOutboundClient {
        self.outbound.clone()
    }

    pub(crate) async fn prepare_probe_target(
        &self,
        station_key_id: String,
    ) -> Result<StationKeyConnectivityProbeTarget, StationKeyConnectivityCommandError> {
        let key = self
            .credentials
            .list_key_pool_items()
            .await?
            .into_iter()
            .find(|item| item.id == station_key_id)
            .ok_or_else(|| {
                StationKeyConnectivityCommandError::Message(
                    "Station Key does not exist".to_string(),
                )
            })?;
        if !key.api_key_present {
            return Err(StationKeyConnectivityCommandError::Message(
                "Station Key does not have a saved API key".to_string(),
            ));
        }
        let secret = self
            .credentials
            .resolve_station_key_secret(station_key_id.clone())
            .await?;
        let api_key = String::from_utf8(secret.as_bytes().to_vec())
            .map(Zeroizing::new)
            .map_err(|_| {
                StationKeyConnectivityCommandError::Message(
                    "Station Key API key is not valid UTF-8".to_string(),
                )
            })?;
        let capabilities = self
            .credentials
            .get_station_key_capabilities(station_key_id)
            .await?;
        Ok(StationKeyConnectivityProbeTarget {
            key,
            api_key,
            capabilities,
        })
    }

    pub(crate) fn record_station_key_connectivity(
        &self,
        station_key_id: String,
        station_id: String,
        endpoint_revision: i64,
        ok: bool,
        duration_ms: i64,
        message: String,
    ) -> impl std::future::Future<Output = Result<(), ApplicationError>> + Send + '_ {
        async move {
            self.routing
                .record_station_key_connectivity(
                    station_key_id,
                    station_id,
                    endpoint_revision,
                    ok,
                    duration_ms,
                    message,
                )
                .await
        }
    }
}
