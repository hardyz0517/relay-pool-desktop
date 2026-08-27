use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationRedemptionResult {
    pub provider: String,
    pub success: bool,
    pub message: String,
    pub credited_detail: Option<String>,
}
