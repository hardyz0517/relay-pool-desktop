use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationCredentials {
    pub station_id: String,
    pub login_username: Option<String>,
    pub password_present: bool,
    pub access_token_present: bool,
    pub refresh_token_present: bool,
    pub cookie_present: bool,
    pub remember_password: bool,
    pub login_status: String,
    pub login_error: Option<String>,
    pub last_login_at: Option<String>,
    pub session_status: String,
    pub session_expires_at: Option<String>,
    pub newapi_user_id: Option<String>,
    pub token_expires_at: Option<String>,
    pub token_refreshed_at: Option<String>,
    pub session_source: String,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStationSessionInput {
    pub station_id: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub cookie: Option<String>,
    pub newapi_user_id: Option<String>,
    pub token_expires_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PersistStationSessionInput {
    pub station_id: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub cookie: Option<String>,
    pub newapi_user_id: Option<String>,
    pub token_expires_at: Option<String>,
    pub session_expires_at: Option<String>,
    pub session_source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StationSessionCredentialKind {
    AccessToken,
    #[allow(
        dead_code,
        reason = "supported by the credential clearing port for refreshable sessions"
    )]
    RefreshToken,
    Cookie,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionResolveStatus {
    Ready,
    ManualRequired,
}

#[derive(Debug, Clone)]
pub struct ResolvedSession {
    #[allow(
        dead_code,
        reason = "retained by the collector session resolution port contract"
    )]
    pub status: SessionResolveStatus,
    pub access_token: Option<String>,
    #[allow(
        dead_code,
        reason = "retained by the collector session resolution port contract"
    )]
    pub refresh_token: Option<String>,
    pub cookie: Option<String>,
    pub newapi_user_id: Option<String>,
    pub message: Option<String>,
}

impl ResolvedSession {
    pub fn manual_required(message: impl Into<String>) -> Self {
        Self {
            status: SessionResolveStatus::ManualRequired,
            access_token: None,
            refresh_token: None,
            cookie: None,
            newapi_user_id: None,
            message: Some(message.into()),
        }
    }
}

pub fn token_is_fresh(expires_at: Option<&str>, now_ms: i64) -> bool {
    expires_at
        .and_then(|value| value.parse::<i64>().ok())
        .map(|expires| expires > now_ms + 60_000)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::token_is_fresh;

    #[test]
    fn token_freshness_uses_sixty_second_refresh_window() {
        assert!(token_is_fresh(Some("200000"), 100000));
        assert!(!token_is_fresh(Some("150000"), 100000));
        assert!(!token_is_fresh(None, 100000));
    }
}
