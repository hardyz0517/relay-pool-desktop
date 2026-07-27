#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparedNewApiAuthKind {
    AccessToken,
    Cookie,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedNewApiAuthContext {
    pub kind: PreparedNewApiAuthKind,
    pub secret: String,
    pub user_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct NewApiResolvedSession {
    pub access_token: Option<String>,
    pub cookie: Option<String>,
    pub newapi_user_id: Option<String>,
    pub message: Option<String>,
}

pub(crate) trait NewApiAuthSessionSource {
    fn resolve_newapi_session(
        &self,
        station_id: &str,
        data_key: &[u8; 32],
        now_ms: i64,
    ) -> Result<NewApiResolvedSession, String>;
}

pub(crate) fn prepare_collector_auth_context<S: NewApiAuthSessionSource + ?Sized>(
    source: &S,
    data_key: &[u8; 32],
    station_id: &str,
    now_ms: i64,
) -> Result<PreparedNewApiAuthContext, String> {
    let session = source.resolve_newapi_session(station_id, data_key, now_ms)?;
    let user_id = session
        .newapi_user_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "NewAPI session is missing user id".to_string())?;
    if let Some(access_token) = session
        .access_token
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(PreparedNewApiAuthContext {
            kind: PreparedNewApiAuthKind::AccessToken,
            secret: access_token,
            user_id,
        });
    }
    if let Some(cookie) = session
        .cookie
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(PreparedNewApiAuthContext {
            kind: PreparedNewApiAuthKind::Cookie,
            secret: cookie,
            user_id,
        });
    }
    Err(session
        .message
        .unwrap_or_else(|| "NewAPI session credentials are missing".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubSessionSource {
        session: NewApiResolvedSession,
    }

    impl NewApiAuthSessionSource for StubSessionSource {
        fn resolve_newapi_session(
            &self,
            _station_id: &str,
            _data_key: &[u8; 32],
            _now_ms: i64,
        ) -> Result<NewApiResolvedSession, String> {
            Ok(self.session.clone())
        }
    }

    #[test]
    fn auth_context_prefers_access_token_over_cookie() {
        let source = StubSessionSource {
            session: NewApiResolvedSession {
                access_token: Some("token".to_string()),
                cookie: Some("session=abc".to_string()),
                newapi_user_id: Some("42".to_string()),
                message: None,
            },
        };

        let context =
            prepare_collector_auth_context(&source, &[0; 32], "station-1", 123).expect("context");

        assert_eq!(context.kind, PreparedNewApiAuthKind::AccessToken);
        assert_eq!(context.secret, "token");
        assert_eq!(context.user_id, "42");
    }

    #[test]
    fn auth_context_uses_cookie_when_access_token_is_missing() {
        let source = StubSessionSource {
            session: NewApiResolvedSession {
                access_token: None,
                cookie: Some("session=abc".to_string()),
                newapi_user_id: Some("42".to_string()),
                message: None,
            },
        };

        let context =
            prepare_collector_auth_context(&source, &[0; 32], "station-1", 123).expect("context");

        assert_eq!(context.kind, PreparedNewApiAuthKind::Cookie);
        assert_eq!(context.secret, "session=abc");
    }

    #[test]
    fn auth_context_requires_user_id_and_session_secret() {
        let source = StubSessionSource {
            session: NewApiResolvedSession {
                access_token: None,
                cookie: None,
                newapi_user_id: Some("42".to_string()),
                message: Some("manual session required".to_string()),
            },
        };
        assert_eq!(
            prepare_collector_auth_context(&source, &[0; 32], "station-1", 123).unwrap_err(),
            "manual session required"
        );

        let source = StubSessionSource {
            session: NewApiResolvedSession {
                access_token: Some("token".to_string()),
                cookie: None,
                newapi_user_id: None,
                message: None,
            },
        };
        assert_eq!(
            prepare_collector_auth_context(&source, &[0; 32], "station-1", 123).unwrap_err(),
            "NewAPI session is missing user id"
        );
    }
}
