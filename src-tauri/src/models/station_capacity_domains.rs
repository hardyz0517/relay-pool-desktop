use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationCapacityDomain {
    pub station_id: String,
    pub provider_family: String,
    pub deployment_identity: Option<String>,
    pub region_identity: Option<String>,
    pub revision: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertStationCapacityDomainInput {
    pub station_id: String,
    pub expected_revision: i64,
    pub provider_family: String,
    pub deployment_identity: Option<String>,
    pub region_identity: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearStationCapacityDomainInput {
    pub station_id: String,
    pub expected_revision: i64,
}
