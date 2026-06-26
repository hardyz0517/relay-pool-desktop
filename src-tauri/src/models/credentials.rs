use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationCredentials {
    pub station_id: String,
    pub login_username: Option<String>,
    pub password_present: bool,
    pub remember_password: bool,
    pub login_status: String,
    pub login_error: Option<String>,
    pub last_login_at: Option<String>,
    pub session_status: String,
    pub session_expires_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStationCredentialsInput {
    pub station_id: String,
    pub login_username: Option<String>,
    pub login_password: Option<String>,
    pub remember_password: bool,
}
