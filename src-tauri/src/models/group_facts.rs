use serde::{Deserialize, Serialize};

pub const BINDING_KIND_STATION_GROUP: &str = "station_group";
pub const BINDING_KIND_KEY_BINDING: &str = "key_binding";

pub const BINDING_STATUS_AVAILABLE: &str = "available";
pub const BINDING_STATUS_BOUND: &str = "bound";
pub const BINDING_STATUS_MISSING: &str = "missing";
pub const BINDING_STATUS_DISABLED: &str = "disabled";
pub const BINDING_STATUS_MANUAL_LEGACY: &str = "manual_legacy";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationGroupBinding {
    pub id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub binding_kind: String,
    pub parent_group_binding_id: Option<String>,
    pub group_key_hash: String,
    pub group_id_hash: Option<String>,
    pub group_name: String,
    pub binding_status: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub group_category_override: Option<String>,
    pub rate_source: Option<String>,
    pub confidence: f64,
    pub last_seen_at: Option<String>,
    pub last_checked_at: Option<String>,
    pub last_rate_changed_at: Option<String>,
    pub raw_json_redacted: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRateRecord {
    pub id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub group_binding_id: Option<String>,
    pub binding_kind: String,
    pub group_key_hash: String,
    pub group_name: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub source: String,
    pub confidence: f64,
    pub raw_json_redacted: Option<serde_json::Value>,
    pub checked_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertStationGroupBindingInput {
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub binding_kind: String,
    pub parent_group_binding_id: Option<String>,
    pub group_key_hash: String,
    pub group_id_hash: Option<String>,
    pub group_name: String,
    pub binding_status: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub group_category_override: Option<String>,
    pub rate_source: Option<String>,
    pub confidence: f64,
    pub last_seen_at: Option<String>,
    pub raw_json_redacted: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertGroupRateRecordInput {
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub group_binding_id: Option<String>,
    pub binding_kind: String,
    pub group_key_hash: String,
    pub group_name: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub source: String,
    pub confidence: f64,
    pub raw_json_redacted: Option<serde_json::Value>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStationKeyGroupBindingInput {
    pub station_key_id: String,
    pub group_binding_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_group_binding_serializes_camel_case() {
        let binding = StationGroupBinding {
            id: "gb-1".to_string(),
            station_id: "station-1".to_string(),
            station_key_id: None,
            binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
            parent_group_binding_id: None,
            group_key_hash: "hash-1".to_string(),
            group_id_hash: Some("gid-hash".to_string()),
            group_name: "default".to_string(),
            binding_status: BINDING_STATUS_AVAILABLE.to_string(),
            default_rate_multiplier: Some(1.0),
            user_rate_multiplier: Some(0.8),
            effective_rate_multiplier: Some(0.8),
            inferred_group_category: Some("gpt".to_string()),
            group_category_override: None,
            rate_source: Some("groups_api".to_string()),
            confidence: 0.95,
            last_seen_at: Some("1000".to_string()),
            last_checked_at: Some("1000".to_string()),
            last_rate_changed_at: None,
            raw_json_redacted: None,
            created_at: "1000".to_string(),
            updated_at: "1000".to_string(),
        };
        let value = serde_json::to_value(binding).expect("json");

        assert_eq!(value["stationId"], "station-1");
        assert_eq!(value["bindingKind"], "station_group");
        assert_eq!(value["groupKeyHash"], "hash-1");
        assert_eq!(value["effectiveRateMultiplier"], 0.8);
    }
}
