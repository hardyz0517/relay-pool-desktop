pub(crate) mod change_logs;
pub(crate) mod channel_monitor_mutations;
pub(crate) mod channel_monitor_operations;
pub(crate) mod channel_monitor_reads;
pub(crate) mod collector_facts;
pub(crate) mod data_migration;
pub(crate) mod operations;
pub(crate) mod pricing_mutations;
pub(crate) mod pricing_reads;
pub(crate) mod provider_drafts;
pub(crate) mod proxy_workspace_reads;
pub(crate) mod routing_health_reads;
pub(crate) mod routing_mutations;
pub(crate) mod runtime_status;
pub(crate) mod settings;
pub(crate) mod station_collector_operations;
pub(crate) mod station_keys;
pub(crate) mod stations;
pub(crate) mod updater_data_recovery;

#[cfg(test)]
pub use settings::SettingsDto;
pub use stations::StationDto;

use crate::commands::error::{
    CommandError, CommandErrorCode, PublicErrorDetails, PublicFieldError,
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyInputDto {}

impl EmptyInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, CommandError> {
        serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The command does not accept input fields.",
            )
        })
    }
}

fn invalid_input(field: &'static str, code: &'static str, message: &'static str) -> CommandError {
    CommandError::try_new(
        CommandErrorCode::InvalidInput,
        "The command input is invalid.",
        false,
        Some(PublicErrorDetails::Validation {
            fields: vec![PublicFieldError {
                field: field.into(),
                code: code.into(),
                message: message.into(),
            }],
        }),
        None,
    )
    .expect("transport validation errors use bounded static text")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_rejects_unknown_fields() {
        EmptyInputDto::parse(serde_json::json!({})).expect("empty input");
        let error = EmptyInputDto::parse(serde_json::json!({ "unexpected": true }))
            .expect_err("unknown field");
        assert_eq!(error.code, CommandErrorCode::InvalidInput);
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
pub struct TypeDescriptor {
    pub name: &'static str,
    pub typescript: &'static str,
}

#[cfg_attr(not(test), allow(dead_code))]
pub const REGISTERED_TYPES: &[TypeDescriptor] = &[
    change_logs::CHANGE_LOGS_TYPE,
    channel_monitor_mutations::CHANNEL_MONITOR_MUTATIONS_TYPE,
    channel_monitor_operations::CHANNEL_MONITOR_OPERATIONS_TYPE,
    channel_monitor_reads::CHANNEL_MONITOR_READS_TYPE,
    collector_facts::COLLECTOR_FACTS_TYPE,
    data_migration::DATA_MIGRATION_TYPE,
    operations::OPERATIONS_TYPE,
    pricing_reads::PRICING_READS_TYPE,
    pricing_mutations::PRICING_MUTATIONS_TYPE,
    proxy_workspace_reads::PROXY_WORKSPACE_READS_TYPE,
    provider_drafts::PROVIDER_DRAFTS_TYPE,
    routing_health_reads::ROUTING_HEALTH_READS_TYPE,
    routing_mutations::ROUTING_MUTATIONS_TYPE,
    runtime_status::RUNTIME_STATUS_TYPE,
    settings::SETTINGS_TYPE,
    station_collector_operations::STATION_COLLECTOR_OPERATIONS_TYPE,
    station_keys::STATION_KEY_TYPE,
    stations::STATION_TYPE,
    updater_data_recovery::UPDATER_DATA_RECOVERY_TYPE,
    TypeDescriptor {
        name: "RuntimeContractInfo",
        typescript: crate::ipc::runtime_contract::RUNTIME_CONTRACT_TYPESCRIPT,
    },
    TypeDescriptor {
        name: "CommandError",
        typescript: crate::commands::error::COMMAND_ERROR_TYPESCRIPT,
    },
];
