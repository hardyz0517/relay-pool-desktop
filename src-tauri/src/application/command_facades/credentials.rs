use std::sync::Arc;

use crate::{
    application::{credentials::CredentialService, error::ApplicationError},
    models::credentials::{
        StationCredentials, UpdateStationCredentialsInput, UpdateStationSessionInput,
    },
};

#[derive(Clone)]
pub(crate) struct CredentialsCommandFacade {
    credentials: Arc<CredentialService>,
}

impl CredentialsCommandFacade {
    pub(crate) fn new(credentials: Arc<CredentialService>) -> Self {
        Self { credentials }
    }

    pub(crate) async fn get_station_credentials(
        &self,
        station_id: String,
    ) -> Result<StationCredentials, ApplicationError> {
        self.credentials.get_station_credentials(station_id).await
    }

    pub(crate) async fn update_station_credentials(
        &self,
        input: UpdateStationCredentialsInput,
    ) -> Result<StationCredentials, ApplicationError> {
        self.credentials.update_station_credentials(input).await
    }

    pub(crate) async fn update_station_session(
        &self,
        input: UpdateStationSessionInput,
    ) -> Result<StationCredentials, ApplicationError> {
        self.credentials.update_station_session(input).await
    }

    pub(crate) async fn clear_station_credentials(
        &self,
        station_id: String,
    ) -> Result<StationCredentials, ApplicationError> {
        self.credentials.clear_station_credentials(station_id).await
    }
}
