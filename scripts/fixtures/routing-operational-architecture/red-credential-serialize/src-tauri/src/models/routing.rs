#[derive(Debug, serde::Serialize)]
pub struct RuntimeRoutingCandidate {
    pub station_key_id: String,
    pub api_key: Option<String>,
}
