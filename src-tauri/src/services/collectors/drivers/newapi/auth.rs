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
pub(crate) struct NewApiLoginProbeOutcome {
    pub cookie_present: bool,
    pub login_message: Option<String>,
    pub manual_required: Option<String>,
}

#[cfg(test)]
struct NewApiPasswordLogin {
    user_id: String,
    cookie: Option<String>,
    outcome: NewApiLoginProbeOutcome,
}

#[cfg(test)]
const LOGIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

#[cfg(test)]
pub(crate) fn login_with_password(
    database: &dyn crate::services::collectors::CollectorSourcePort,
    data_key: &[u8; 32],
    station: &crate::models::stations::Station,
    login_username: &str,
    login_password: &str,
) -> Result<NewApiLoginProbeOutcome, String> {
    let login = request_password_login(&station.website_url, login_username, login_password)?;
    if login.outcome.manual_required.is_some() {
        return Ok(login.outcome);
    }
    database.persist_station_session_with_data_key(
        crate::models::credentials::PersistStationSessionInput {
            station_id: station.id.clone(),
            access_token: None,
            refresh_token: None,
            cookie: login.cookie,
            newapi_user_id: Some(login.user_id),
            token_expires_at: None,
            session_expires_at: None,
            session_source: "password_login".to_string(),
        },
        data_key,
        station.endpoint_revision,
    )?;
    Ok(login.outcome)
}

#[cfg(test)]
pub(crate) fn test_login_credentials(
    website_url: &str,
    login_username: &str,
    login_password: &str,
) -> Result<NewApiLoginProbeOutcome, String> {
    request_password_login(website_url, login_username, login_password).map(|login| login.outcome)
}

#[cfg(test)]
fn request_password_login(
    website_url: &str,
    login_username: &str,
    login_password: &str,
) -> Result<NewApiPasswordLogin, String> {
    let url =
        crate::services::station_endpoints::build_management_url(website_url, "/api/user/login")?;
    let response = match ureq::post(&url)
        .timeout(LOGIN_TIMEOUT)
        .set("Content-Type", "application/json")
        .send_json(serde_json::json!({
            "username": login_username,
            "password": login_password,
        })) {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => {
            return Err(crate::services::secrets::mask::redact_text(
                &error.to_string(),
            ))
        }
    };
    let status = response.status();
    let set_cookies = response
        .all("Set-Cookie")
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let text = response.into_string().unwrap_or_default();
    let payload =
        serde_json::from_str::<serde_json::Value>(&text).unwrap_or(serde_json::Value::Null);
    if !(200..400).contains(&status) {
        return Err(crate::services::secrets::mask::redact_text(&text));
    }
    if payload
        .get("require_2fa")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(NewApiLoginProbeOutcome {
            cookie_present: false,
            login_message: Some("NewAPI login requires manual verification".to_string()),
            manual_required: Some("manual_session_required".to_string()),
        }
        .into_password_login(String::new(), None));
    }
    let data = super::parsers::envelope_data(&payload)
        .map_err(|error| crate::services::secrets::mask::redact_text(&error.message))?;
    let user_id = data
        .get("id")
        .and_then(|value| {
            value
                .as_str()
                .map(ToString::to_string)
                .or_else(|| value.as_i64().map(|id| id.to_string()))
        })
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "NewAPI login response is missing user id".to_string())?;
    let cookie = normalize_set_cookie_headers(&set_cookies);
    let cookie_present = cookie.is_some();
    let manual_required = (!cookie_present).then(|| {
        "NewAPI login succeeded but did not return a session cookie; manual session is required."
            .to_string()
    });
    Ok(NewApiPasswordLogin {
        user_id,
        cookie,
        outcome: NewApiLoginProbeOutcome {
            cookie_present,
            login_message: Some(if cookie_present {
                "NewAPI login succeeded".to_string()
            } else {
                "NewAPI login did not return a session cookie".to_string()
            }),
            manual_required,
        },
    })
}

#[cfg(test)]
pub(crate) fn normalize_set_cookie_headers(headers: &[String]) -> Option<String> {
    let cookies = headers
        .iter()
        .filter_map(|header| header.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    (!cookies.is_empty()).then(|| cookies.join("; "))
}

#[cfg(test)]
impl NewApiLoginProbeOutcome {
    fn into_password_login(self, user_id: String, cookie: Option<String>) -> NewApiPasswordLogin {
        NewApiPasswordLogin {
            user_id,
            cookie,
            outcome: self,
        }
    }
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

    #[test]
    fn login_normalizes_multiple_set_cookie_headers() {
        let headers = vec![
            "session=abc; Path=/; HttpOnly; SameSite=Lax".to_string(),
            "lang=zh; Path=/".to_string(),
        ];
        assert_eq!(
            normalize_set_cookie_headers(&headers),
            Some("session=abc; lang=zh".to_string())
        );
    }
}
