use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    commands::error::{CommandError, CommandErrorCode, PublicErrorDetails, PublicFieldError},
    models::provider_drafts::{
        CommitProviderDraftInput, CreateProviderDraftInput, PatchProviderDraftInput, ProviderDraft,
        ProviderDraftKeySecretInput, ProviderDraftPayload, ProviderDraftPreview,
    },
    services::collectors::output::CollectorTask,
};

pub type ProviderDraftDto = ProviderDraft;
pub type ProviderDraftPreviewDto = ProviderDraftPreview;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateProviderDraftInputDto {
    pub base_station_id: Option<String>,
    pub payload: ProviderDraftPayload,
}

impl CreateProviderDraftInputDto {
    pub fn parse(value: Value) -> Result<CreateProviderDraftInput, CommandError> {
        let input: Self = parse_value(value)?;
        Ok(CreateProviderDraftInput {
            base_station_id: normalize_optional(input.base_station_id),
            payload: input.payload,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PatchProviderDraftInputDto {
    pub draft_id: String,
    pub expected_revision: i64,
    pub payload: ProviderDraftPayload,
    pub station_api_key: Option<String>,
    pub login_password: Option<String>,
    pub key_api_keys: Vec<ProviderDraftKeySecretInput>,
}

impl PatchProviderDraftInputDto {
    pub fn parse(value: Value) -> Result<PatchProviderDraftInput, CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("draftId", &input.draft_id)?;
        if input.expected_revision < 1 {
            return Err(invalid(
                "expectedRevision",
                "invalid_revision",
                "Expected revision must be positive.",
            ));
        }
        Ok(PatchProviderDraftInput {
            draft_id: input.draft_id,
            expected_revision: input.expected_revision,
            payload: input.payload,
            station_api_key: input.station_api_key,
            login_password: input.login_password,
            key_api_keys: input.key_api_keys,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderDraftIdInputDto {
    pub draft_id: String,
}

impl ProviderDraftIdInputDto {
    pub fn parse(value: Value) -> Result<Self, CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("draftId", &input.draft_id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectProviderDraftPreviewInputDto {
    pub draft_id: String,
    pub task_type: String,
}

impl CollectProviderDraftPreviewInputDto {
    pub fn parse(value: Value) -> Result<(String, CollectorTask), CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("draftId", &input.draft_id)?;
        let task = match input.task_type.as_str() {
            "detect" => CollectorTask::Detect,
            "balance" => CollectorTask::Balance,
            "groups" => CollectorTask::Groups,
            "full" => CollectorTask::Full,
            _ => {
                return Err(invalid(
                    "taskType",
                    "unsupported_value",
                    "Unsupported draft collection task.",
                ))
            }
        };
        Ok((input.draft_id, task))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitProviderDraftInputDto {
    pub draft_id: String,
    pub expected_revision: i64,
    pub commit_key: String,
}

impl CommitProviderDraftInputDto {
    pub fn parse(value: Value) -> Result<CommitProviderDraftInput, CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("draftId", &input.draft_id)?;
        validate_id("commitKey", &input.commit_key)?;
        if input.expected_revision < 1 {
            return Err(invalid(
                "expectedRevision",
                "invalid_revision",
                "Expected revision must be positive.",
            ));
        }
        Ok(CommitProviderDraftInput {
            draft_id: input.draft_id,
            expected_revision: input.expected_revision,
            commit_key: input.commit_key,
        })
    }
}

fn parse_value<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, CommandError> {
    serde_json::from_value(value).map_err(|_| {
        invalid(
            "input",
            "invalid_shape",
            "The provider draft input is invalid.",
        )
    })
}

fn validate_id(field: &'static str, value: &str) -> Result<(), CommandError> {
    if value.trim().is_empty() || value.len() > 256 {
        Err(invalid(field, "invalid_id", "The identifier is invalid."))
    } else {
        Ok(())
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn invalid(field: &'static str, code: &'static str, message: &'static str) -> CommandError {
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
    .expect("provider draft validation messages are bounded")
}

pub const PROVIDER_DRAFTS_TYPE: super::TypeDescriptor = super::TypeDescriptor {
    name: "ProviderDrafts",
    typescript: r#"
export type ProviderDraftGroupDto = {
  clientId: string; groupKeyHash: string; groupIdHash: string | null; groupName: string;
  rateMultiplier: number | null; inferredGroupCategory: string | null;
  groupCategoryOverride: string | null; source: string;
};
export type ProviderDraftKeyDto = {
  clientId: string; name: string; enabled: boolean; groupClientId: string | null;
  groupIdHash: string | null; groupName: string | null; rateMultiplier: number | null; note: string | null;
};
export type ProviderDraftPayloadDto = {
  name: string; stationType: string; websiteUrl: string; apiBaseUrl: string;
  collectorProxyMode: string; collectorProxyUrl: string | null; enabled: boolean;
  creditPerCny: number; lowBalanceThresholdCny: number | null; collectionIntervalMinutes: number;
  note: string | null; loginUsername: string | null; rememberPassword: boolean;
  groups: ProviderDraftGroupDto[]; keys: ProviderDraftKeyDto[];
};
export type ProviderDraftDto = {
  id: string; baseStationId: string | null; revision: number; state: string; payloadSchemaVersion: number;
  payload: ProviderDraftPayloadDto; stationApiKeyPresent: boolean; loginPasswordPresent: boolean;
  keyApiKeyClientIds: string[]; committedStationId: string | null;
  createdAt: string; updatedAt: string; expiresAt: string;
};
export type CreateProviderDraftInputDto = { baseStationId: string | null; payload: ProviderDraftPayloadDto };
export type ProviderDraftKeySecretInputDto = { clientId: string; apiKey: string };
export type PatchProviderDraftInputDto = {
  draftId: string; expectedRevision: number; payload: ProviderDraftPayloadDto;
  stationApiKey: string | null; loginPassword: string | null; keyApiKeys: ProviderDraftKeySecretInputDto[];
};
export type ProviderDraftIdInputDto = { draftId: string };
export type CollectProviderDraftPreviewInputDto = {
  draftId: string; taskType: "detect" | "balance" | "groups" | "models" | "full";
};
export type ProviderDraftPreviewGroupDto = {
  groupKeyHash: string; groupIdHash: string | null; groupName: string; rateMultiplier: number | null;
  inferredGroupCategory: string | null; source: string; confidence: number;
};
export type ProviderDraftPreviewDto = {
  draftId: string; kind: string; runtimeFingerprint: string; status: string;
  groups: ProviderDraftPreviewGroupDto[]; models: string[]; balance: number | null;
  summaryJson: Record<string, unknown>; collectedAt: string;
};
export type CommitProviderDraftInputDto = {
  draftId: string; expectedRevision: number; commitKey: string;
};
"#,
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let payload = serde_json::json!({
        "name": "Draft Provider", "stationType": "newapi",
        "websiteUrl": "https://draft.example.test", "apiBaseUrl": "https://draft.example.test/v1",
        "collectorProxyMode": "inherit", "collectorProxyUrl": null, "enabled": true,
        "creditPerCny": 1.0, "lowBalanceThresholdCny": null, "collectionIntervalMinutes": 5,
        "note": null, "loginUsername": null, "rememberPassword": false, "groups": [], "keys": []
    });
    let output = serde_json::json!({
        "id": "draft-fixture", "baseStationId": null, "revision": 1, "state": "active",
        "payloadSchemaVersion": 1, "payload": payload.clone(), "stationApiKeyPresent": false,
        "loginPasswordPresent": false, "keyApiKeyClientIds": [], "committedStationId": null,
        "createdAt": "1000", "updatedAt": "1000", "expiresAt": "2000"
    });
    let station = serde_json::json!({
        "id":"station-fixture","name":"Draft Provider","stationType":"newapi",
        "websiteUrl":"https://draft.example.test","apiBaseUrl":"https://draft.example.test/v1",
        "endpointRevision":1,"collectorProxyMode":"inherit","collectorProxyUrl":null,
        "apiKeyMasked":"未设置","apiKeyPresent":false,"keyCount":0,"enabled":true,"priority":0,
        "creditPerCny":1.0,"balanceRaw":null,"balanceCny":null,"lowBalanceThresholdCny":null,
        "collectionIntervalMinutes":5,"status":"unchecked","latencyMs":null,"lastCheckedAt":null,
        "lastPricingFetchedAt":null,"note":null,"createdAt":"1000","updatedAt":"1000"
    });
    vec![
        serde_json::json!({"command":"create_or_resume_provider_draft","input":{"baseStationId":null,"payload":payload.clone()},"output":output.clone()}),
        serde_json::json!({"command":"get_provider_draft","input":{"draftId":"draft-fixture"},"output":output.clone()}),
        serde_json::json!({"command":"patch_provider_draft","input":{"draftId":"draft-fixture","expectedRevision":1,"payload":payload,"stationApiKey":null,"loginPassword":null,"keyApiKeys":[]},"output":output}),
        serde_json::json!({"command":"discard_provider_draft","input":{"draftId":"draft-fixture"},"output":null}),
        serde_json::json!({"command":"collect_provider_draft_preview","input":{"draftId":"draft-fixture","taskType":"groups"},"output":{"draftId":"draft-fixture","kind":"groups","runtimeFingerprint":"hash","status":"success","groups":[],"models":[],"balance":null,"summaryJson":{},"collectedAt":"1000"}}),
        serde_json::json!({"command":"scan_provider_draft_remote_keys","input":{"draftId":"draft-fixture"},"output":{"stationId":"draft-fixture","capability":{"stationId":"draft-fixture","stationType":"newapi","canListRemoteKeys":true,"canCreateRemoteKey":false,"canReadGroups":true,"requiresManualSession":true,"unsupportedReason":null},"keys":[],"syncedStationKeyIds":[],"message":"read-only"}}),
        serde_json::json!({"command":"commit_provider_draft","input":{"draftId":"draft-fixture","expectedRevision":1,"commitKey":"commit-fixture"},"output":station}),
        serde_json::json!({"command":"start_provider_draft_authorization","input":{"draftId":"draft-fixture"},"output":{"stationId":"draft-fixture","status":"capturing","captureCount":0,"recognizedFieldCount":0,"pendingConfirmationCount":0,"webAuthorizationCandidate":false,"lastError":null}}),
        serde_json::json!({"command":"finish_provider_draft_authorization_session","input":{"draftId":"draft-fixture"},"output":{"draftId":"draft-fixture","kind":"capture","runtimeFingerprint":"hash","status":"success","groups":[],"models":[],"balance":null,"summaryJson":{},"collectedAt":"1000"}}),
    ]
}
