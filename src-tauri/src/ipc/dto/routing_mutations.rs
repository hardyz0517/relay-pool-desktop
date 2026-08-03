use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use crate::models::routing::{ModelAlias, StationKeyCapabilities};
use crate::models::{
    routing::{UpdateStationKeyCapabilitiesInput, UpsertModelAliasInput},
    stations::EndpointPingResult,
};

use super::{invalid_input, TypeDescriptor};

const MAX_ID_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 256;
const MAX_TAG_BYTES: usize = 128;
const MAX_NOTE_BYTES: usize = 4_096;
const MAX_MODEL_LIST_ITEMS: usize = 256;
const MAX_ROUTING_TAGS: usize = 64;
const MAX_ROUTING_KEY_IDS: usize = 2_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EndpointPingStatusDto {
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointPingResultDto {
    pub station_id: String,
    pub ok: bool,
    pub status: EndpointPingStatusDto,
    pub latency_ms: Option<i64>,
    pub checked_at: String,
    pub error_summary: Option<String>,
}

impl TryFrom<EndpointPingResult> for EndpointPingResultDto {
    type Error = crate::commands::error::CommandError;

    fn try_from(value: EndpointPingResult) -> Result<Self, Self::Error> {
        let status = match (value.status.as_str(), value.ok) {
            ("success", true) => EndpointPingStatusDto::Success,
            ("failed", false) => EndpointPingStatusDto::Failed,
            _ => return Err(crate::commands::error::CommandError::internal(None)),
        };
        Ok(Self {
            station_id: value.station_id,
            ok: value.ok,
            status,
            latency_ms: value.latency_ms,
            checked_at: value.checked_at,
            error_summary: value.error_summary,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReorderLocalRoutingKeysInputDto {
    pub station_key_ids: Vec<String>,
}

impl ReorderLocalRoutingKeysInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        if input.station_key_ids.len() > MAX_ROUTING_KEY_IDS {
            return Err(invalid_input(
                "stationKeyIds",
                "too_many_items",
                "The routing key order contains too many items.",
            ));
        }
        let mut seen = HashSet::with_capacity(input.station_key_ids.len());
        for id in &input.station_key_ids {
            validate_id("stationKeyIds", id)?;
            if !seen.insert(id.as_str()) {
                return Err(invalid_input(
                    "stationKeyIds",
                    "duplicate_item",
                    "The routing key order contains duplicate items.",
                ));
            }
        }
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteModelAliasInputDto {
    pub id: String,
}

impl DeleteModelAliasInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("id", &input.id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateStationKeyCapabilitiesInputDto {
    pub station_key_id: String,
    pub supports_chat_completions: bool,
    pub supports_responses: bool,
    pub supports_embeddings: bool,
    pub supports_stream: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_reasoning: bool,
    pub model_allowlist: Vec<String>,
    pub model_blocklist: Vec<String>,
    pub preferred_models: Vec<String>,
    pub only_use_as_backup: bool,
    pub routing_tags: Vec<String>,
}

impl UpdateStationKeyCapabilitiesInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("stationKeyId", &input.station_key_id)?;
        validate_unique_text_list(
            "modelAllowlist",
            &input.model_allowlist,
            MAX_MODEL_LIST_ITEMS,
            MAX_MODEL_BYTES,
        )?;
        validate_unique_text_list(
            "modelBlocklist",
            &input.model_blocklist,
            MAX_MODEL_LIST_ITEMS,
            MAX_MODEL_BYTES,
        )?;
        validate_unique_text_list(
            "preferredModels",
            &input.preferred_models,
            MAX_MODEL_LIST_ITEMS,
            MAX_MODEL_BYTES,
        )?;
        validate_unique_text_list(
            "routingTags",
            &input.routing_tags,
            MAX_ROUTING_TAGS,
            MAX_TAG_BYTES,
        )?;
        Ok(input)
    }

    pub fn into_domain(self) -> UpdateStationKeyCapabilitiesInput {
        UpdateStationKeyCapabilitiesInput {
            station_key_id: self.station_key_id,
            supports_chat_completions: self.supports_chat_completions,
            supports_responses: self.supports_responses,
            supports_embeddings: self.supports_embeddings,
            supports_stream: self.supports_stream,
            supports_tools: self.supports_tools,
            supports_vision: self.supports_vision,
            supports_reasoning: self.supports_reasoning,
            model_allowlist: self.model_allowlist,
            model_blocklist: self.model_blocklist,
            preferred_models: self.preferred_models,
            only_use_as_backup: self.only_use_as_backup,
            routing_tags: self.routing_tags,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertModelAliasInputDto {
    pub id: Option<String>,
    pub client_model: String,
    pub upstream_model: String,
    pub enabled: bool,
    pub note: Option<String>,
}

impl UpsertModelAliasInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        if let Some(id) = input.id.as_deref() {
            validate_id("id", id)?;
        }
        validate_text("clientModel", &input.client_model, MAX_MODEL_BYTES, false)?;
        validate_text(
            "upstreamModel",
            &input.upstream_model,
            MAX_MODEL_BYTES,
            false,
        )?;
        if let Some(note) = input.note.as_deref() {
            validate_text("note", note, MAX_NOTE_BYTES, true)?;
        }
        Ok(input)
    }

    pub fn into_domain(self) -> UpsertModelAliasInput {
        UpsertModelAliasInput {
            id: self.id,
            client_model: self.client_model,
            upstream_model: self.upstream_model,
            enabled: self.enabled,
            note: self.note,
        }
    }
}

fn parse_value<T: for<'de> Deserialize<'de>>(
    value: Value,
) -> Result<T, crate::commands::error::CommandError> {
    serde_json::from_value(value).map_err(|_| {
        invalid_input(
            "input",
            "invalid_shape",
            "The routing mutation payload is invalid.",
        )
    })
}

fn validate_id(
    field: &'static str,
    value: &str,
) -> Result<(), crate::commands::error::CommandError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if !valid {
        return Err(invalid_input(
            field,
            "invalid_id",
            "The identifier is invalid.",
        ));
    }
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
    allow_empty: bool,
) -> Result<(), crate::commands::error::CommandError> {
    if (!allow_empty && value.trim().is_empty())
        || value.len() > max_bytes
        || value.chars().any(char::is_control)
    {
        return Err(invalid_input(
            field,
            "invalid_text",
            "The text value is invalid.",
        ));
    }
    Ok(())
}

fn validate_unique_text_list(
    field: &'static str,
    values: &[String],
    max_items: usize,
    max_bytes: usize,
) -> Result<(), crate::commands::error::CommandError> {
    if values.len() > max_items {
        return Err(invalid_input(
            field,
            "too_many_items",
            "The list contains too many items.",
        ));
    }
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        validate_text(field, value, max_bytes, false)?;
        if !seen.insert(value.as_str()) {
            return Err(invalid_input(
                field,
                "duplicate_item",
                "The list contains duplicate items.",
            ));
        }
    }
    Ok(())
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-dto-type-descriptor; owner=ipc; remove_when=descriptor is registered in production binding export"
    )
)]
pub const ROUTING_MUTATIONS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "RoutingMutationsDto",
    typescript: include_str!("routing_mutations.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let ping_input = super::station_keys::StationIdInputDto::parse(serde_json::json!({
        "stationId":"station-1"
    }))
    .expect("endpoint ping fixture input");
    let reorder_input = ReorderLocalRoutingKeysInputDto::parse(serde_json::json!({
        "stationKeyIds":["key-1","key-2"]
    }))
    .expect("routing reorder fixture input");
    let capabilities_input = UpdateStationKeyCapabilitiesInputDto::parse(serde_json::json!({
        "stationKeyId":"key-1",
        "supportsChatCompletions":true,
        "supportsResponses":true,
        "supportsEmbeddings":false,
        "supportsStream":true,
        "supportsTools":false,
        "supportsVision":false,
        "supportsReasoning":false,
        "modelAllowlist":["fixture-model"],
        "modelBlocklist":[],
        "preferredModels":["fixture-model"],
        "onlyUseAsBackup":false,
        "routingTags":["fixture"]
    }))
    .expect("capabilities fixture input");
    let alias_input = UpsertModelAliasInputDto::parse(serde_json::json!({
        "id":null,
        "clientModel":"client-model",
        "upstreamModel":"upstream-model",
        "enabled":true,
        "note":null
    }))
    .expect("alias fixture input");
    let delete_input = DeleteModelAliasInputDto::parse(serde_json::json!({"id":"alias-1"}))
        .expect("delete alias fixture input");

    vec![
        serde_json::json!({
            "command":"ping_station_endpoint",
            "input":ping_input,
            "output":EndpointPingResultDto {
                station_id:"station-1".into(),
                ok:true,
                status:EndpointPingStatusDto::Success,
                latency_ms:Some(20),
                checked_at:"1700000000000".into(),
                error_summary:None,
            }
        }),
        serde_json::json!({
            "command":"reorder_local_routing_keys",
            "input":reorder_input,
            "output":super::proxy_workspace_reads::fixture_workspace()
        }),
        serde_json::json!({
            "command":"update_station_key_capabilities",
            "input":capabilities_input,
            "output":fixture_capabilities()
        }),
        serde_json::json!({
            "command":"upsert_model_alias",
            "input":alias_input,
            "output":fixture_alias()
        }),
        serde_json::json!({"command":"delete_model_alias","input":delete_input,"output":null}),
    ]
}

#[cfg(test)]
fn fixture_capabilities() -> StationKeyCapabilities {
    StationKeyCapabilities {
        station_key_id: "key-1".into(),
        supports_chat_completions: true,
        supports_responses: true,
        supports_embeddings: false,
        supports_stream: true,
        supports_tools: false,
        supports_vision: false,
        supports_reasoning: false,
        model_allowlist: vec!["fixture-model".into()],
        model_blocklist: Vec::new(),
        preferred_models: vec!["fixture-model".into()],
        only_use_as_backup: false,
        routing_tags: vec!["fixture".into()],
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_alias() -> ModelAlias {
    ModelAlias {
        id: "alias-1".into(),
        client_model: "client-model".into(),
        upstream_model: "upstream-model".into(),
        enabled: true,
        note: None,
        created_at: "1700000000000".into(),
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    #[test]
    fn routing_key_reorder_rejects_unknown_duplicate_invalid_and_oversized_ids() {
        let oversized = (0..=MAX_ROUTING_KEY_IDS)
            .map(|index| format!("key-{index}"))
            .collect::<Vec<_>>();
        for value in [
            serde_json::json!({"stationKeyIds":["key-1"],"unexpected":true}),
            serde_json::json!({"stationKeyIds":["key-1","key-1"]}),
            serde_json::json!({"stationKeyIds":["bad id"]}),
            serde_json::json!({"stationKeyIds":oversized}),
        ] {
            let error = ReorderLocalRoutingKeysInputDto::parse(value)
                .expect_err("invalid routing key order");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }

        let parsed = ReorderLocalRoutingKeysInputDto::parse(serde_json::json!({
            "stationKeyIds":[]
        }))
        .expect("empty order is valid");
        assert!(parsed.station_key_ids.is_empty());
    }

    #[test]
    fn endpoint_ping_output_rejects_open_status_values() {
        for (status, ok) in [("unknown", false), ("success", false), ("failed", true)] {
            let error = EndpointPingResultDto::try_from(EndpointPingResult {
                station_id: "station-1".into(),
                ok,
                status: status.into(),
                latency_ms: None,
                checked_at: "1700000000000".into(),
                error_summary: None,
            })
            .expect_err("open or inconsistent output status must fail closed");
            assert_eq!(error.code, CommandErrorCode::Internal);
        }
    }

    #[test]
    fn rejects_unknown_fields_invalid_ids_and_malformed_alias_text() {
        for value in [
            serde_json::json!({"id":"bad id"}),
            serde_json::json!({"id":"alias-1","unexpected":true}),
        ] {
            let error = DeleteModelAliasInputDto::parse(value).expect_err("invalid delete input");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }

        for value in [
            serde_json::json!({"id":null,"clientModel":"","upstreamModel":"upstream","enabled":true,"note":null}),
            serde_json::json!({"id":null,"clientModel":"client","upstreamModel":"upstream\n","enabled":true,"note":null}),
            serde_json::json!({"id":null,"clientModel":"client","upstreamModel":"upstream","enabled":true,"note":null,"unexpected":true}),
        ] {
            let error = UpsertModelAliasInputDto::parse(value).expect_err("invalid alias input");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }

    #[test]
    fn rejects_oversized_or_duplicate_capability_lists() {
        let base = serde_json::json!({
            "stationKeyId":"key-1",
            "supportsChatCompletions":true,
            "supportsResponses":true,
            "supportsEmbeddings":false,
            "supportsStream":true,
            "supportsTools":false,
            "supportsVision":false,
            "supportsReasoning":false,
            "modelAllowlist":[],
            "modelBlocklist":[],
            "preferredModels":[],
            "onlyUseAsBackup":false,
            "routingTags":[]
        });
        for value in [
            {
                let mut value = base.clone();
                value["modelAllowlist"] = serde_json::json!(["same", "same"]);
                value
            },
            {
                let mut value = base.clone();
                value["routingTags"] = serde_json::json!(["bad\ntag"]);
                value
            },
            {
                let mut value = base;
                value["unexpected"] = serde_json::json!(true);
                value
            },
        ] {
            let error = UpdateStationKeyCapabilitiesInputDto::parse(value)
                .expect_err("invalid capabilities input");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }
}
