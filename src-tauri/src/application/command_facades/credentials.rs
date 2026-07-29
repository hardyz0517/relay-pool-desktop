use std::sync::Arc;

use crate::{
    application::{credentials::CredentialService, error::ApplicationError},
    models::credentials::{
        CommonLoginProfile, StationCredentials, UpdateStationCredentialsInput,
        UpdateStationSessionInput, UpsertCommonLoginProfileInput,
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

    pub(crate) async fn list_common_login_profiles(
        &self,
    ) -> Result<Vec<CommonLoginProfile>, ApplicationError> {
        self.credentials.list_common_login_profiles().await
    }

    pub(crate) async fn upsert_common_login_profile(
        &self,
        input: UpsertCommonLoginProfileInput,
    ) -> Result<CommonLoginProfile, ApplicationError> {
        self.credentials.upsert_common_login_profile(input).await
    }

    pub(crate) async fn delete_common_login_profile(
        &self,
        profile_id: String,
    ) -> Result<(), ApplicationError> {
        self.credentials
            .delete_common_login_profile(profile_id)
            .await
    }

    pub(crate) async fn get_common_login_profile_password(
        &self,
        profile_id: String,
    ) -> Result<String, ApplicationError> {
        self.credentials
            .get_common_login_profile_password(profile_id)
            .await
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
