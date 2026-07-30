use std::time::Duration;

use http::{header, HeaderValue, Method};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::{
    models::collector::{StationLoginTestInput, StationLoginTestResult},
    outbound::{
        AsyncOutboundClient, OutboundHeaderPolicy, OutboundHeaders, OutboundRequest,
        OutboundRetryPolicy, ProxyPolicy, RequestBudget,
    },
    services::{secrets::mask::redact_text, station_endpoints::build_management_url},
};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(20);
const SUB2API_LOGIN_PATHS: [&str; 3] = ["/api/v1/auth/login", "/auth/login", "/api/login"];
const SUB2API_LOGIN_FIELDS: [&str; 3] = ["email", "username", "user"];

#[derive(Debug, Clone)]
pub(crate) struct LoginProbeAttempt {
    pub credential_present: bool,
    pub login_message: Option<String>,
    pub manual_required: Option<String>,
    pub newapi_session: Option<NewApiPasswordSession>,
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
    let url = build_management_url(website_url, "/api/user/login")?;
    let response = execute_login_request(
        outbound,
        url,
        json!({
            "username": login_username,
            "password": login_password,
        }),
        proxy,
        cancellation,
        correlation_id,
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
        return Err(redact_text(&String::from_utf8_lossy(&response.body)));
    }
    if body
        .get("require_2fa")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(LoginProbeAttempt {
            credential_present: false,
            login_message: Some("NewAPI login requires manual verification".to_string()),
            manual_required: Some("manual_session_required".to_string()),
            newapi_session: None,
        });
    }
    let user_id = newapi_user_id(&body)
        .ok_or_else(|| format!("NewAPI login response is missing user id (HTTP {status})"))?;
    let cookie = normalize_set_cookie_headers(&set_cookies);
    let Some(cookie) = cookie else {
        return Ok(LoginProbeAttempt {
            credential_present: false,
            login_message: Some("NewAPI login did not return a session cookie".to_string()),
            manual_required: Some(
                "NewAPI login succeeded but the response had no usable Cookie".to_string(),
            ),
            newapi_session: None,
        });
    };
    Ok(LoginProbeAttempt {
        credential_present: true,
        login_message: Some("NewAPI login succeeded".to_string()),
        manual_required: None,
        newapi_session: Some(NewApiPasswordSession { user_id, cookie }),
    })
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
    for path in SUB2API_LOGIN_PATHS {
        for field in SUB2API_LOGIN_FIELDS {
            let url = build_management_url(website_url, path)?;
            let response = execute_login_request(
                outbound,
                url,
                json!({ field: login_username, "password": login_password }),
                proxy.clone(),
                cancellation.clone(),
                correlation_id.clone(),
            )
            .await?;
            let status = response.status.as_u16();
            let body = response_body_json(&response.body);
            if let Some(token) = extract_token(&body) {
                return Ok(LoginProbeAttempt {
                    credential_present: !token.trim().is_empty(),
                    login_message: Some(format!("login token received from {path}")),
                    manual_required: None,
                    newapi_session: None,
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
    })
}

async fn execute_login_request(
    outbound: &AsyncOutboundClient,
    url: String,
    payload: Value,
    proxy: ProxyPolicy,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
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
        budget: RequestBudget::from_now(LOGIN_TIMEOUT),
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

fn newapi_user_id(value: &Value) -> Option<String> {
    value
        .pointer("/data/id")
        .or_else(|| value.get("id"))
        .and_then(string_or_i64)
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
        || value
            .get("requires_2fa")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || value
            .get("captcha_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || value
            .get("manual_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn shorten_error(message: &str) -> String {
    message.chars().take(240).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn sub2api_login_helpers_classify_token_and_manual_paths() {
        assert_eq!(
            extract_token(&json!({"data": {"access_token": "fresh-token"}})).as_deref(),
            Some("fresh-token")
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
        });

        assert_eq!(result.status, "success");
        assert_eq!(
            result.diagnosis.as_deref(),
            Some("登录接口返回可用 token。")
        );
        assert!(result.token_present);
    }
}
