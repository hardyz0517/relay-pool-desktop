use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::models::{credentials::StationSessionCredentialKind, stations::Station};
use crate::services::{
    collectors::{
        adapters::newapi::{auth, parsers},
        url::join_url,
    },
    database::AppDatabase,
    outbound::{agent_builder_for_proxy, resolve_proxy_config, ProxyConfig},
    secrets::mask::redact_text,
};

pub(super) const NEWAPI_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NewApiOperation {
    SelfInfo,
    Groups,
    Models,
    ListTokens,
    RevealToken,
    CreateToken,
}

#[derive(Debug, Clone)]
pub(super) struct NewApiResponse {
    pub data: serde_json::Value,
    pub endpoint_result: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NewApiRequestError {
    AuthRequired { code: String, message: String },
    ManualRequired { code: String, message: String },
    Transient { code: String, message: String },
    OutcomeUnknown { code: String, message: String },
    Permanent { code: String, message: String },
}

impl NewApiOperation {
    pub fn max_transient_retries(self) -> usize {
        match self {
            Self::CreateToken => 0,
            _ => 1,
        }
    }

    pub fn is_non_idempotent(self) -> bool {
        self == Self::CreateToken
    }
}

pub(super) fn get_json_with_auth_context(
    base_url: &str,
    path: &str,
    auth: &auth::NewApiAuthContext,
    operation: NewApiOperation,
    timeout: Duration,
) -> Result<NewApiResponse, NewApiRequestError> {
    execute_json_with_auth_context(
        ureq::AgentBuilder::new().timeout(timeout).build(),
        base_url,
        path,
        auth,
        operation,
        None,
        timeout,
    )
}

pub(super) fn post_json_with_auth_context(
    base_url: &str,
    path: &str,
    auth: &auth::NewApiAuthContext,
    operation: NewApiOperation,
    body: Value,
    timeout: Duration,
) -> Result<NewApiResponse, NewApiRequestError> {
    execute_json_with_auth_context(
        ureq::AgentBuilder::new().timeout(timeout).build(),
        base_url,
        path,
        auth,
        operation,
        Some(body),
        timeout,
    )
}

pub(super) fn get_authenticated_json(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    path: &str,
    operation: NewApiOperation,
) -> Result<NewApiResponse, NewApiRequestError> {
    authenticated_json(database, data_key, station, path, operation, None)
}

pub(super) fn post_authenticated_json(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    path: &str,
    operation: NewApiOperation,
    body: Value,
) -> Result<NewApiResponse, NewApiRequestError> {
    authenticated_json(database, data_key, station, path, operation, Some(body))
}

fn authenticated_json(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    path: &str,
    operation: NewApiOperation,
    body: Option<Value>,
) -> Result<NewApiResponse, NewApiRequestError> {
    let settings = database.get_settings().map_err(permanent_error)?;
    let proxy = resolve_proxy_config(
        &station.collector_proxy_mode,
        station.collector_proxy_url.clone(),
        &settings.collector_proxy_mode,
        settings.collector_proxy_url,
    );
    let mut auth_context = resolve_auth_context(database, data_key, station)?;
    let mut used_kind = auth_context.kind;
    for attempt in 0..=1 {
        match execute_json_with_proxy(
            &station.base_url,
            path,
            &auth_context,
            operation,
            body.clone(),
            &proxy,
        ) {
            Ok(response) => return Ok(response),
            Err(NewApiRequestError::AuthRequired { .. }) if attempt == 0 => {
                let credential_kind = match used_kind {
                    auth::NewApiAuthKind::AccessToken => StationSessionCredentialKind::AccessToken,
                    auth::NewApiAuthKind::Cookie => StationSessionCredentialKind::Cookie,
                };
                database
                    .invalidate_station_session_credential(&station.id, credential_kind)
                    .map_err(permanent_error)?;
                auth_context = resolve_auth_context(database, data_key, station)?;
                used_kind = auth_context.kind;
            }
            Err(error) => return Err(error),
        }
    }
    Err(auth_required_error("NewAPI authentication failed"))
}

fn resolve_auth_context(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
) -> Result<auth::NewApiAuthContext, NewApiRequestError> {
    let session = database
        .resolve_station_session_with_data_key(
            station.id.clone(),
            data_key,
            crate::services::database::now_millis_for_services() as i64,
        )
        .map_err(permanent_error)?;
    let user_id = session
        .newapi_user_id
        .clone()
        .filter(|value| !value.trim().is_empty());
    if let (Some(access_token), Some(user_id)) = (session.access_token.clone(), user_id.clone()) {
        return Ok(auth::NewApiAuthContext::access_token(access_token, user_id));
    }
    if let (Some(cookie), Some(user_id)) = (session.cookie.clone(), user_id) {
        return Ok(auth::NewApiAuthContext::cookie(cookie, user_id));
    }
    let credentials = database
        .get_station_credentials(station.id.clone())
        .map_err(permanent_error)?;
    let Some(username) = credentials
        .login_username
        .filter(|value| !value.trim().is_empty())
    else {
        return Err(manual_required_error(
            "NewAPI login credentials are missing",
        ));
    };
    let password = database
        .get_station_login_password_with_data_key(station.id.clone(), data_key)
        .map_err(permanent_error)?
        .ok_or_else(|| manual_required_error("NewAPI login password is missing"))?;
    auth::login_with_password(database, data_key, station, &username, &password)
        .map_err(permanent_error)?;
    let session = database
        .resolve_station_session_with_data_key(
            station.id.clone(),
            data_key,
            crate::services::database::now_millis_for_services() as i64,
        )
        .map_err(permanent_error)?;
    match (session.cookie, session.newapi_user_id) {
        (Some(cookie), Some(user_id)) if !user_id.trim().is_empty() => {
            Ok(auth::NewApiAuthContext::cookie(cookie, user_id))
        }
        _ => Err(manual_required_error(
            "NewAPI password login did not produce a Cookie session",
        )),
    }
}

fn execute_json_with_proxy(
    base_url: &str,
    path: &str,
    auth: &auth::NewApiAuthContext,
    operation: NewApiOperation,
    body: Option<Value>,
    proxy: &ProxyConfig,
) -> Result<NewApiResponse, NewApiRequestError> {
    let agent = agent_builder_for_proxy(proxy)
        .map_err(permanent_error)?
        .timeout(NEWAPI_REQUEST_TIMEOUT)
        .build();
    execute_json_with_auth_context(
        agent,
        base_url,
        path,
        auth,
        operation,
        body,
        NEWAPI_REQUEST_TIMEOUT,
    )
}

fn execute_json_with_auth_context(
    agent: ureq::Agent,
    base_url: &str,
    path: &str,
    auth: &auth::NewApiAuthContext,
    operation: NewApiOperation,
    body: Option<Value>,
    timeout: Duration,
) -> Result<NewApiResponse, NewApiRequestError> {
    let url = join_url(base_url, path);
    let mut attempt = 0;
    loop {
        let started = Instant::now();
        let result = send_once(&agent, &url, auth, body.clone(), timeout);
        match result {
            Ok(response) => {
                let status = response.status();
                let text = response.into_string().unwrap_or_default();
                let endpoint_result = json!({
                    "path": path,
                    "status": status,
                    "ok": (200..400).contains(&status),
                    "durationMs": started.elapsed().as_millis() as i64,
                });
                if is_auth_status(status) {
                    return Err(auth_required_error(&text));
                }
                if is_transient_status(status) {
                    if attempt < operation.max_transient_retries() {
                        attempt += 1;
                        continue;
                    }
                    return Err(transient_error(&text));
                }
                if !(200..400).contains(&status) {
                    return Err(permanent_error(text));
                }
                let payload = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
                let data = match parsers::envelope_data(&payload) {
                    Ok(data) => data.clone(),
                    Err(_error)
                        if operation == NewApiOperation::CreateToken
                            && payload.get("success").and_then(Value::as_bool) == Some(true) =>
                    {
                        Value::Null
                    }
                    Err(error) => return Err(permanent_error(error.message)),
                };
                return Ok(NewApiResponse {
                    data,
                    endpoint_result,
                });
            }
            Err(error) => {
                if operation.is_non_idempotent() {
                    return Err(NewApiRequestError::OutcomeUnknown {
                        code: "create_outcome_unknown".to_string(),
                        message: "NewAPI non-idempotent request outcome is unknown".to_string(),
                    });
                }
                if attempt < operation.max_transient_retries() {
                    attempt += 1;
                    continue;
                }
                return Err(transient_error(error.to_string()));
            }
        }
    }
}

fn send_once(
    agent: &ureq::Agent,
    url: &str,
    auth: &auth::NewApiAuthContext,
    body: Option<Value>,
    timeout: Duration,
) -> Result<ureq::Response, ureq::Error> {
    let request = if body.is_some() {
        agent.post(url)
    } else {
        agent.get(url)
    }
    .timeout(timeout)
    .set("New-Api-User", &auth.user_id)
    .set("Content-Type", "application/json");
    let request = if let Some(authorization) = auth.authorization_value() {
        request.set("Authorization", &authorization)
    } else if let Some(cookie) = auth.cookie_value() {
        request.set("Cookie", cookie)
    } else {
        request
    };
    match body {
        Some(body) => request.send_json(body),
        None => request.call(),
    }
}

fn is_auth_status(status: u16) -> bool {
    matches!(status, 401 | 403)
}

fn is_transient_status(status: u16) -> bool {
    status == 408 || status == 429 || (500..600).contains(&status)
}

fn sanitize_message(message: impl Into<String>) -> String {
    let redacted = redact_text(&message.into());
    redacted.chars().take(240).collect()
}

fn auth_required_error(message: impl Into<String>) -> NewApiRequestError {
    NewApiRequestError::AuthRequired {
        code: "auth_required".to_string(),
        message: sanitize_message(message),
    }
}

fn manual_required_error(message: impl Into<String>) -> NewApiRequestError {
    NewApiRequestError::ManualRequired {
        code: "manual_session_required".to_string(),
        message: sanitize_message(message),
    }
}

fn transient_error(message: impl Into<String>) -> NewApiRequestError {
    NewApiRequestError::Transient {
        code: "transient_request_failed".to_string(),
        message: sanitize_message(message),
    }
}

fn permanent_error(message: impl Into<String>) -> NewApiRequestError {
    NewApiRequestError::Permanent {
        code: "permanent_request_failed".to_string(),
        message: sanitize_message(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::collectors::adapters::newapi::{
        auth::NewApiAuthContext,
        test_support::{json_response, TestHttpServer},
    };
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn get_retries_one_transient_failure_but_create_never_retries() {
        assert_eq!(NewApiOperation::ListTokens.max_transient_retries(), 1);
        assert_eq!(NewApiOperation::CreateToken.max_transient_retries(), 0);
        assert!(NewApiOperation::CreateToken.is_non_idempotent());
    }

    #[test]
    fn get_retries_one_transient_response() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                502,
                json!({"success": false, "message": "bad gateway"}),
            )),
            Some(json_response(
                200,
                json!({"success": true, "data": {"ok": true}}),
            )),
        ]);
        let auth = NewApiAuthContext::access_token("secret", "42");

        let result = get_json_with_auth_context(
            &server.base_url,
            "/api/status",
            &auth,
            NewApiOperation::ListTokens,
            Duration::from_secs(2),
        )
        .expect("response");
        let requests = server.finish();

        assert_eq!(result.data["ok"], true);
        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("Authorization: Bearer secret"));
    }

    #[test]
    fn create_returns_unknown_outcome_without_retry_when_connection_closes() {
        let server = TestHttpServer::sequence(vec![None]);
        let auth = NewApiAuthContext::cookie("session=abc", "42");

        let error = post_json_with_auth_context(
            &server.base_url,
            "/api/token/",
            &auth,
            NewApiOperation::CreateToken,
            json!({"name": "relay"}),
            Duration::from_secs(2),
        )
        .unwrap_err();
        let requests = server.finish();

        assert_eq!(
            error,
            NewApiRequestError::OutcomeUnknown {
                code: "create_outcome_unknown".to_string(),
                message: "NewAPI non-idempotent request outcome is unknown".to_string(),
            }
        );
        assert_eq!(requests.len(), 1);
    }
}
