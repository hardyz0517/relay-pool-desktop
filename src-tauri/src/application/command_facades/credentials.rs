use std::sync::Arc;

use crate::{
    application::{credentials::CredentialService, error::ApplicationError},
    models::credentials::{
        CommonLoginEmail, CommonLoginOptions, CommonLoginPassword, StationCredentials,
        UpdateStationCredentialsInput, UpdateStationSessionInput, UpsertCommonLoginEmailInput,
        UpsertCommonLoginPasswordInput,
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

    pub(crate) async fn list_common_login_options(
        &self,
    ) -> Result<CommonLoginOptions, ApplicationError> {
        self.credentials.list_common_login_options().await
    }

    pub(crate) async fn upsert_common_login_email(
        &self,
        input: UpsertCommonLoginEmailInput,
    ) -> Result<CommonLoginEmail, ApplicationError> {
        self.credentials.upsert_common_login_email(input).await
    }

    pub(crate) async fn delete_common_login_email(
        &self,
        email_id: String,
    ) -> Result<(), ApplicationError> {
        self.credentials.delete_common_login_email(email_id).await
    }

    pub(crate) async fn upsert_common_login_password(
        &self,
        input: UpsertCommonLoginPasswordInput,
    ) -> Result<CommonLoginPassword, ApplicationError> {
        self.credentials.upsert_common_login_password(input).await
    }

    pub(crate) async fn delete_common_login_password(
        &self,
        password_id: String,
    ) -> Result<(), ApplicationError> {
        self.credentials
            .delete_common_login_password(password_id)
            .await
    }

    pub(crate) async fn get_common_login_password(
        &self,
        password_id: String,
    ) -> Result<String, ApplicationError> {
        self.credentials
            .get_common_login_password(password_id)
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
