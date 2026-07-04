use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StationKeyDeserializePayload {
    id: String,
    station_id: String,
    name: String,
    api_key_masked: String,
    api_key_present: bool,
    enabled: bool,
    priority: i64,
    group_name: Option<String>,
    tier_label: Option<String>,
    group_binding_id: Option<String>,
    group_id_hash: Option<String>,
    rate_multiplier: Option<f64>,
    rate_source: Option<String>,
    rate_collected_at: Option<String>,
    balance_scope: Option<String>,
    status: String,
    last_checked_at: Option<String>,
    last_used_at: Option<String>,
    note: Option<String>,
    created_at: String,
    updated_at: String,
}

impl<'de> Deserialize<'de> for crate::models::station_keys::StationKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let payload = StationKeyDeserializePayload::deserialize(deserializer)?;
        Ok(Self {
            id: payload.id,
            station_id: payload.station_id,
            name: payload.name,
            api_key_masked: payload.api_key_masked,
            api_key_present: payload.api_key_present,
            enabled: payload.enabled,
            priority: payload.priority,
            group_name: payload.group_name,
            tier_label: payload.tier_label,
            group_binding_id: payload.group_binding_id,
            group_id_hash: payload.group_id_hash,
            rate_multiplier: payload.rate_multiplier,
            rate_source: payload.rate_source,
            rate_collected_at: payload.rate_collected_at,
            balance_scope: payload.balance_scope,
            status: payload.status,
            last_checked_at: payload.last_checked_at,
            last_used_at: payload.last_used_at,
            note: payload.note,
            created_at: payload.created_at,
            updated_at: payload.updated_at,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteKeyCapability {
    pub station_id: String,
    pub station_type: String,
    pub can_list_remote_keys: bool,
    pub can_create_remote_key: bool,
    pub can_read_groups: bool,
    pub requires_manual_session: bool,
    pub unsupported_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStationKey {
    pub id: String,
    pub station_id: String,
    pub remote_key_id_hash: Option<String>,
    pub remote_key_name: Option<String>,
    pub api_key_masked: Option<String>,
    pub api_key_fingerprint: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_name: Option<String>,
    pub tier_label: Option<String>,
    pub rate_multiplier: Option<f64>,
    pub rate_source: Option<String>,
    pub created_at: Option<String>,
    pub last_used_at: Option<String>,
    pub raw_source: String,
    pub match_status: RemoteKeyMatchStatus,
    pub matched_station_key_id: Option<String>,
    pub match_confidence: f64,
    pub collected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteKeyMatchStatus {
    Matched,
    Possible,
    Unbound,
}

impl RemoteKeyMatchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RemoteKeyMatchStatus::Matched => "matched",
            RemoteKeyMatchStatus::Possible => "possible",
            RemoteKeyMatchStatus::Unbound => "unbound",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "matched" => RemoteKeyMatchStatus::Matched,
            "possible" => RemoteKeyMatchStatus::Possible,
            _ => RemoteKeyMatchStatus::Unbound,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteKeyScanResult {
    pub station_id: String,
    pub capability: RemoteKeyCapability,
    pub keys: Vec<RemoteStationKey>,
    pub synced_station_key_ids: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRemoteStationKeyInput {
    pub station_id: String,
    pub name: String,
    pub group_id_hash: Option<String>,
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRemoteStationKeyResult {
    pub remote_key: RemoteStationKey,
    pub station_key: crate::models::station_keys::StationKey,
    pub full_key_once: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindRemoteStationKeyInput {
    pub remote_key_id: String,
    pub station_key_id: String,
}
