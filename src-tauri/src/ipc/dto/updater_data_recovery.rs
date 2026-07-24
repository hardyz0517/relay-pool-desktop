use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::services::updater::{PublishedUpdateInspection, UpdaterNetworkConfig};

use super::{invalid_input, TypeDescriptor};

const MAX_VERSION_BYTES: usize = 128;

pub type PublishedUpdateInspectionDto = PublishedUpdateInspection;
pub type UpdaterNetworkConfigDto = UpdaterNetworkConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishedUpdateInspectionInputDto {
    pub current_version: String,
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
}
