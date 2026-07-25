use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    commands::data_recovery::{DataStoreCandidateView, DataStoreStartupView},
    services::{
        data_store::types::ActivationResult, updater::PublishedUpdateInspection,
        updater::UpdaterNetworkConfig,
    },
};

use super::{invalid_input, TypeDescriptor};

const MAX_VERSION_BYTES: usize = 128;
const MAX_CANDIDATE_ID_BYTES: usize = 512;

pub type ActivationResultDto = ActivationResult;
pub type DataStoreCandidateViewDto = DataStoreCandidateView;
pub type DataStoreStartupViewDto = DataStoreStartupView;
pub type PublishedUpdateInspectionDto = PublishedUpdateInspection;
pub type UpdaterNetworkConfigDto = UpdaterNetworkConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivateDataStoreCandidateInputDto {
    pub candidate_id: String,
}

impl ActivateDataStoreCandidateInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The data store activation payload is invalid.",
            )
        })?;
        validate_candidate_id(&input.candidate_id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateNewDataStoreInputDto {
    pub confirmed: bool,
}

impl CreateNewDataStoreInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The new data store payload is invalid.",
            )
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishedUpdateInspectionInputDto {
    pub current_version: String,
}

fn validate_candidate_id(value: &str) -> Result<(), crate::commands::error::CommandError> {
    if value.trim().is_empty()
        || value.len() > MAX_CANDIDATE_ID_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(invalid_input(
            "candidateId",
            "invalid_id",
            "The data store candidate id is invalid.",
        ));
    }
    Ok(())
}

impl PublishedUpdateInspectionInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The updater inspection payload is invalid.",
            )
        })?;
        if input.current_version.trim().is_empty()
            || input.current_version.len() > MAX_VERSION_BYTES
            || input
                .current_version
                .chars()
                .any(|character| character.is_control())
        {
            return Err(invalid_input(
                "currentVersion",
                "invalid_version",
                "The current application version is invalid.",
            ));
        }
        Ok(input)
    }
}

pub const UPDATER_DATA_RECOVERY_TYPE: TypeDescriptor = TypeDescriptor {
    name: "UpdaterDataRecovery",
    typescript: include_str!("updater_data_recovery.typescript.txt"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_inspection_rejects_unknown_empty_and_control_version_input() {
        PublishedUpdateInspectionInputDto::parse(serde_json::json!({
            "currentVersion": "0.3.2"
        }))
        .expect("valid version");

        for value in [
            serde_json::json!({ "currentVersion": "" }),
            serde_json::json!({ "currentVersion": "   " }),
            serde_json::json!({ "currentVersion": "0.3.2\nsecret" }),
            serde_json::json!({ "currentVersion": "0.3.2", "unexpected": true }),
        ] {
            let error = PublishedUpdateInspectionInputDto::parse(value)
                .expect_err("invalid updater version input");
            assert_eq!(
                error.code,
                crate::commands::error::CommandErrorCode::InvalidInput
            );
        }
    }

    #[test]
    fn data_recovery_inputs_reject_unknown_empty_and_control_candidate_ids() {
        ActivateDataStoreCandidateInputDto::parse(serde_json::json!({
            "candidateId": "candidate-1"
        }))
        .expect("valid candidate");
        CreateNewDataStoreInputDto::parse(serde_json::json!({
            "confirmed": true
        }))
        .expect("valid confirmation");

        for value in [
            serde_json::json!({ "candidateId": "" }),
            serde_json::json!({ "candidateId": "   " }),
            serde_json::json!({ "candidateId": "candidate\n1" }),
            serde_json::json!({ "candidateId": "a".repeat(MAX_CANDIDATE_ID_BYTES + 1) }),
            serde_json::json!({ "candidateId": "candidate-1", "unexpected": true }),
        ] {
            let error = ActivateDataStoreCandidateInputDto::parse(value)
                .expect_err("invalid data store candidate input");
            assert_eq!(
                error.code,
                crate::commands::error::CommandErrorCode::InvalidInput
            );
        }

        let error = CreateNewDataStoreInputDto::parse(serde_json::json!({
            "confirmed": true,
            "unexpected": true
        }))
        .expect_err("unknown confirmation field");
        assert_eq!(
            error.code,
            crate::commands::error::CommandErrorCode::InvalidInput
        );
    }
}
