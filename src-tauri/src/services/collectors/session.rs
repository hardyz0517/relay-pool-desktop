#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionResolveStatus {
    Ready,
    ManualRequired,
    Failed,
}

#[derive(Debug, Clone)]
pub struct ResolvedSession {
    pub status: SessionResolveStatus,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub cookie: Option<String>,
    pub newapi_user_id: Option<String>,
    pub message: Option<String>,
}

pub fn token_is_fresh(expires_at: Option<&str>, now_ms: i64) -> bool {
    expires_at
        .and_then(|value| value.parse::<i64>().ok())
        .map(|expires| expires > now_ms + 60_000)
        .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_freshness_uses_sixty_second_refresh_window() {
        assert!(token_is_fresh(Some("200000"), 100000));
        assert!(!token_is_fresh(Some("150000"), 100000));
        assert!(!token_is_fresh(None, 100000));
    }
}
