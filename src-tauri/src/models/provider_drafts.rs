use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDraftPayload {
    pub name: String,
    pub station_type: String,
    pub website_url: String,
    pub api_base_url: String,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub enabled: bool,
    pub credit_per_cny: f64,
    pub low_balance_threshold_cny: Option<f64>,
    pub collection_interval_minutes: u16,
    pub note: Option<String>,
    pub login_username: Option<String>,
    pub remember_password: bool,
    pub groups: Vec<ProviderDraftGroup>,
    pub keys: Vec<ProviderDraftKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDraftGroup {
    pub client_id: String,
    pub group_key_hash: String,
    pub group_id_hash: Option<String>,
    pub group_name: String,
    pub rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub group_category_override: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDraftKey {
    pub client_id: String,
    pub name: String,
    pub enabled: bool,
    pub group_client_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_name: Option<String>,
    pub rate_multiplier: Option<f64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDraft {
    pub id: String,
    pub base_station_id: Option<String>,
    pub revision: i64,
    pub state: String,
    pub payload_schema_version: i64,
    pub payload: ProviderDraftPayload,
    pub station_api_key_present: bool,
    pub login_password_present: bool,
    pub key_api_key_client_ids: Vec<String>,
    pub committed_station_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone)]
pub struct CreateProviderDraftInput {
    pub base_station_id: Option<String>,
    pub payload: ProviderDraftPayload,
}

#[derive(Debug, Clone)]
pub struct PatchProviderDraftInput {
    pub draft_id: String,
    pub expected_revision: i64,
    pub payload: ProviderDraftPayload,
    pub station_api_key: Option<String>,
    pub login_password: Option<String>,
    pub key_api_keys: Vec<ProviderDraftKeySecretInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDraftKeySecretInput {
    pub client_id: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDraftPreviewGroup {
    pub group_key_hash: String,
    pub group_id_hash: Option<String>,
    pub group_name: String,
    pub rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub source: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDraftPreview {
    pub draft_id: String,
    pub kind: String,
    pub runtime_fingerprint: String,
    pub status: String,
    pub groups: Vec<ProviderDraftPreviewGroup>,
    pub models: Vec<String>,
    pub balance: Option<f64>,
    pub summary_json: serde_json::Value,
    pub collected_at: String,
}

#[derive(Debug, Clone)]
pub struct CommitProviderDraftInput {
    pub draft_id: String,
    pub expected_revision: i64,
    pub commit_key: String,
}
