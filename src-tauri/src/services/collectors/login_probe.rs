use std::time::Duration;

use http::{header, HeaderValue, Method};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::{
    models::collector::{StationLoginTestInput, StationLoginTestResult},
    outbound::{
        AsyncOutboundClient, OutboundHeaderPolicy, OutboundHeaders, OutboundRequest,
        OutboundRetryPolicy, ProxyPolicy, RequestBudget, SecretHeaderValue,
    },
    services::{secrets::mask::redact_text, station_endpoints::build_management_url},
};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(20);
// A password probe may try several compatible Sub2API contracts. The
// per-request timeout must not multiply into an unbounded UI operation.
const LOGIN_PROBE_TIMEOUT: Duration = Duration::from_secs(20);
const SUB2API_LOGIN_PATHS: [&str; 3] = ["/api/v1/auth/login", "/auth/login", "/api/login"];
const SUB2API_LOGIN_FIELDS: [&str; 3] = ["email", "username", "user"];

#[derive(Debug, Clone)]
pub(crate) struct LoginProbeAttempt {
    pub credential_present: bool,
    pub login_message: Option<String>,
    pub manual_required: Option<String>,
    pub newapi_session: Option<NewApiPasswordSession>,
    pub session: Option<LoginProbeSession>,
}

#[derive(Debug, Clone)]
pub(crate) struct LoginProbeSession {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub cookie: Option<String>,
    pub newapi_user_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct NewApiPasswordSession {
    pub user_id: String,
    pub cookie: String,
}

pub(crate) async fn test_station_login_input(
    outbound: &AsyncOutboundClient,
    input: StationLoginTestInput,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<StationLoginTestResult, String> {
    let website_url = input.website_url.trim();
    let login_username = input.login_username.trim();
    let login_password = input.login_password.trim();

    if website_url.is_empty() {
        return Ok(StationLoginTestResult {
            status: "missing_base_url".to_string(),
            message: "请先填写基础地址。".to_string(),
            diagnosis: None,
            token_present: false,
        });
    }
    if login_username.is_empty() || login_password.is_empty() {
        return Ok(StationLoginTestResult {
            status: "missing_credentials".to_string(),
            message: "请先填写登录用户名和密码。".to_string(),
            diagnosis: None,
            token_present: false,
        });
    }

    let station_type = input.station_type.as_deref().unwrap_or("sub2api").trim();
    let attempt = probe_login(
        outbound,
        station_type,
        website_url,
        login_username,
        login_password,
        ProxyPolicy::Direct,
        cancellation,
        correlation_id,
    )
    .await?;
    Ok(result_from_attempt(attempt))
}

pub(crate) async fn probe_login(
    outbound: &AsyncOutboundClient,
    station_type: &str,
    website_url: &str,
    login_username: &str,
    login_password: &str,
    proxy: ProxyPolicy,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<LoginProbeAttempt, String> {
    match station_type.trim() {
        "newapi" => {
            probe_newapi_login(
                outbound,
                website_url,
                login_username,
                login_password,
                proxy,
                cancellation,
                correlation_id,
            )
            .await
        }
        _ => {
            probe_sub2api_login(
                outbound,
                website_url,
                login_username,
                login_password,
                proxy,
                cancellation,
                correlation_id,
            )
            .await
        }
    }
}

fn result_from_attempt(attempt: LoginProbeAttempt) -> StationLoginTestResult {
    let token_present = attempt.credential_present;
    StationLoginTestResult {
        status: if token_present {
            "success"
        } else {
            "manual_required"
        }
        .to_string(),
        message: attempt
            .login_message
            .unwrap_or_else(|| "连通性测试已完成。".to_string()),
        diagnosis: attempt
            .manual_required
            .or_else(|| token_present.then(|| "登录接口返回可用 token。".to_string())),
        token_present,
    }
}

async fn probe_newapi_login(
    outbound: &AsyncOutboundClient,
    website_url: &str,
    login_username: &str,
    login_password: &str,
    proxy: ProxyPolicy,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<LoginProbeAttempt, String> {
    let deadline = tokio::time::Instant::now() + LOGIN_PROBE_TIMEOUT;
    let url = build_management_url(website_url, "/api/user/login")?;
    let response = execute_login_request(
        outbound,
        url,
        json!({
            "username": login_username,
            "password": login_password,
        }),
        proxy.clone(),
        cancellation.clone(),
        correlation_id.clone(),
        LOGIN_TIMEOUT,
    )
    .await?;
    let status = response.status.as_u16();
    let set_cookies = response
        .headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let body = response_body_json(&response.body);
    if !response.status.is_success() {
        if needs_manual_login(&body, status) {
            return Ok(newapi_manual_login_attempt(
                newapi_response_message(&body, "NewAPI login requires browser authorization"),
                "NewAPI login requires browser authorization",
            ));
        }
        let detail = redact_text(&String::from_utf8_lossy(&response.body));
        return Err(if detail.trim().is_empty() {
            format!("NewAPI login failed (HTTP {status})")
        } else {
            detail
        });
    }
    if needs_manual_login(&body, status) {
        return Ok(newapi_manual_login_attempt(
            newapi_response_message(&body, "NewAPI login requires manual verification"),
            "captcha, 2FA, or another interactive NewAPI login step is required",
        ));
    }
    if body.get("success").and_then(Value::as_bool) == Some(false) {
        return Ok(newapi_manual_login_attempt(
            newapi_response_message(&body, "NewAPI rejected the saved login credentials"),
            "NewAPI login was not accepted; complete browser authorization",
        ));
    }
    let Some(mut cookie) = normalize_set_cookie_headers(&set_cookies) else {
        return Ok(newapi_manual_login_attempt(
            "NewAPI login did not return a session cookie",
            "NewAPI login returned no reusable browser session",
        ));
    };
    let user_id = if let Some(user_id) = newapi_user_id(&body) {
        user_id
    } else {
        let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) else {
            return Ok(newapi_manual_login_attempt(
                "NewAPI login session verification timed out",
                "NewAPI login cookie could not be verified within the bounded time budget",
            ));
        };
        let self_url = build_management_url(website_url, "/api/user/self")?;
        let self_response = execute_newapi_self_request(
            outbound,
            self_url,
            &cookie,
            proxy,
            cancellation,
            correlation_id,
            remaining.min(LOGIN_TIMEOUT),
        )
        .await?;
        let self_status = self_response.status.as_u16();
        let self_set_cookies = self_response
            .headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let self_body = response_body_json(&self_response.body);
        if !self_response.status.is_success()
            || needs_manual_login(&self_body, self_status)
            || self_body.get("success").and_then(Value::as_bool) == Some(false)
        {
            return Ok(newapi_manual_login_attempt(
                newapi_response_message(
                    &self_body,
                    "NewAPI login cookie was rejected by the user self probe",
                ),
                "NewAPI login cookie could not be verified; complete browser authorization",
            ));
        }
        let Some(user_id) = newapi_user_id(&self_body) else {
            return Ok(newapi_manual_login_attempt(
                format!("NewAPI user self response is missing user id (HTTP {self_status})"),
                "NewAPI login cookie did not produce a verifiable user identity",
            ));
        };
        if let Some(merged_cookie) = merge_cookie_header(&cookie, &self_set_cookies) {
            cookie = merged_cookie;
        }
        user_id
    };
    let session = LoginProbeSession {
        access_token: None,
        refresh_token: None,
        cookie: Some(cookie.clone()),
        newapi_user_id: Some(user_id.clone()),
    };
    Ok(LoginProbeAttempt {
        credential_present: true,
        login_message: Some("NewAPI login succeeded".to_string()),
        manual_required: None,
        newapi_session: Some(NewApiPasswordSession { user_id, cookie }),
        session: Some(session),
    })
}

fn newapi_manual_login_attempt(
    login_message: impl Into<String>,
    diagnosis: impl Into<String>,
) -> LoginProbeAttempt {
    LoginProbeAttempt {
        credential_present: false,
        login_message: Some(login_message.into()),
        manual_required: Some(diagnosis.into()),
        newapi_session: None,
        session: None,
    }
}

fn newapi_response_message(body: &Value, fallback: &str) -> String {
    body.get("message")
        .and_then(Value::as_str)
        .map(redact_text)
        .filter(|value| !value.trim().is_empty())
        .map(|value| shorten_error(&value))
        .unwrap_or_else(|| fallback.to_string())
}

async fn probe_sub2api_login(
    outbound: &AsyncOutboundClient,
    website_url: &str,
    login_username: &str,
    login_password: &str,
    proxy: ProxyPolicy,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<LoginProbeAttempt, String> {
    let deadline = tokio::time::Instant::now() + LOGIN_PROBE_TIMEOUT;
    for path in SUB2API_LOGIN_PATHS {
        for field in SUB2API_LOGIN_FIELDS {
            let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now())
            else {
                return Ok(LoginProbeAttempt {
                    credential_present: false,
                    login_message: Some("login probe timed out".to_string()),
                    manual_required: Some(
                        "station login probe exceeded its bounded time budget".to_string(),
                    ),
                    newapi_session: None,
                    session: None,
                });
            };
            let url = build_management_url(website_url, path)?;
            let response = execute_login_request(
                outbound,
                url,
                json!({ field: login_username, "password": login_password }),
                proxy.clone(),
                cancellation.clone(),
                correlation_id.clone(),
                remaining.min(LOGIN_TIMEOUT),
            )
            .await?;
            let status = response.status.as_u16();
            let body = response_body_json(&response.body);
            let set_cookies = response
                .headers
                .get_all(header::SET_COOKIE)
                .iter()
                .filter_map(|value| value.to_str().ok())
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            if let Some(token) = extract_token(&body) {
                return Ok(LoginProbeAttempt {
                    credential_present: !token.trim().is_empty(),
                    login_message: Some(format!("login token received from {path}")),
                    manual_required: None,
                    newapi_session: None,
                    session: Some(LoginProbeSession {
                        access_token: Some(token),
                        refresh_token: extract_refresh_token(&body),
                        cookie: normalize_set_cookie_headers(&set_cookies),
                        // Sub2API's SPA route guard requires both auth_token
                        // and auth_user. Reuse the existing persisted user-id
                        // slot so browser scans can restore that identity
                        // without storing the full login response.
                        newapi_user_id: extract_user_id(&body),
                    }),
                });
            }
            if is_region_restricted_login(&body, status) {
                return Ok(LoginProbeAttempt {
                    credential_present: false,
                    login_message: Some(shorten_error(&body.to_string())),
                    manual_required: Some(
                        "login is region restricted; configure a collector proxy and retry"
                            .to_string(),
                    ),
                    newapi_session: None,
                    session: None,
                });
            }
            if needs_manual_login(&body, status) {
                return Ok(LoginProbeAttempt {
                    credential_present: false,
                    login_message: Some(shorten_error(&body.to_string())),
                    manual_required: Some(
                        "captcha, 2FA, or another interactive login step is required".to_string(),
                    ),
                    newapi_session: None,
                    session: None,
                });
            }
            if response.status.is_success() {
                return Ok(LoginProbeAttempt {
                    credential_present: false,
                    login_message: Some(
                        "login succeeded but the response contained no token".to_string(),
                    ),
                    manual_required: Some(
                        "the login response contained no usable token".to_string(),
                    ),
                    newapi_session: None,
                    session: None,
                });
            }
        }
    }
    Ok(LoginProbeAttempt {
        credential_present: false,
        login_message: Some("no login endpoint returned a usable token".to_string()),
        manual_required: Some(
            "credentials were rejected or the login contract changed".to_string(),
        ),
        newapi_session: None,
        session: None,
    })
}

async fn execute_login_request(
    outbound: &AsyncOutboundClient,
    url: String,
    payload: Value,
    proxy: ProxyPolicy,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
    timeout: Duration,
) -> Result<crate::outbound::OutboundResponse, String> {
    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    headers
        .insert_public(
            header::ACCEPT,
            HeaderValue::from_static("application/json"),
            &policy,
        )
        .map_err(|error| error.to_string())?;
    headers
        .insert_public(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
            &policy,
        )
        .map_err(|error| error.to_string())?;
    let request = OutboundRequest {
        method: Method::POST,
        url,
        correlation_id,
        headers,
        body: serde_json::to_vec(&payload).map_err(|error| error.to_string())?,
        proxy,
        budget: RequestBudget::from_now(timeout),
        retry_policy: OutboundRetryPolicy::Never,
    };
    outbound
        .execute(request, cancellation)
        .await
        .map_err(|error| error.to_string())
}

async fn execute_newapi_self_request(
    outbound: &AsyncOutboundClient,
    url: String,
    cookie: &str,
    proxy: ProxyPolicy,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
    timeout: Duration,
) -> Result<crate::outbound::OutboundResponse, String> {
    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    headers
        .insert_public(
            header::ACCEPT,
            HeaderValue::from_static("application/json"),
            &policy,
        )
        .map_err(|error| error.to_string())?;
    headers
        .insert_sensitive(
            header::COOKIE,
            SecretHeaderValue::new(cookie.to_string()),
            &policy,
        )
        .map_err(|error| error.to_string())?;
    let request = OutboundRequest {
        method: Method::GET,
        url,
        correlation_id,
        headers,
        body: Vec::new(),
        proxy,
        budget: RequestBudget::from_now(timeout),
        retry_policy: OutboundRetryPolicy::Never,
    };
    outbound
        .execute(request, cancellation)
        .await
        .map_err(|error| error.to_string())
}

fn response_body_json(body: &[u8]) -> Value {
    serde_json::from_slice::<Value>(body).unwrap_or(Value::Null)
}

fn extract_refresh_token(value: &Value) -> Option<String> {
    value
        .get("refresh_token")
        .or_else(|| value.get("refreshToken"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| value.get("data").and_then(extract_refresh_token))
}

fn normalize_set_cookie_headers(headers: &[String]) -> Option<String> {
    let cookies = headers
        .iter()
        .filter_map(|header| header.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    (!cookies.is_empty()).then(|| cookies.join("; "))
}

fn merge_cookie_header(existing: &str, set_cookie_headers: &[String]) -> Option<String> {
    let mut pairs = existing
        .split(';')
        .filter_map(cookie_pair)
        .collect::<Vec<_>>();
    for header in set_cookie_headers {
        let Some((name, value)) = header.split(';').next().and_then(cookie_pair) else {
            continue;
        };
        if let Some(existing) = pairs.iter_mut().find(|(current, _)| current == &name) {
            existing.1 = value;
        } else {
            pairs.push((name, value));
        }
    }
    (!pairs.is_empty()).then(|| {
        pairs
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    })
}

fn cookie_pair(value: &str) -> Option<(String, String)> {
    let (name, value) = value.trim().split_once('=')?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() || value.is_empty() {
        return None;
    }
    Some((name.to_string(), value.to_string()))
}

fn newapi_user_id(value: &Value) -> Option<String> {
    value
        .pointer("/data/id")
        .or_else(|| value.get("id"))
        .and_then(string_or_i64)
}

fn extract_user_id(value: &Value) -> Option<String> {
    [
        value.pointer("/user/id"),
        value.pointer("/data/user/id"),
        value.pointer("/data/id"),
        value.get("id"),
    ]
    .into_iter()
    .flatten()
    .find_map(string_or_i64)
}

fn string_or_i64(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| value.as_i64().map(|id| id.to_string()))
}

fn extract_token(value: &Value) -> Option<String> {
    value
        .get("access_token")
        .or_else(|| value.get("token"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| value.get("data").and_then(extract_token))
}

fn is_region_restricted_login(value: &Value, status: u16) -> bool {
    if status != 403 {
        return false;
    }
    let text = value.to_string().to_lowercase();
    text.contains("region_restricted") || text.contains("region")
}

fn needs_manual_login(value: &Value, status: u16) -> bool {
    if matches!(status, 401 | 403) {
        return true;
    }
    let text = value.to_string().to_lowercase();
    text.contains("geetest")
        || text.contains("captcha")
        || text.contains("turnstile")
        || text.contains("verification_failed")
        || contains_true_flag(
            value,
            &[
                "require_2fa",
                "requires_2fa",
                "captcha_required",
                "manual_required",
            ],
        )
}

fn contains_true_flag(value: &Value, names: &[&str]) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(name, child)| {
            (names
                .iter()
                .any(|candidate| name.eq_ignore_ascii_case(candidate))
                && child.as_bool() == Some(true))
                || contains_true_flag(child, names)
        }),
        Value::Array(items) => items.iter().any(|item| contains_true_flag(item, names)),
        _ => false,
    }
}

fn shorten_error(message: &str) -> String {
    message.chars().take(240).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        outbound::AsyncOutboundClientConfig,
        services::collectors::drivers::newapi::test_support::TestHttpServer,
    };

    fn json_response_with_cookie(body: Value, cookie: &str) -> String {
        let body = body.to_string();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: {cookie}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        )
    }

    #[test]
    fn newapi_login_helpers_normalize_cookie_and_user_id() {
        let headers = vec![
            "session=abc; Path=/; HttpOnly".to_string(),
            "lang=zh; Path=/".to_string(),
        ];

        assert_eq!(
            normalize_set_cookie_headers(&headers),
            Some("session=abc; lang=zh".to_string())
        );
        assert_eq!(
            newapi_user_id(&json!({"success": true, "data": {"id": 42}})).as_deref(),
            Some("42")
        );
        assert_eq!(
            merge_cookie_header(
                "session=old; theme=light",
                &[
                    "session=rotated; Path=/; HttpOnly".to_string(),
                    "locale=zh; Path=/".to_string(),
                ],
            )
            .as_deref(),
            Some("session=rotated; theme=light; locale=zh")
        );
    }

    #[test]
    fn newapi_login_helpers_detect_nested_interactive_steps() {
        assert!(needs_manual_login(
            &json!({"success": true, "data": {"require_2fa": true}}),
            200,
        ));
        assert!(needs_manual_login(
            &json!({"success": false, "message": "Turnstile verification failed"}),
            200,
        ));
    }

    #[tokio::test]
    async fn newapi_login_uses_cookie_self_probe_when_login_body_has_no_user_id() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response_with_cookie(
                json!({"success": true, "data": {}}),
                "session=login; Path=/; HttpOnly",
            )),
            Some(json_response_with_cookie(
                json!({"success": true, "data": {"id": 42}}),
                "session=rotated; Path=/; HttpOnly",
            )),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());

        let attempt = probe_newapi_login(
            &outbound,
            &server.base_url,
            "user@example.invalid",
            "saved-password",
            ProxyPolicy::Direct,
            CancellationToken::new(),
            Some("newapi-login-test".to_string()),
        )
        .await
        .expect("login probe");

        let session = attempt.newapi_session.expect("verified NewAPI session");
        assert_eq!(session.user_id, "42");
        assert_eq!(session.cookie, "session=rotated");
        let requests = server.finish();
        assert!(requests[0].starts_with("POST /api/user/login "));
        assert!(requests[1].starts_with("GET /api/user/self "));
        assert!(requests[1]
            .to_ascii_lowercase()
            .contains("cookie: session=login"));
    }

    #[tokio::test]
    async fn newapi_turnstile_business_failure_is_not_misreported_as_missing_user_id() {
        let body = json!({
            "success": false,
            "message": "Turnstile verification failed"
        });
        let body_text = body.to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body_text}",
            body_text.len(),
        );
        let server = TestHttpServer::sequence(vec![Some(response)]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());

        let attempt = probe_newapi_login(
            &outbound,
            &server.base_url,
            "user@example.invalid",
            "saved-password",
            ProxyPolicy::Direct,
            CancellationToken::new(),
            Some("newapi-turnstile-test".to_string()),
        )
        .await
        .expect("manual login result");

        assert!(attempt.newapi_session.is_none());
        assert!(attempt.manual_required.is_some());
        assert_eq!(
            attempt.login_message.as_deref(),
            Some("Turnstile verification failed")
        );
        assert_eq!(server.finish().len(), 1);
    }

    #[test]
    fn sub2api_login_helpers_classify_token_and_manual_paths() {
        assert_eq!(
            extract_token(&json!({"data": {"access_token": "fresh-token"}})).as_deref(),
            Some("fresh-token")
        );
        assert_eq!(
            extract_user_id(&json!({"data": {"user": {"id": 42}}})).as_deref(),
            Some("42")
        );
        assert!(needs_manual_login(
            &json!({"reason": "GEETEST_VERIFICATION_FAILED"}),
            400
        ));
        assert!(is_region_restricted_login(
            &json!({"reason": "REGION_RESTRICTED"}),
            403
        ));
    }

    #[test]
    fn login_result_uses_readable_chinese_diagnosis() {
        let result = result_from_attempt(LoginProbeAttempt {
            credential_present: true,
            login_message: Some("login token received from /api/v1/auth/login".to_string()),
            manual_required: None,
            newapi_session: None,
            session: None,
        });

        assert_eq!(result.status, "success");
        assert_eq!(
            result.diagnosis.as_deref(),
            Some("登录接口返回可用 token。")
        );
        assert!(result.token_present);
    }
}
