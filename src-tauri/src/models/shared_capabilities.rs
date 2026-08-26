use serde::{Deserialize, Serialize};

use crate::models::{
    group_facts::{GroupRateRecord, StationGroupBinding},
    routing::{StationKeyCapabilities, UpdateStationKeyCapabilitiesInput},
    station_keys::StationKey,
    stations::Station,
};

pub type StationKeyStatus = String;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SaveStationKeyMode {
    Create,
    Update,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StationKeyGroupSelectionKind {
    Keep,
    Clear,
    Set,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationKeyGroupSelection {
    pub kind: StationKeyGroupSelectionKind,
    pub group_binding_id: Option<String>,
    #[allow(
        dead_code,
        reason = "retained for the existing station-key input payload contract"
    )]
    pub group_id_hash: Option<String>,
    #[allow(
        dead_code,
        reason = "retained for the existing station-key input payload contract"
    )]
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStationKeyWithDefaultsInput {
    pub mode: SaveStationKeyMode,
    pub id: Option<String>,
    pub station_id: String,
    pub name: String,
    pub api_key: Option<String>,
    pub enabled: bool,
    pub schedulable: Option<bool>,
    pub priority: Option<i64>,
    pub tier_label: Option<String>,
    pub balance_scope: Option<String>,
    pub status: Option<StationKeyStatus>,
    pub note: Option<String>,
    pub group_selection: StationKeyGroupSelection,
    pub capabilities: Option<UpdateStationKeyCapabilitiesInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStationKeyWithDefaultsResult {
    pub station_key: StationKey,
    pub capabilities: StationKeyCapabilities,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationGroupOption {
    pub value: String,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_name: String,
    pub rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub group_category_override: Option<String>,
    pub effective_group_category: String,
    pub rate_source: Option<String>,
    pub selectable_for_remote_key: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingComparisonWorkspace {
    pub stations: Vec<Station>,
    pub station_keys: Vec<StationKey>,
    pub group_bindings: Vec<StationGroupBinding>,
    pub group_rates: Vec<GroupRateRecord>,
    pub developer_mode_enabled: bool,
}
