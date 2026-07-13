use serde_json::{json, Value};

use crate::models::{credentials::PersistStationSessionInput, stations::Station};
use crate::services::{
    collectors::{
        adapters::newapi::parsers,
        url::{collector_base_urls, join_url},
    },
    database::AppDatabase,
    secrets::mask::redact_text,
};

const LOGIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NewApiAuthKind {
    AccessToken,
    Cookie,
}

#[derive(Debug, Clone)]
pub(super) struct NewApiAuthContext {
    pub kind: NewApiAuthKind,
    pub secret: String,
    pub user_id: String,
    pub session_source: String,
}

impl NewApiAuthContext {
    pub fn access_token(secret: impl Into<String>, user_id: impl Into<String>) -> Self {
        Self {
            kind: NewApiAuthKind::AccessToken,
            secret: secret.into(),
            user_id: user_id.into(),
            session_source: "unknown".to_string(),
        }
    }

    pub fn cookie(secret: impl Into<String>, user_id: impl Into<String>) -> Self {
        Self {
            kind: NewApiAuthKind::Cookie,
            secret: secret.into(),
            user_id: user_id.into(),
            session_source: "unknown".to_string(),
        }
    }

    pub fn with_session_source(mut self, session_source: impl Into<String>) -> Self {
        let session_source = session_source.into();
        let trimmed = session_source.trim();
        self.session_source = if trimmed.is_empty() {
            "unknown".to_string()
        } else {
            trimmed.to_string()
        };
        self
    }

    pub fn authorization_value(&self) -> Option<String> {
        (self.kind == NewApiAuthKind::AccessToken).then(|| format!("Bearer {}", self.secret))
    }

    pub fn cookie_value(&self) -> Option<&str> {
        (self.kind == NewApiAuthKind::Cookie).then_some(self.secret.as_str())
    }
}

pub(crate) struct NewApiLoginProbeOutcome {
    pub cookie_present: bool,
    pub login_message: Option<String>,
    pub manual_required: Option<String>,
}

struct NewApiPasswordLogin {
    user_id: String,
    cookie: Option<String>,
    outcome: NewApiLoginProbeOutcome,
}

pub(super) fn normalize_set_cookie_headers(headers: &[String]) -> Option<String> {
    let cookies = headers
        .iter()
        .filter_map(|header| header.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    (!cookies.is_empty()).then(|| cookies.join("; "))
}

pub(crate) fn login_with_password(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    login_username: &str,
    login_password: &str,
) -> Result<NewApiLoginProbeOutcome, String> {
    let login = request_password_login(&station.base_url, login_username, login_password)?;
    if login.outcome.manual_required.is_some() {
        return Ok(login.outcome);
    }
    database.persist_station_session_with_data_key(
        PersistStationSessionInput {
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
    )?;
    Ok(login.outcome)
}

pub(crate) fn test_login_credentials(
    base_url: &str,
    login_username: &str,
    login_password: &str,
) -> Result<NewApiLoginProbeOutcome, String> {
    request_password_login(base_url, login_username, login_password).map(|login| login.outcome)
}

fn request_password_login(
    base_url: &str,
    login_username: &str,
    login_password: &str,
) -> Result<NewApiPasswordLogin, String> {
    let urls = collector_base_urls(base_url);
    let url = join_url(&urls.management_base_url, "/api/user/login");
    let response = match ureq::post(&url)
        .timeout(LOGIN_TIMEOUT)
        .set("Content-Type", "application/json")
        .send_json(json!({
            "username": login_username,
            "password": login_password,
        })) {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => return Err(redact_text(&error.to_string())),
    };
    let status = response.status();
    let set_cookies = response
        .all("Set-Cookie")
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let text = response.into_string().unwrap_or_default();
    let payload = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
    if !(200..400).contains(&status) {
        return Err(redact_text(&text));
    }
    if payload
        .get("require_2fa")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(NewApiLoginProbeOutcome {
            cookie_present: false,
            login_message: Some("NewAPI login requires manual verification".to_string()),
            manual_required: Some("manual_session_required".to_string()),
        }
        .into_password_login(String::new(), None));
    }
    let data = parsers::envelope_data(&payload).map_err(|error| redact_text(&error.message))?;
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
    let manual_required = (!cookie_present)
        .then(|| "NewAPI 登录成功但响应没有返回 Cookie，无法保存会话。".to_string());
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

    #[test]
    fn access_token_and_cookie_emit_distinct_headers() {
        assert_eq!(
            NewApiAuthContext::access_token("secret", "42").authorization_value(),
            Some("Bearer secret".to_string())
        );
        assert_eq!(
            NewApiAuthContext::cookie("session=abc", "42").cookie_value(),
            Some("session=abc")
        );
    }
}
