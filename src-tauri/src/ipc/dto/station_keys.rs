use std::collections::HashSet;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

use crate::models::{
    credentials::{
        CommonLoginProfile, StationCredentials, UpdateStationCredentialsInput,
        UpdateStationSessionInput, UpsertCommonLoginProfileInput,
    },
    group_facts::UpdateStationKeyGroupBindingInput,
    remote_keys::{
        BindRemoteStationKeyInput, CreateLocalStationKeyFromRemoteResult,
        CreateRemoteStationKeyInput, CreateRemoteStationKeyResult, RemoteKeyCapability,
        RemoteKeyScanResult, RemoteStationKey,
    },
    shared_capabilities::{
        SaveStationKeyMode, SaveStationKeyWithDefaultsInput, SaveStationKeyWithDefaultsResult,
        StationKeyGroupSelectionKind,
    },
    station_keys::{CreateStationKeyInput, KeyPoolItem, StationKey, UpdateStationKeyInput},
};
#[cfg(test)]
use crate::models::{remote_keys::RemoteKeyMatchStatus, routing::StationKeyCapabilities};

use super::{invalid_input, TypeDescriptor};

const MAX_ID_BYTES: usize = 128;
const MAX_NAME_BYTES: usize = 256;
const MAX_SECRET_BYTES: usize = 16_384;
const MAX_COOKIE_BYTES: usize = 65_536;
const MAX_NOTE_BYTES: usize = 4_096;
const MAX_TEXT_BYTES: usize = 512;
const MAX_KEY_ORDER_ITEMS: usize = 2_000;
const MAX_PRIORITY: i64 = 1_000_000;
const MAX_CONCURRENCY: i64 = 10_000;
const MAX_LOAD_FACTOR: i64 = 1_000_000;
const MAX_RATE_MULTIPLIER: f64 = 1_000_000.0;

pub type StationKeyDto = StationKey;
pub type KeyPoolItemDto = KeyPoolItem;
pub type RemoteKeyCapabilityDto = RemoteKeyCapability;
pub type RemoteStationKeyDto = RemoteStationKey;
pub type RemoteKeyScanResultDto = RemoteKeyScanResult;
pub type CreateRemoteStationKeyResultDto = CreateRemoteStationKeyResult;
pub type CreateLocalStationKeyFromRemoteResultDto = CreateLocalStationKeyFromRemoteResult;
pub type StationCredentialsDto = StationCredentials;
pub type CommonLoginProfileDto = CommonLoginProfile;
pub type SaveStationKeyWithDefaultsResultDto = SaveStationKeyWithDefaultsResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StationIdInputDto {
    pub station_id: String,
}

impl StationIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("stationId", &input.station_id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StationKeyIdInputDto {
    pub id: String,
}

impl StationKeyIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("id", &input.id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommonLoginProfileIdInputDto {
    pub id: String,
}

impl CommonLoginProfileIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("id", &input.id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertCommonLoginProfileInputDto {
    pub id: Option<String>,
    pub email: String,
    pub password: Option<String>,
}

impl UpsertCommonLoginProfileInputDto {
    pub fn parse(
        value: Value,
    ) -> Result<UpsertCommonLoginProfileInput, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        if let Some(id) = input.id.as_deref() {
            validate_id("id", id)?;
        }
        validate_text("email", &input.email, MAX_NAME_BYTES, false)?;
        validate_optional_secret("password", input.password.as_deref(), MAX_SECRET_BYTES)?;
        Ok(UpsertCommonLoginProfileInput {
            id: input.id,
            email: input.email.trim().to_string(),
            password: input.password,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteStationKeyInputDto {
    pub remote_key_id: String,
    pub station_id: String,
}

impl RemoteStationKeyInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("remoteKeyId", &input.remote_key_id)?;
        validate_id("stationId", &input.station_id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReorderKeyPoolInputDto {
    pub key_ids: Vec<String>,
}

impl ReorderKeyPoolInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id_list("keyIds", &input.key_ids)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReorderStationKeysInputDto {
    pub station_id: String,
    pub key_ids: Vec<String>,
}

impl ReorderStationKeysInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("stationId", &input.station_id)?;
        validate_id_list("keyIds", &input.key_ids)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StationKeyConnectivityInputDto {
    pub station_key_id: String,
    pub model: String,
}

impl StationKeyConnectivityInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("stationKeyId", &input.station_key_id)?;
        validate_text("model", &input.model, MAX_TEXT_BYTES, false)?;
        Ok(Self {
            station_key_id: input.station_key_id,
            model: input.model.trim().to_string(),
        })
    }
}

pub struct CreateStationKeyInputDto;
impl CreateStationKeyInputDto {
    pub fn parse(
        value: Value,
    ) -> Result<CreateStationKeyInput, crate::commands::error::CommandError> {
        validate_object_keys(
            &value,
            &[
                "stationId",
                "name",
                "apiKey",
                "enabled",
                "priority",
                "maxConcurrency",
                "loadFactor",
                "schedulable",
                "groupName",
                "tierLabel",
                "groupBindingId",
                "groupIdHash",
                "rateMultiplier",
                "manualRateMultiplier",
                "rateSource",
                "balanceScope",
                "note",
            ],
        )?;
        let input: CreateStationKeyInput = parse_value(value)?;
        validate_station_key_fields(
            &input.station_id,
            &input.name,
            Some(&input.api_key),
            input.priority,
            input.max_concurrency,
            input.load_factor,
            input.group_name.as_deref(),
            input.tier_label.as_deref(),
            input.group_binding_id.as_deref(),
            input.group_id_hash.as_deref(),
            input.rate_multiplier,
            input.manual_rate_multiplier,
            input.rate_source.as_deref(),
            input.balance_scope.as_deref(),
            input.note.as_deref(),
        )?;
        if input.api_key.trim().is_empty() {
            return Err(invalid_input(
                "apiKey",
                "required",
                "An API key is required.",
            ));
        }
        Ok(input)
    }
}

pub struct UpdateStationKeyInputDto;
impl UpdateStationKeyInputDto {
    pub fn parse(
        value: Value,
    ) -> Result<UpdateStationKeyInput, crate::commands::error::CommandError> {
        validate_object_keys(
            &value,
            &[
                "id",
                "stationId",
                "name",
                "apiKey",
                "enabled",
                "priority",
                "maxConcurrency",
                "loadFactor",
                "schedulable",
                "groupName",
                "tierLabel",
                "groupBindingId",
                "groupIdHash",
                "rateMultiplier",
                "manualRateMultiplier",
                "rateSource",
                "balanceScope",
                "status",
                "note",
            ],
        )?;
        let input: UpdateStationKeyInput = parse_value(value)?;
        validate_id("id", &input.id)?;
        validate_station_key_fields(
            &input.station_id,
            &input.name,
            input.api_key.as_deref(),
            Some(input.priority),
            Some(input.max_concurrency),
            input.load_factor,
            input.group_name.as_deref(),
            input.tier_label.as_deref(),
            input.group_binding_id.as_deref(),
            input.group_id_hash.as_deref(),
            input.rate_multiplier,
            input.manual_rate_multiplier.flatten(),
            input.rate_source.as_deref(),
            input.balance_scope.as_deref(),
            input.note.as_deref(),
        )?;
        validate_enum(
            "status",
            &input.status,
            &["unchecked", "healthy", "warning", "error", "disabled"],
        )?;
        Ok(input)
    }
}

pub struct CreateRemoteStationKeyInputDto;
impl CreateRemoteStationKeyInputDto {
    pub fn parse(
        value: Value,
    ) -> Result<CreateRemoteStationKeyInput, crate::commands::error::CommandError> {
        validate_object_keys(
            &value,
            &[
                "stationId",
                "name",
                "groupBindingId",
                "groupIdHash",
                "groupName",
            ],
        )?;
        let input: CreateRemoteStationKeyInput = parse_value(value)?;
        validate_id("stationId", &input.station_id)?;
        validate_text("name", &input.name, MAX_NAME_BYTES, false)?;
        validate_optional_id("groupBindingId", input.group_binding_id.as_deref())?;
        validate_optional_text(
            "groupIdHash",
            input.group_id_hash.as_deref(),
            MAX_TEXT_BYTES,
        )?;
        validate_optional_text("groupName", input.group_name.as_deref(), MAX_NAME_BYTES)?;
        Ok(input)
    }
}

pub struct BindRemoteStationKeyInputDto;
impl BindRemoteStationKeyInputDto {
    pub fn parse(
        value: Value,
    ) -> Result<BindRemoteStationKeyInput, crate::commands::error::CommandError> {
        validate_object_keys(&value, &["remoteKeyId", "stationKeyId"])?;
        let input: BindRemoteStationKeyInput = parse_value(value)?;
        validate_id("remoteKeyId", &input.remote_key_id)?;
        validate_id("stationKeyId", &input.station_key_id)?;
        Ok(input)
    }
}

pub struct UpdateStationKeyGroupBindingInputDto;
impl UpdateStationKeyGroupBindingInputDto {
    pub fn parse(
        value: Value,
    ) -> Result<UpdateStationKeyGroupBindingInput, crate::commands::error::CommandError> {
        validate_object_keys(&value, &["stationKeyId", "groupBindingId"])?;
        let input: UpdateStationKeyGroupBindingInput = parse_value(value)?;
        validate_id("stationKeyId", &input.station_key_id)?;
        validate_id("groupBindingId", &input.group_binding_id)?;
        Ok(input)
    }
}

pub struct UpdateStationCredentialsInputDto;
impl UpdateStationCredentialsInputDto {
    pub fn parse(
        value: Value,
    ) -> Result<UpdateStationCredentialsInput, crate::commands::error::CommandError> {
        validate_object_keys(
            &value,
            &[
                "stationId",
                "loginUsername",
                "loginPassword",
                "rememberPassword",
            ],
        )?;
        let input: UpdateStationCredentialsInput = parse_value(value)?;
        validate_id("stationId", &input.station_id)?;
        validate_optional_text(
            "loginUsername",
            input.login_username.as_deref(),
            MAX_NAME_BYTES,
        )?;
        validate_optional_secret(
            "loginPassword",
            input.login_password.as_deref(),
            MAX_SECRET_BYTES,
        )?;
        Ok(input)
    }
}

pub struct UpdateStationSessionInputDto;
impl UpdateStationSessionInputDto {
    pub fn parse(
        value: Value,
    ) -> Result<UpdateStationSessionInput, crate::commands::error::CommandError> {
        validate_object_keys(
            &value,
            &[
                "stationId",
                "accessToken",
                "refreshToken",
                "cookie",
                "newapiUserId",
                "tokenExpiresAt",
            ],
        )?;
        let input: UpdateStationSessionInput = parse_value(value)?;
        validate_id("stationId", &input.station_id)?;
        validate_optional_secret(
            "accessToken",
            input.access_token.as_deref(),
            MAX_SECRET_BYTES,
        )?;
        validate_optional_secret(
            "refreshToken",
            input.refresh_token.as_deref(),
            MAX_SECRET_BYTES,
        )?;
        validate_optional_secret("cookie", input.cookie.as_deref(), MAX_COOKIE_BYTES)?;
        validate_optional_text(
            "newapiUserId",
            input.newapi_user_id.as_deref(),
            MAX_ID_BYTES,
        )?;
        validate_optional_text(
            "tokenExpiresAt",
            input.token_expires_at.as_deref(),
            MAX_TEXT_BYTES,
        )?;
        Ok(input)
    }
}

pub struct SaveStationKeyWithDefaultsInputDto;
impl SaveStationKeyWithDefaultsInputDto {
    pub fn parse(
        value: Value,
    ) -> Result<SaveStationKeyWithDefaultsInput, crate::commands::error::CommandError> {
        validate_object_keys(
            &value,
            &[
                "mode",
                "id",
                "stationId",
                "name",
                "apiKey",
                "enabled",
                "schedulable",
                "priority",
                "tierLabel",
                "balanceScope",
                "status",
                "note",
                "groupSelection",
                "capabilities",
            ],
        )?;
        validate_group_selection_shape(&value)?;
        validate_nested_object_keys(
            &value,
            "capabilities",
            &[
                "stationKeyId",
                "supportsChatCompletions",
                "supportsResponses",
                "supportsEmbeddings",
                "supportsStream",
                "supportsTools",
                "supportsVision",
                "supportsReasoning",
                "modelAllowlist",
                "modelBlocklist",
                "preferredModels",
                "onlyUseAsBackup",
                "routingTags",
            ],
        )?;
        let input: SaveStationKeyWithDefaultsInput = parse_value(value)?;
        validate_id("stationId", &input.station_id)?;
        validate_text("name", &input.name, MAX_NAME_BYTES, false)?;
        validate_optional_secret("apiKey", input.api_key.as_deref(), MAX_SECRET_BYTES)?;
        validate_optional_text("tierLabel", input.tier_label.as_deref(), MAX_NAME_BYTES)?;
        validate_optional_text(
            "balanceScope",
            input.balance_scope.as_deref(),
            MAX_TEXT_BYTES,
        )?;
        if let Some(status) = input.status.as_deref() {
            validate_enum(
                "status",
                status,
                &["unchecked", "healthy", "warning", "error", "disabled"],
            )?;
        }
        validate_optional_text("note", input.note.as_deref(), MAX_NOTE_BYTES)?;
        if let Some(priority) = input.priority {
            validate_range("priority", priority, 0, MAX_PRIORITY)?;
        }
        match input.mode {
            SaveStationKeyMode::Create => {
                if input.id.is_some()
                    || input
                        .api_key
                        .as_deref()
                        .is_none_or(|value| value.trim().is_empty())
                {
                    return Err(invalid_input(
                        "input",
                        "invalid_create",
                        "The create payload is invalid.",
                    ));
                }
                if input.group_selection.kind == StationKeyGroupSelectionKind::Keep {
                    return Err(invalid_input(
                        "groupSelection",
                        "invalid_mode",
                        "Create cannot keep an existing group.",
                    ));
                }
            }
            SaveStationKeyMode::Update => {
                validate_id("id", input.id.as_deref().unwrap_or_default())?
            }
        }
        if input.group_selection.kind == StationKeyGroupSelectionKind::Set {
            validate_id(
                "groupBindingId",
                input
                    .group_selection
                    .group_binding_id
                    .as_deref()
                    .unwrap_or_default(),
            )?;
            validate_optional_text(
                "groupIdHash",
                input.group_selection.group_id_hash.as_deref(),
                MAX_TEXT_BYTES,
            )?;
            validate_optional_text(
                "groupName",
                input.group_selection.group_name.as_deref(),
                MAX_NAME_BYTES,
            )?;
        }
        if let Some(capabilities) = &input.capabilities {
            validate_capabilities(capabilities)?;
        }
        Ok(input)
    }
}

fn parse_value<T: DeserializeOwned>(
    value: Value,
) -> Result<T, crate::commands::error::CommandError> {
    serde_json::from_value(value).map_err(|_| {
        invalid_input(
            "input",
            "invalid_shape",
            "The station key payload is invalid.",
        )
    })
}

fn validate_object_keys(
    value: &Value,
    allowed: &[&str],
) -> Result<(), crate::commands::error::CommandError> {
    let object = value.as_object().ok_or_else(|| {
        invalid_input(
            "input",
            "invalid_shape",
            "The station key payload is invalid.",
        )
    })?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid_input(
            "input",
            "unknown_field",
            "The station key payload contains an unknown field.",
        ));
    }
    Ok(())
}

fn validate_nested_object_keys(
    value: &Value,
    field: &'static str,
    allowed: &[&str],
) -> Result<(), crate::commands::error::CommandError> {
    let Some(nested) = value.get(field) else {
        return Ok(());
    };
    if nested.is_null() {
        return Ok(());
    }
    let object = nested
        .as_object()
        .ok_or_else(|| invalid_input(field, "invalid_shape", "The nested payload is invalid."))?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid_input(
            field,
            "unknown_field",
            "The nested payload contains an unknown field.",
        ));
    }
    Ok(())
}

fn validate_group_selection_shape(
    value: &Value,
) -> Result<(), crate::commands::error::CommandError> {
    let selection = value
        .get("groupSelection")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            invalid_input(
                "groupSelection",
                "invalid_shape",
                "The group selection payload is invalid.",
            )
        })?;
    let kind = selection
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            invalid_input(
                "groupSelection",
                "invalid_kind",
                "The group selection kind is invalid.",
            )
        })?;
    let allowed = match kind {
        "keep" | "clear" => &["kind"][..],
        "set" => &["kind", "groupBindingId", "groupIdHash", "groupName"][..],
        _ => {
            return Err(invalid_input(
                "groupSelection",
                "invalid_kind",
                "The group selection kind is invalid.",
            ))
        }
    };
    if selection.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid_input(
            "groupSelection",
            "invalid_variant_field",
            "The group selection contains a field that is invalid for its kind.",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_station_key_fields(
    station_id: &str,
    name: &str,
    api_key: Option<&str>,
    priority: Option<i64>,
    max_concurrency: Option<i64>,
    load_factor: Option<i64>,
    group_name: Option<&str>,
    tier_label: Option<&str>,
    group_binding_id: Option<&str>,
    group_id_hash: Option<&str>,
    rate_multiplier: Option<f64>,
    manual_rate_multiplier: Option<f64>,
    rate_source: Option<&str>,
    balance_scope: Option<&str>,
    note: Option<&str>,
) -> Result<(), crate::commands::error::CommandError> {
    validate_id("stationId", station_id)?;
    validate_text("name", name, MAX_NAME_BYTES, false)?;
    validate_optional_secret("apiKey", api_key, MAX_SECRET_BYTES)?;
    if let Some(value) = priority {
        validate_range("priority", value, 0, MAX_PRIORITY)?;
    }
    if let Some(value) = max_concurrency {
        validate_range("maxConcurrency", value, 1, MAX_CONCURRENCY)?;
    }
    if let Some(value) = load_factor {
        validate_range("loadFactor", value, 0, MAX_LOAD_FACTOR)?;
    }
    validate_optional_text("groupName", group_name, MAX_NAME_BYTES)?;
    validate_optional_text("tierLabel", tier_label, MAX_NAME_BYTES)?;
    validate_optional_id("groupBindingId", group_binding_id)?;
    validate_optional_text("groupIdHash", group_id_hash, MAX_TEXT_BYTES)?;
    validate_optional_multiplier("rateMultiplier", rate_multiplier)?;
    validate_optional_multiplier("manualRateMultiplier", manual_rate_multiplier)?;
    validate_optional_text("rateSource", rate_source, MAX_TEXT_BYTES)?;
    validate_optional_text("balanceScope", balance_scope, MAX_TEXT_BYTES)?;
    validate_optional_text("note", note, MAX_NOTE_BYTES)
}

fn validate_capabilities(
    input: &crate::models::routing::UpdateStationKeyCapabilitiesInput,
) -> Result<(), crate::commands::error::CommandError> {
    validate_id("stationKeyId", &input.station_key_id)?;
    for (field, values) in [
        ("modelAllowlist", &input.model_allowlist),
        ("modelBlocklist", &input.model_blocklist),
        ("preferredModels", &input.preferred_models),
        ("routingTags", &input.routing_tags),
    ] {
        if values.len() > 1_000 {
            return Err(invalid_input(
                field,
                "too_many_items",
                "The list contains too many items.",
            ));
        }
        for value in values {
            validate_text(field, value, MAX_TEXT_BYTES, false)?;
        }
    }
    Ok(())
}

fn validate_id_list(
    field: &'static str,
    values: &[String],
) -> Result<(), crate::commands::error::CommandError> {
    if values.len() > MAX_KEY_ORDER_ITEMS {
        return Err(invalid_input(
            field,
            "too_many_items",
            "The key order contains too many items.",
        ));
    }
    let mut unique = HashSet::with_capacity(values.len());
    for value in values {
        validate_id(field, value)?;
        if !unique.insert(value) {
            return Err(invalid_input(
                field,
                "duplicate_item",
                "The key order contains a duplicate ID.",
            ));
        }
    }
    Ok(())
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

fn validate_optional_id(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), crate::commands::error::CommandError> {
    if let Some(value) = value {
        validate_id(field, value)?;
    }
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
    allow_empty: bool,
) -> Result<(), crate::commands::error::CommandError> {
    if (!allow_empty && value.trim().is_empty())
        || value.len() > max
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

fn validate_optional_text(
    field: &'static str,
    value: Option<&str>,
    max: usize,
) -> Result<(), crate::commands::error::CommandError> {
    if let Some(value) = value {
        validate_text(field, value, max, true)?;
    }
    Ok(())
}

fn validate_optional_secret(
    _field: &'static str,
    value: Option<&str>,
    max: usize,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_some_and(|value| value.len() > max || value.contains('\0')) {
        return Err(invalid_input(
            "input",
            "invalid_value",
            "The supplied value is invalid.",
        ));
    }
    Ok(())
}

fn validate_range(
    field: &'static str,
    value: i64,
    min: i64,
    max: i64,
) -> Result<(), crate::commands::error::CommandError> {
    if !(min..=max).contains(&value) {
        return Err(invalid_input(
            field,
            "out_of_range",
            "The numeric value is out of range.",
        ));
    }
    Ok(())
}

fn validate_optional_multiplier(
    field: &'static str,
    value: Option<f64>,
) -> Result<(), crate::commands::error::CommandError> {
    if value
        .is_some_and(|value| !value.is_finite() || !(0.0..=MAX_RATE_MULTIPLIER).contains(&value))
    {
        return Err(invalid_input(
            field,
            "out_of_range",
            "The multiplier is out of range.",
        ));
    }
    Ok(())
}

fn validate_enum(
    field: &'static str,
    value: &str,
    allowed: &[&str],
) -> Result<(), crate::commands::error::CommandError> {
    if !allowed.contains(&value) {
        return Err(invalid_input(
            field,
            "invalid_enum",
            "The enum value is invalid.",
        ));
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub const STATION_KEY_TYPE: TypeDescriptor = TypeDescriptor {
    name: "StationKeyDto",
    typescript: include_str!("station_keys.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let station_key = fixture_station_key();
    let key_pool_item = fixture_key_pool_item();
    let capability = fixture_remote_capability();
    let remote_key = fixture_remote_key();
    let credentials = fixture_credentials();
    let common_login_profile = CommonLoginProfile {
        id: "common-login-fixture".into(),
        email: "fixture@example.com".into(),
        password_present: true,
        password_masked: "fix...word".into(),
    };
    let saved = SaveStationKeyWithDefaultsResult {
        station_key: fixture_station_key(),
        capabilities: fixture_capabilities(),
        message: "Fixture station key saved.".into(),
    };
    let created_remote = CreateRemoteStationKeyResult {
        remote_key: fixture_remote_key(),
        station_key: fixture_station_key(),
        full_key_once: Some("fixture-not-a-real-key-one-time".into()),
        message: "Fixture remote key created.".into(),
    };
    let created_local = CreateLocalStationKeyFromRemoteResult {
        remote_key: fixture_remote_key(),
        station_key: fixture_station_key(),
        message: "Fixture local key created.".into(),
    };
    let scan = RemoteKeyScanResult {
        station_id: "station-fixture".into(),
        capability: fixture_remote_capability(),
        keys: vec![fixture_remote_key()],
        synced_station_key_ids: vec!["station-key-fixture".into()],
        message: "Fixture scan completed.".into(),
    };

    let station_id = checked_input(
        serde_json::json!({"stationId": "station-fixture"}),
        StationIdInputDto::parse,
    );
    let remote_station_key = checked_input(
        serde_json::json!({
            "remoteKeyId": "remote-key-fixture",
            "stationId": "station-fixture"
        }),
        RemoteStationKeyInputDto::parse,
    );
    let create_station_key = checked_input(
        serde_json::json!({
            "stationId": "station-fixture",
            "name": "Fixture key",
            "apiKey": "fixture-not-a-real-api-key",
            "enabled": true,
            "priority": null,
            "maxConcurrency": 3,
            "loadFactor": null,
            "schedulable": true,
            "groupName": null,
            "tierLabel": null,
            "groupBindingId": null,
            "groupIdHash": null,
            "rateMultiplier": null,
            "manualRateMultiplier": null,
            "rateSource": null,
            "balanceScope": null,
            "note": null
        }),
        CreateStationKeyInputDto::parse,
    );
    let update_station_key = checked_input(
        serde_json::json!({
            "id": "station-key-fixture",
            "stationId": "station-fixture",
            "name": "Fixture key",
            "apiKey": null,
            "enabled": true,
            "priority": 0,
            "maxConcurrency": 3,
            "loadFactor": null,
            "schedulable": true,
            "groupName": null,
            "tierLabel": null,
            "groupBindingId": null,
            "groupIdHash": null,
            "rateMultiplier": null,
            "manualRateMultiplier": null,
            "rateSource": null,
            "balanceScope": null,
            "status": "unchecked",
            "note": null
        }),
        UpdateStationKeyInputDto::parse,
    );
    let save_with_defaults = checked_input(
        serde_json::json!({
            "mode": "create",
            "id": null,
            "stationId": "station-fixture",
            "name": "Fixture default key",
            "apiKey": "fixture-not-a-real-api-key",
            "enabled": true,
            "schedulable": true,
            "priority": null,
            "tierLabel": null,
            "balanceScope": null,
            "status": "unchecked",
            "note": null,
            "groupSelection": {"kind": "clear"},
            "capabilities": null
        }),
        SaveStationKeyWithDefaultsInputDto::parse,
    );
    let create_remote = checked_input(
        serde_json::json!({
            "stationId": "station-fixture",
            "name": "Fixture remote key",
            "groupBindingId": null,
            "groupIdHash": null,
            "groupName": null
        }),
        CreateRemoteStationKeyInputDto::parse,
    );
    let credentials_input = checked_input(
        serde_json::json!({
            "stationId": "station-fixture",
            "loginUsername": "fixture-user",
            "loginPassword": "fixture-not-a-real-password",
            "rememberPassword": false
        }),
        UpdateStationCredentialsInputDto::parse,
    );
    let common_login_profile_input = checked_input(
        serde_json::json!({
            "id": null,
            "email": "fixture@example.com",
            "password": "fixture-not-a-real-password"
        }),
        UpsertCommonLoginProfileInputDto::parse,
    );
    let common_login_profile_id = checked_input(
        serde_json::json!({"id": "common-login-fixture"}),
        CommonLoginProfileIdInputDto::parse,
    );
    let session_input = checked_input(
        serde_json::json!({
            "stationId": "station-fixture",
            "accessToken": "fixture-not-a-real-access-token",
            "refreshToken": null,
            "cookie": null,
            "newapiUserId": null,
            "tokenExpiresAt": null
        }),
        UpdateStationSessionInputDto::parse,
    );

    vec![
        serde_json::json!({"command": "list_station_keys", "input": station_id.clone(), "output": [station_key.clone()]}),
        serde_json::json!({"command": "create_station_key", "input": create_station_key, "output": station_key.clone()}),
        serde_json::json!({"command": "update_station_key", "input": update_station_key, "output": station_key.clone()}),
        serde_json::json!({"command": "save_station_key_with_defaults", "input": save_with_defaults, "output": saved}),
        serde_json::json!({"command": "update_station_key_group_binding", "input": checked_input(serde_json::json!({"stationKeyId": "station-key-fixture", "groupBindingId": "group-fixture"}), UpdateStationKeyGroupBindingInputDto::parse), "output": station_key.clone()}),
        serde_json::json!({"command": "delete_station_key", "input": checked_input(serde_json::json!({"id": "station-key-fixture"}), StationKeyIdInputDto::parse), "output": null}),
        serde_json::json!({"command": "reorder_station_keys", "input": checked_input(serde_json::json!({"stationId": "station-fixture", "keyIds": ["station-key-fixture"]}), ReorderStationKeysInputDto::parse), "output": [station_key]}),
        serde_json::json!({"command": "get_remote_key_capability", "input": station_id.clone(), "output": capability}),
        serde_json::json!({"command": "list_remote_station_keys", "input": station_id.clone(), "output": [remote_key.clone()]}),
        serde_json::json!({"command": "scan_remote_station_keys", "input": station_id.clone(), "output": scan}),
        serde_json::json!({"command": "create_remote_station_key", "input": create_remote, "output": created_remote}),
        serde_json::json!({"command": "create_local_station_key_from_remote", "input": remote_station_key.clone(), "output": created_local}),
        serde_json::json!({"command": "bind_remote_station_key", "input": checked_input(serde_json::json!({"remoteKeyId": "remote-key-fixture", "stationKeyId": "station-key-fixture"}), BindRemoteStationKeyInputDto::parse), "output": [remote_key.clone()]}),
        serde_json::json!({"command": "unbind_remote_station_key", "input": remote_station_key, "output": [remote_key]}),
        serde_json::json!({"command": "list_key_pool_items", "input": {}, "output": [key_pool_item.clone()]}),
        serde_json::json!({"command": "reorder_key_pool", "input": checked_input(serde_json::json!({"keyIds": ["station-key-fixture"]}), ReorderKeyPoolInputDto::parse), "output": [key_pool_item]}),
        serde_json::json!({"command": "get_station_credentials", "input": station_id.clone(), "output": credentials.clone()}),
        serde_json::json!({"command": "list_common_login_profiles", "input": {}, "output": [common_login_profile.clone()]}),
        serde_json::json!({"command": "upsert_common_login_profile", "input": common_login_profile_input, "output": common_login_profile}),
        serde_json::json!({"command": "delete_common_login_profile", "input": common_login_profile_id.clone(), "output": null}),
        serde_json::json!({"command": "get_common_login_profile_password", "input": common_login_profile_id, "output": "fixture-not-a-real-password"}),
        serde_json::json!({"command": "update_station_credentials", "input": credentials_input, "output": credentials.clone()}),
        serde_json::json!({"command": "update_station_session", "input": session_input, "output": credentials.clone()}),
        serde_json::json!({"command": "clear_station_credentials", "input": station_id, "output": credentials}),
    ]
}

#[cfg(test)]
fn checked_input<T>(
    value: Value,
    parse: impl FnOnce(Value) -> Result<T, crate::commands::error::CommandError>,
) -> Value {
    parse(value.clone()).expect("station-key fixture input must pass runtime validation");
    value
}

#[cfg(test)]
fn fixture_station_key() -> StationKey {
    StationKey {
        id: "station-key-fixture".into(),
        station_id: "station-fixture".into(),
        name: "Fixture key".into(),
        api_key_masked: "fixture-...redacted".into(),
        api_key_present: true,
        enabled: true,
        priority: 0,
        max_concurrency: 3,
        load_factor: None,
        schedulable: true,
        group_name: None,
        tier_label: None,
        group_binding_id: None,
        group_id_hash: None,
        rate_multiplier: None,
        manual_rate_multiplier: None,
        manual_rate_updated_at: None,
        rate_source: None,
        rate_collected_at: None,
        balance_scope: None,
        status: "unchecked".into(),
        last_checked_at: None,
        last_used_at: None,
        note: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}

#[cfg(test)]
fn fixture_key_pool_item() -> KeyPoolItem {
    KeyPoolItem {
        id: "station-key-fixture".into(),
        station_id: "station-fixture".into(),
        station_name: "Fixture Station".into(),
        station_type: "newapi".into(),
        station_api_base_url: "https://provider.invalid/v1".into(),
        station_endpoint_revision: 1,
        station_upstream_api_format: "auto".into(),
        name: "Fixture key".into(),
        api_key_masked: "fixture-...redacted".into(),
        api_key_present: true,
        enabled: true,
        priority: 0,
        max_concurrency: 3,
        load_factor: None,
        schedulable: true,
        group_name: None,
        tier_label: None,
        group_binding_id: None,
        group_id_hash: None,
        rate_multiplier: None,
        manual_rate_multiplier: None,
        manual_rate_updated_at: None,
        rate_source: None,
        rate_collected_at: None,
        balance_scope: None,
        status: "unchecked".into(),
        last_checked_at: None,
        last_used_at: None,
        note: None,
        capability_summary: vec!["chat_completions".into()],
        model_scope_summary: "all models".into(),
        only_use_as_backup: false,
        cooldown_until: None,
        success_rate: None,
        avg_latency_ms: None,
        consecutive_failures: 0,
        last_error_summary: None,
        endpoint_ping_status: "unchecked".into(),
        endpoint_ping_ms: None,
        endpoint_ping_checked_at: None,
        endpoint_ping_error: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}

#[cfg(test)]
fn fixture_remote_capability() -> RemoteKeyCapability {
    RemoteKeyCapability {
        station_id: "station-fixture".into(),
        station_type: "newapi".into(),
        can_list_remote_keys: true,
        can_create_remote_key: true,
        can_read_groups: true,
        requires_manual_session: false,
        unsupported_reason: None,
    }
}

#[cfg(test)]
fn fixture_remote_key() -> RemoteStationKey {
    RemoteStationKey {
        id: "remote-key-fixture".into(),
        station_id: "station-fixture".into(),
        remote_key_id_hash: Some("fixture-id-hash".into()),
        remote_key_name: Some("Fixture remote key".into()),
        api_key_masked: Some("fixture-...redacted".into()),
        api_key_fingerprint: Some("fixture-fingerprint".into()),
        group_id_hash: None,
        group_name: None,
        tier_label: None,
        rate_multiplier: None,
        rate_source: None,
        created_at: Some("2026-01-01T00:00:00Z".into()),
        last_used_at: None,
        raw_source: "fixture_sanitized".into(),
        match_status: RemoteKeyMatchStatus::Matched,
        matched_station_key_id: Some("station-key-fixture".into()),
        match_confidence: 1.0,
        collected_at: "2026-01-01T00:00:00Z".into(),
    }
}

#[cfg(test)]
fn fixture_credentials() -> StationCredentials {
    StationCredentials {
        station_id: "station-fixture".into(),
        login_username: Some("fixture-user".into()),
        password_present: true,
        access_token_present: true,
        refresh_token_present: false,
        cookie_present: false,
        remember_password: false,
        login_status: "saved".into(),
        login_error: None,
        last_login_at: None,
        session_status: "valid".into(),
        session_expires_at: None,
        newapi_user_id: None,
        token_expires_at: None,
        token_refreshed_at: None,
        session_source: "fixture".into(),
        updated_at: Some("2026-01-01T00:00:00Z".into()),
    }
}

#[cfg(test)]
fn fixture_capabilities() -> StationKeyCapabilities {
    StationKeyCapabilities {
        station_key_id: "station-key-fixture".into(),
        supports_chat_completions: true,
        supports_responses: true,
        supports_embeddings: true,
        supports_stream: true,
        supports_tools: true,
        supports_vision: true,
        supports_reasoning: true,
        model_allowlist: vec![],
        model_blocklist: vec![],
        preferred_models: vec![],
        only_use_as_backup: false,
        routing_tags: vec!["fixture".into()],
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::Path, sync::Arc};

    use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, Connection, Row};

    use crate::{
        application::{
            clock::SystemClock, credentials::CredentialService, ids::UuidV7Generator,
            stations::StationService,
        },
        commands::error::CommandErrorCode,
        ipc::dto::stations::CreateStationInputDto,
        persistence::runtime::PersistenceRuntime,
        services::secrets::vault::DataKeyVault,
    };

    #[test]
    fn station_key_inputs_reject_unknown_fields_duplicates_and_oversized_secrets() {
        let unknown = CreateStationKeyInputDto::parse(serde_json::json!({
            "stationId":"station-1","name":"Key","apiKey":"fake","enabled":true,
            "groupName":null,"tierLabel":null,"note":null,"unexpected":true
        }))
        .expect_err("unknown field");
        assert_eq!(unknown.code, CommandErrorCode::InvalidInput);

        assert!(
            ReorderKeyPoolInputDto::parse(serde_json::json!({"keyIds":["key-1","key-1"]})).is_err()
        );
        let oversized = "x".repeat(MAX_SECRET_BYTES + 1);
        let error = UpdateStationCredentialsInputDto::parse(serde_json::json!({
            "stationId":"station-1","loginUsername":null,"loginPassword":oversized,
            "rememberPassword":false
        }))
        .expect_err("oversized secret");
        assert_eq!(error.code, CommandErrorCode::InvalidInput);
        let public_error = serde_json::to_string(&error).unwrap();
        assert!(!public_error.contains(&"x".repeat(100)));
        for sensitive_name in ["password", "token", "cookie", "apiKey", "secret"] {
            assert!(!public_error
                .to_ascii_lowercase()
                .contains(&sensitive_name.to_ascii_lowercase()));
        }
    }

    #[test]
    fn save_with_defaults_rejects_unknown_status_and_cross_variant_group_fields() {
        let base = serde_json::json!({
            "mode": "update",
            "id": "key-1",
            "stationId": "station-1",
            "name": "Key",
            "apiKey": null,
            "enabled": true,
            "groupSelection": { "kind": "keep" }
        });

        let mut invalid_status = base.clone();
        invalid_status["status"] = serde_json::json!("mystery");
        let status_error =
            SaveStationKeyWithDefaultsInputDto::parse(invalid_status).expect_err("unknown status");
        assert_eq!(status_error.code, CommandErrorCode::InvalidInput);

        let mut invalid_keep = base.clone();
        invalid_keep["groupSelection"] = serde_json::json!({
            "kind": "keep",
            "groupBindingId": "binding-1"
        });
        let keep_error = SaveStationKeyWithDefaultsInputDto::parse(invalid_keep)
            .expect_err("keep cannot carry set-only fields");
        assert_eq!(keep_error.code, CommandErrorCode::InvalidInput);

        let mut invalid_set = base;
        invalid_set["groupSelection"] = serde_json::json!({ "kind": "set" });
        let set_error = SaveStationKeyWithDefaultsInputDto::parse(invalid_set)
            .expect_err("set requires a binding id");
        assert_eq!(set_error.code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn station_key_dto_lifecycle_persists_only_encrypted_secret_material() {
        let root = std::env::temp_dir().join(format!(
            "relay-pool-station-key-dto-lifecycle-{}",
            uuid::Uuid::now_v7()
        ));
        std::fs::create_dir_all(&root).expect("create lifecycle fixture directory");
        let database_path = root.join("relay-pool-v2.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&database_path)
            .await
            .expect("initialize lifecycle database");
        let clock = Arc::new(SystemClock);
        let ids = Arc::new(UuidV7Generator);
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let credentials = CredentialService::new(
            runtime.handle(),
            Arc::new(DataKeyVault::new([23; 32])),
            clock,
            ids,
        );

        let invalid = CreateStationKeyInputDto::parse(serde_json::json!({
            "stationId": "station-never-created",
            "name": "Rejected key",
            "apiKey": "sk-rejected-plaintext-canary",
            "enabled": true,
            "unexpected": true
        }))
        .expect_err("unknown fields must fail before the application service is called");
        assert_eq!(invalid.code, CommandErrorCode::InvalidInput);
        assert_eq!(table_count(&database_path, "station_keys").await, 0);
        assert_eq!(table_count(&database_path, "secrets").await, 0);

        let station = stations
            .create(
                CreateStationInputDto::parse(serde_json::json!({
                    "name": "Headless DTO Station",
                    "stationType": "openai-compatible",
                    "websiteUrl": "https://station.invalid",
                    "apiBaseUrl": "https://station.invalid/v1",
                    "apiKey": "",
                    "collectorProxyMode": "inherit",
                    "collectorProxyUrl": null,
                    "enabled": true,
                    "creditPerCny": 1.0,
                    "lowBalanceThresholdCny": null,
                    "collectionIntervalMinutes": 5,
                    "note": null
                }))
                .expect("valid station DTO")
                .into_domain()
                .expect("station domain input"),
            )
            .await
            .expect("create station through application service");

        let create_secret = "sk-create-plaintext-canary";
        let created = credentials
            .create_station_key(
                CreateStationKeyInputDto::parse(serde_json::json!({
                    "stationId": station.id,
                    "name": "Primary",
                    "apiKey": create_secret,
                    "enabled": true,
                    "priority": 0,
                    "maxConcurrency": 3,
                    "loadFactor": null,
                    "schedulable": true,
                    "groupName": null,
                    "tierLabel": null,
                    "groupBindingId": null,
                    "groupIdHash": null,
                    "rateMultiplier": null,
                    "manualRateMultiplier": null,
                    "rateSource": null,
                    "balanceScope": null,
                    "note": null
                }))
                .expect("valid create station-key DTO"),
            )
            .await
            .expect("create station key through application service");

        let created_json = serde_json::to_value(&created).expect("serialize created DTO");
        assert_eq!(created_json["apiKeyPresent"], true);
        assert_eq!(created_json["apiKeyMasked"], "sk-...nary");
        assert!(created_json.get("apiKey").is_none());
        assert!(!created_json.to_string().contains(create_secret));
        assert_secret_storage(&database_path, &created.id, create_secret).await;

        let listed = credentials
            .list_station_keys(
                StationIdInputDto::parse(serde_json::json!({"stationId": created.station_id}))
                    .expect("valid list DTO")
                    .station_id,
            )
            .await
            .expect("list station keys through application service");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);

        let update_secret = "sk-update-plaintext-canary";
        let updated = credentials
            .update_station_key(
                UpdateStationKeyInputDto::parse(serde_json::json!({
                    "id": created.id,
                    "stationId": created.station_id,
                    "name": "Primary updated",
                    "apiKey": update_secret,
                    "enabled": true,
                    "priority": 0,
                    "maxConcurrency": 4,
                    "loadFactor": 2,
                    "schedulable": true,
                    "groupName": null,
                    "tierLabel": "standard",
                    "groupBindingId": null,
                    "groupIdHash": null,
                    "rateMultiplier": 1.25,
                    "manualRateMultiplier": null,
                    "rateSource": "manual",
                    "balanceScope": "key",
                    "status": "healthy",
                    "note": "updated through DTO"
                }))
                .expect("valid update station-key DTO"),
            )
            .await
            .expect("update station key through application service");
        assert_eq!(updated.name, "Primary updated");
        assert_eq!(updated.max_concurrency, 4);
        assert_eq!(updated.api_key_masked, "sk-...nary");
        assert_secret_storage(&database_path, &updated.id, update_secret).await;
        assert!(!database_contains(&database_path, create_secret).await);

        credentials
            .delete_station_key(
                StationKeyIdInputDto::parse(serde_json::json!({"id": updated.id}))
                    .expect("valid delete DTO")
                    .id,
            )
            .await
            .expect("delete station key through application service");
        stations
            .delete(station.id)
            .await
            .expect("delete station through application service");

        assert_eq!(table_count(&database_path, "stations").await, 0);
        assert_eq!(table_count(&database_path, "station_keys").await, 0);
        assert_eq!(table_count(&database_path, "secrets").await, 0);

        runtime.close().await.expect("close lifecycle database");
        std::fs::remove_dir_all(root).expect("remove lifecycle fixture directory");
    }

    async fn assert_secret_storage(database_path: &Path, station_key_id: &str, plaintext: &str) {
        let mut connection = connect(database_path).await;
        let row = sqlx::query(
            r#"
            SELECT k.api_key, k.api_key_secret_id, s.ciphertext, s.nonce
            FROM station_keys k
            JOIN secrets s ON s.id = k.api_key_secret_id
            WHERE k.id = ?1
            "#,
        )
        .bind(station_key_id)
        .fetch_one(&mut connection)
        .await
        .expect("read encrypted station-key secret");
        let legacy_plaintext: String = row.get("api_key");
        let secret_id: String = row.get("api_key_secret_id");
        let ciphertext: Vec<u8> = row.get("ciphertext");
        let nonce: Vec<u8> = row.get("nonce");
        connection.close().await.expect("close secret inspection");

        assert!(legacy_plaintext.is_empty());
        assert!(!secret_id.is_empty());
        assert!(!ciphertext.is_empty());
        assert!(!nonce.is_empty());
        assert!(!contains_bytes(&ciphertext, plaintext.as_bytes()));
        assert!(!contains_bytes(&nonce, plaintext.as_bytes()));
        assert!(!database_contains(database_path, plaintext).await);
    }

    async fn table_count(database_path: &Path, table: &str) -> i64 {
        let mut connection = connect(database_path).await;
        let row = sqlx::query(&format!("SELECT COUNT(*) AS count FROM {table}"))
            .fetch_one(&mut connection)
            .await
            .expect("count lifecycle rows");
        connection.close().await.expect("close lifecycle count");
        row.get("count")
    }

    async fn database_contains(database_path: &Path, plaintext: &str) -> bool {
        let bytes = std::fs::read(database_path).expect("read lifecycle database");
        contains_bytes(&bytes, plaintext.as_bytes())
    }

    async fn connect(database_path: &Path) -> sqlx::SqliteConnection {
        SqliteConnectOptions::new()
            .filename(database_path)
            .create_if_missing(false)
            .connect()
            .await
            .expect("connect lifecycle database")
    }

    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        !needle.is_empty()
            && haystack
                .windows(needle.len())
                .any(|window| window == needle)
    }
}
