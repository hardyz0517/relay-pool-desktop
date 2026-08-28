use http::{header, HeaderName, HeaderValue, Method};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::{
    models::{
        credentials::ResolvedSession, station_redemption::StationRedemptionResult,
        stations::Station,
    },
    outbound::{
        AsyncOutboundClient, OutboundFailureKind, OutboundHeaderPolicy, OutboundHeaders,
        OutboundRequest, OutboundRetryPolicy, ProxyPolicy, RequestBudget, SecretHeaderValue,
    },
    services::{
        station_endpoints::build_management_url,
        station_sessions::{merge_set_cookie_headers, token_expires_at_from_payload},
    },
};

const NEW_API_USER_HEADER: HeaderName = HeaderName::from_static("new-api-user");
const SUB2API_USER_UI_REQUEST_HEADER: HeaderName = HeaderName::from_static("x-user-ui-request");
const MAX_UPSTREAM_MESSAGE_CHARS: usize = 512;

pub(crate) struct StationRedemptionAttempt {
    pub result: StationRedemptionResult,
    pub authentication_rejected: bool,
}

pub(crate) struct RefreshedSub2ApiSession {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub cookie: Option<String>,
    pub token_expires_at: Option<String>,
}

pub(crate) fn timeout_result(station_type: &str) -> StationRedemptionResult {
    result(
        &station_type.to_ascii_lowercase(),
        false,
        "兑换请求超时，请稍后重试。",
    )
}

pub(crate) async fn redeem_station_code(
    outbound: &AsyncOutboundClient,
    station: &Station,
    session: &ResolvedSession,
    code: &str,
    user_agent: Option<&str>,
    proxy: ProxyPolicy,
    budget: RequestBudget,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> StationRedemptionAttempt {
    let provider = station.station_type.to_ascii_lowercase();
    let request = match build_request(
        station,
        session,
        code,
        user_agent,
        proxy,
        budget,
        correlation_id,
    ) {
        Ok(request) => request,
        Err(message) => return attempt(result(&provider, false, message), false),
    };

    let response = match outbound.execute(request, cancellation).await {
        Ok(response) => response,
        Err(error) => {
            return attempt(
                result(&provider, false, outbound_failure_message(&error.kind)),
                false,
            )
        }
    };
    let authentication_rejected =
        is_authentication_rejection(response.status.as_u16(), &payload_from_body(&response.body));
    let payload = serde_json::from_slice::<Value>(&response.body).unwrap_or(Value::Null);
    attempt(
        inspect_response(&provider, response.status.as_u16(), payload, code),
        authentication_rejected,
    )
}

pub(crate) async fn refresh_sub2api_session(
    outbound: &AsyncOutboundClient,
    station: &Station,
    refresh_token: &str,
    cookie: Option<&str>,
    user_agent: Option<&str>,
    proxy: ProxyPolicy,
    budget: RequestBudget,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Option<RefreshedSub2ApiSession> {
    let refresh_token = refresh_token.trim();
    if refresh_token.is_empty() {
        return None;
    }
    let url = build_management_url(&station.website_url, "/api/v1/auth/refresh").ok()?;
    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    headers
        .insert_public(
            header::ACCEPT,
            HeaderValue::from_static("application/json"),
            &policy,
        )
        .ok()?;
    headers
        .insert_public(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
            &policy,
        )
        .ok()?;
    if let Some(user_agent) = user_agent.map(str::trim).filter(|value| !value.is_empty()) {
        headers
            .insert_public(
                header::USER_AGENT,
                HeaderValue::from_str(user_agent).ok()?,
                &policy,
            )
            .ok()?;
    }
    if let Some(cookie) = cookie.map(str::trim).filter(|value| !value.is_empty()) {
        headers
            .insert_sensitive(
                header::COOKIE,
                SecretHeaderValue::new(cookie.to_string()),
                &policy,
            )
            .ok()?;
    }
    let response = outbound
        .execute(
            OutboundRequest {
                method: Method::POST,
                url,
                correlation_id,
                headers,
                body: serde_json::to_vec(&json!({ "refresh_token": refresh_token })).ok()?,
                proxy,
                budget,
                retry_policy: OutboundRetryPolicy::Never,
            },
            cancellation,
        )
        .await
        .ok()?;
    if !response.status.is_success() {
        return None;
    }
    let payload = payload_from_body(&response.body);
    let access_token = response_string(&payload, &["/access_token", "/data/access_token"])?;
    let refresh_token = response_string(&payload, &["/refresh_token", "/data/refresh_token"]);
    let set_cookie_headers = response
        .headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    Some(RefreshedSub2ApiSession {
        access_token: access_token.to_string(),
        refresh_token: refresh_token.map(ToString::to_string),
        cookie: merge_set_cookie_headers(cookie, &set_cookie_headers),
        token_expires_at: token_expires_at_from_payload(&payload),
    })
}

fn build_request(
    station: &Station,
    session: &ResolvedSession,
    code: &str,
    user_agent: Option<&str>,
    proxy: ProxyPolicy,
    budget: RequestBudget,
    correlation_id: Option<String>,
) -> Result<OutboundRequest, &'static str> {
    let provider = station.station_type.to_ascii_lowercase();
    let (path, body) = match provider.as_str() {
        "sub2api" => ("/api/v1/redeem", json!({ "code": code })),
        "newapi" => ("/api/user/topup", json!({ "key": code })),
        _ => return Err("当前站点类型不支持兑换码。"),
    };
    let url =
        build_management_url(&station.website_url, path).map_err(|_| "站点地址无效，无法兑换。")?;

    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    headers
        .insert_public(
            header::ACCEPT,
            HeaderValue::from_static("application/json"),
            &policy,
        )
        .map_err(|_| "无法创建兑换请求。")?;
    if let Some(user_agent) = user_agent.map(str::trim).filter(|value| !value.is_empty()) {
        headers
            .insert_public(
                header::USER_AGENT,
                HeaderValue::from_str(user_agent)
                    .map_err(|_| "保存的浏览器标识无效，请重新进行窗口授权。")?,
                &policy,
            )
            .map_err(|_| "无法创建兑换请求。")?;
    }
    headers
        .insert_public(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
            &policy,
        )
        .map_err(|_| "无法创建兑换请求。")?;

    if provider == "sub2api" {
        headers
            .insert_public(
                SUB2API_USER_UI_REQUEST_HEADER,
                HeaderValue::from_static("1"),
                &policy,
            )
            .map_err(|_| "无法创建兑换请求。")?;
    } else {
        let user_id = session
            .newapi_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or("缺少 NewAPI 用户标识，请先完成登录或浏览器授权。")?;
        headers
            .insert_public(
                NEW_API_USER_HEADER,
                HeaderValue::from_str(user_id).map_err(|_| "NewAPI 用户标识无效。")?,
                &policy,
            )
            .map_err(|_| "无法创建兑换请求。")?;
    }

    let bearer = session.access_token.as_deref().map(str::trim).unwrap_or("");
    let cookie = session
        .cookie
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| looks_like_cookie_header(bearer).then_some(bearer));
    if !bearer.is_empty() && !looks_like_cookie_header(bearer) {
        headers
            .insert_sensitive(
                header::AUTHORIZATION,
                SecretHeaderValue::new(format!("Bearer {bearer}")),
                &policy,
            )
            .map_err(|_| "无法创建兑换请求。")?;
    }
    if let Some(cookie) = cookie {
        headers
            .insert_sensitive(
                header::COOKIE,
                SecretHeaderValue::new(cookie.to_string()),
                &policy,
            )
            .map_err(|_| "无法创建兑换请求。")?;
    }
    if bearer.is_empty() && cookie.is_none() {
        return Err("缺少可用登录会话，请先完成登录或浏览器授权。");
    }

    Ok(OutboundRequest {
        method: Method::POST,
        url,
        correlation_id,
        headers,
        body: serde_json::to_vec(&body).map_err(|_| "无法创建兑换请求。")?,
        proxy,
        budget,
        retry_policy: OutboundRetryPolicy::Never,
    })
}

fn inspect_response(
    provider: &str,
    http_status: u16,
    payload: Value,
    submitted_code: &str,
) -> StationRedemptionResult {
    let http_success = (200..300).contains(&http_status);
    let success = match provider {
        "sub2api" => http_success && payload.get("code").and_then(Value::as_i64) == Some(0),
        "newapi" => http_success && payload.get("success").and_then(Value::as_bool) == Some(true),
        _ => false,
    };
    let message = response_message(provider, &payload);
    let fallback = if success {
        "兑换成功。"
    } else if http_status == 401 {
        "登录状态已失效，请重新授权后再试。"
    } else if !http_success {
        "兑换失败，站点拒绝了本次请求。"
    } else {
        "兑换失败，请检查兑换码和登录状态。"
    };
    let message = localize_message(message.unwrap_or(fallback), success);
    StationRedemptionResult {
        provider: provider.to_string(),
        success,
        message: safe_message(message, submitted_code),
        credited_detail: success_detail(provider, &payload),
    }
}

fn result(provider: &str, success: bool, message: impl Into<String>) -> StationRedemptionResult {
    StationRedemptionResult {
        provider: provider.to_string(),
        success,
        message: message.into(),
        credited_detail: None,
    }
}

fn attempt(
    result: StationRedemptionResult,
    authentication_rejected: bool,
) -> StationRedemptionAttempt {
    StationRedemptionAttempt {
        result,
        authentication_rejected,
    }
}

fn payload_from_body(body: &[u8]) -> Value {
    serde_json::from_slice::<Value>(body).unwrap_or(Value::Null)
}

fn response_string<'a>(payload: &'a Value, pointers: &[&str]) -> Option<&'a str> {
    pointers
        .iter()
        .find_map(|pointer| payload.pointer(pointer).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn is_authentication_rejection(http_status: u16, payload: &Value) -> bool {
    if http_status == 401 {
        return true;
    }
    let reason = response_string(
        payload,
        &[
            "/reason",
            "/error/reason",
            "/code",
            "/error/code",
            "/message",
            "/error/message",
        ],
    )
    .unwrap_or_default()
    .to_ascii_lowercase();
    contains_any(
        &reason,
        &[
            "token_expired",
            "token expired",
            "token has expired",
            "session_expired",
            "session expired",
            "unauthorized",
            "not authenticated",
            "login required",
        ],
    )
}

fn response_message<'a>(provider: &str, payload: &'a Value) -> Option<&'a str> {
    let candidates = if provider == "sub2api" {
        [
            "/reason",
            "/data/message",
            "/message",
            "/error/message",
            "/error/code",
        ]
    } else {
        [
            "/message",
            "/data/message",
            "/data",
            "/error/message",
            "/error/code",
        ]
    };
    candidates
        .iter()
        .find_map(|pointer| payload.pointer(pointer).and_then(Value::as_str))
}

fn success_detail(provider: &str, payload: &Value) -> Option<String> {
    if provider == "sub2api" {
        let data = payload.get("data")?;
        let benefit_type = data
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("balance");
        let value = data.get("value").and_then(Value::as_f64)?;
        return match benefit_type.to_ascii_lowercase().as_str() {
            "balance" => Some(format!("已添加：${value:.2}")),
            "concurrency" => Some(format!("已增加并发：{}", format_number(value))),
            "subscription" => Some("订阅权益已生效".to_string()),
            _ => Some(format!("已添加权益：{}", format_number(value))),
        };
    }

    // NewAPI returns its internal quota integer. A station may customize
    // quota_per_unit, so do not guess a currency conversion here.
    payload
        .get("data")
        .and_then(Value::as_f64)
        .map(|value| format!("已添加额度：{}", format_number(value)))
}

fn format_number(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        let formatted = format!("{value:.4}");
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn localize_message(message: &str, success: bool) -> &str {
    let trimmed = message.trim();
    if success && (trimmed.is_empty() || trimmed.eq_ignore_ascii_case("success")) {
        return "兑换成功";
    }

    let normalized = trimmed.to_ascii_lowercase();
    if contains_any(
        &normalized,
        &[
            "token_expired",
            "token expired",
            "token has expired",
            "access token expired",
            "session_expired",
            "session expired",
            "authentication expired",
            "not authenticated",
            "unauthorized",
            "not logged",
            "login required",
        ],
    ) {
        "登录状态已失效，请重新授权后再试。"
    } else if contains_any(
        &normalized,
        &[
            "redeem_code_not_found",
            "redeem code not found",
            "redeem_code_invalid",
            "invalid redemption",
            "invalid redeem",
            "invalid code",
        ],
    ) {
        "兑换码无效，请检查后重试。"
    } else if contains_any(
        &normalized,
        &[
            "redeem_code_used",
            "already used",
            "has been used",
            "code used",
        ],
    ) {
        "该兑换码已被使用。"
    } else if contains_any(
        &normalized,
        &[
            "redeem_code_expired",
            "redeem code expired",
            "redemption code expired",
            "redeem code has expired",
        ],
    ) {
        "该兑换码已过期。"
    } else if contains_any(
        &normalized,
        &[
            "redeem_rate_limited",
            "too many failed attempts",
            "rate limit",
        ],
    ) {
        "尝试次数过多，请稍后再试。"
    } else if contains_any(
        &normalized,
        &["redeem_code_locked", "being processed", "top up processing"],
    ) {
        "兑换请求正在处理中，请稍后再试。"
    } else if contains_any(
        &normalized,
        &[
            "redeem_code_unsupported_type",
            "invitation codes can only be used during registration",
            "unsupported redeem type",
        ],
    ) {
        "该兑换码类型不能在这里使用。"
    } else if contains_any(
        &normalized,
        &["payment compliance", "payment setting is not enabled"],
    ) {
        "站点暂未开放充值功能。"
    } else if contains_any(
        &normalized,
        &["redeem failed", "redemption failed", "failed to redeem"],
    ) {
        "兑换码无效或已失效，请检查后重试。"
    } else {
        trimmed
    }
}

fn contains_any(value: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| value.contains(candidate))
}

fn safe_message(message: &str, submitted_code: &str) -> String {
    let redacted = crate::services::secrets::mask::redact_text(message);
    let redacted = if submitted_code.is_empty() {
        redacted
    } else {
        redacted.replace(submitted_code, "[REDACTED]")
    };
    redacted.chars().take(MAX_UPSTREAM_MESSAGE_CHARS).collect()
}

fn outbound_failure_message(kind: &OutboundFailureKind) -> &'static str {
    match kind {
        OutboundFailureKind::ConnectTimeout
        | OutboundFailureKind::FirstByteTimeout
        | OutboundFailureKind::BodyTimeout
        | OutboundFailureKind::TotalTimeout
        | OutboundFailureKind::BudgetExhausted => "兑换请求超时，请稍后重试。",
        OutboundFailureKind::ProxyPolicy => "站点采集代理配置无效，请检查该站点的代理设置。",
        OutboundFailureKind::RedirectBlocked
        | OutboundFailureKind::RedirectLoop
        | OutboundFailureKind::RedirectLimitExceeded => {
            "兑换接口发生了不安全或异常重定向，请检查站点网址是否为最终访问地址。"
        }
        OutboundFailureKind::BodyLimitExceeded { .. } => "站点返回内容过大，无法确认兑换结果。",
        OutboundFailureKind::Cancelled => "兑换请求已取消。",
        OutboundFailureKind::InvalidUrl
        | OutboundFailureKind::InvalidHeader
        | OutboundFailureKind::HeaderNotAllowed(_)
        | OutboundFailureKind::TransportPolicy => "兑换请求配置无效，请重新进行窗口授权。",
        OutboundFailureKind::RetryAfterExceedsBudget | OutboundFailureKind::RequestFailed => {
            "无法连接兑换接口；若站点启用了 Cloudflare，请重新进行窗口授权。"
        }
    }
}

fn looks_like_cookie_header(value: &str) -> bool {
    value.contains('=') && value.contains(';')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::credentials::SessionResolveStatus,
        outbound::AsyncOutboundClientConfig,
        services::collectors::drivers::newapi::test_support::{json_response, TestHttpServer},
    };

    fn station(website_url: String, station_type: &str) -> Station {
        Station {
            id: "station-fixture".to_string(),
            name: "Fixture".to_string(),
            station_type: station_type.to_string(),
            website_url: website_url.clone(),
            api_base_url: website_url,
            endpoint_revision: 1,
            collector_proxy_mode: "direct".to_string(),
            collector_proxy_url: None,
            api_key_masked: String::new(),
            api_key_present: false,
            key_count: 0,
            enabled: true,
            priority: 0,
            credit_per_cny: 1.0,
            balance_raw: None,
            balance_cny: None,
            low_balance_threshold_cny: None,
            collection_interval_minutes: 60,
            status: "active".to_string(),
            latency_ms: None,
            last_checked_at: None,
            last_pricing_fetched_at: None,
            note: None,
            created_at: "0".to_string(),
            updated_at: "0".to_string(),
        }
    }

    fn sub2api_session(access_token: &str) -> ResolvedSession {
        ResolvedSession {
            status: SessionResolveStatus::Ready,
            access_token: Some(access_token.to_string()),
            refresh_token: Some("fixture-refresh".to_string()),
            cookie: None,
            newapi_user_id: None,
            message: None,
        }
    }

    #[test]
    fn parses_provider_specific_success_envelopes() {
        let sub2api = inspect_response(
            "sub2api",
            200,
            json!({"code":0,"message":"success","data":{"message":"兑换成功","value":10}}),
            "fake-code",
        );
        assert!(sub2api.success);
        assert_eq!(sub2api.message, "兑换成功");
        assert_eq!(sub2api.credited_detail.as_deref(), Some("已添加：$10.00"));

        let newapi = inspect_response(
            "newapi",
            200,
            json!({"success":true,"message":"兑换成功","data":100000}),
            "fake-code",
        );
        assert!(newapi.success);
        assert_eq!(newapi.message, "兑换成功");
        assert_eq!(
            newapi.credited_detail.as_deref(),
            Some("已添加额度：100000")
        );
    }

    #[test]
    fn localizes_common_provider_failures_and_preserves_unknown_messages() {
        for (message, expected) in [
            ("REDEEM_CODE_NOT_FOUND", "兑换码无效，请检查后重试。"),
            ("redeem code already used", "该兑换码已被使用。"),
            ("REDEEM_CODE_EXPIRED", "该兑换码已过期。"),
            (
                "too many failed attempts, please try again later",
                "尝试次数过多，请稍后再试。",
            ),
            (
                "redeem code is being processed, please try again",
                "兑换请求正在处理中，请稍后再试。",
            ),
            (
                "invitation codes can only be used during registration",
                "该兑换码类型不能在这里使用。",
            ),
            ("not authenticated", "登录状态已失效，请重新授权后再试。"),
            ("Redeem failed", "兑换码无效或已失效，请检查后重试。"),
        ] {
            let failure = inspect_response(
                "sub2api",
                409,
                json!({"code":409,"message":message}),
                "fake-code",
            );
            assert_eq!(failure.message, expected, "message: {message}");
        }

        let unknown = inspect_response(
            "newapi",
            200,
            json!({"success":false,"message":"provider-specific failure 42"}),
            "fake-code",
        );
        assert_eq!(unknown.message, "provider-specific failure 42");
    }

    #[test]
    fn distinguishes_expired_login_tokens_from_expired_redeem_codes() {
        let auth_failure = inspect_response(
            "sub2api",
            401,
            json!({"code":401,"message":"token has expired"}),
            "fake-code",
        );
        assert_eq!(auth_failure.message, "登录状态已失效，请重新授权后再试。");
        assert!(is_authentication_rejection(
            401,
            &json!({"code":401,"message":"token has expired"})
        ));

        let redeem_failure = inspect_response(
            "sub2api",
            400,
            json!({
                "code": 400,
                "message": "redeem code expired",
                "reason": "REDEEM_CODE_EXPIRED"
            }),
            "fake-code",
        );
        assert_eq!(redeem_failure.message, "该兑换码已过期。");
        assert!(!is_authentication_rejection(
            400,
            &json!({"reason":"REDEEM_CODE_EXPIRED"})
        ));
    }

    #[tokio::test]
    async fn redemption_preserves_base_path_and_surfaces_authentication_rejection() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            401,
            json!({"code":401,"message":"token has expired"}),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let attempt = redeem_station_code(
            &outbound,
            &station(format!("{}/tenant", server.base_url), "sub2api"),
            &sub2api_session("expired-access"),
            "fixture-code",
            Some("fixture-agent"),
            ProxyPolicy::Direct,
            RequestBudget::from_now(std::time::Duration::from_secs(5)),
            CancellationToken::new(),
            Some("redemption-auth-fixture".to_string()),
        )
        .await;

        assert!(attempt.authentication_rejected);
        assert_eq!(attempt.result.message, "登录状态已失效，请重新授权后再试。");
        let requests = server.finish();
        assert!(requests[0].starts_with("POST /tenant/api/v1/redeem HTTP/1.1"));
        assert!(requests[0]
            .to_ascii_lowercase()
            .contains("authorization: bearer expired-access"));
    }

    #[tokio::test]
    async fn redemption_uses_the_callers_existing_deadline() {
        let budget = RequestBudget::from_now(std::time::Duration::from_millis(1));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());

        let attempt = redeem_station_code(
            &outbound,
            &station("http://127.0.0.1:9".to_string(), "sub2api"),
            &sub2api_session("fixture-access"),
            "fixture-code",
            None,
            ProxyPolicy::Direct,
            budget,
            CancellationToken::new(),
            Some("redemption-expired-budget-fixture".to_string()),
        )
        .await;

        assert!(!attempt.result.success);
        assert_eq!(attempt.result.message, "兑换请求超时，请稍后重试。");
    }

    #[tokio::test]
    async fn sub2api_refresh_parses_rotated_session_and_preserves_base_path() {
        let body = json!({
            "access_token":"fresh-access",
            "refresh_token":"rotated-refresh",
            "expires_in":3600
        })
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: session=rotated; Path=/; HttpOnly\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        let server = TestHttpServer::sequence(vec![Some(response)]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let refreshed = refresh_sub2api_session(
            &outbound,
            &station(format!("{}/tenant", server.base_url), "sub2api"),
            "fixture-refresh",
            Some("cf_clearance=fixture; session=fixture"),
            Some("fixture-agent"),
            ProxyPolicy::Direct,
            RequestBudget::from_now(std::time::Duration::from_secs(5)),
            CancellationToken::new(),
            Some("redemption-refresh-fixture".to_string()),
        )
        .await
        .expect("refresh session");

        assert_eq!(refreshed.access_token, "fresh-access");
        assert_eq!(refreshed.refresh_token.as_deref(), Some("rotated-refresh"));
        assert_eq!(
            refreshed.cookie.as_deref(),
            Some("cf_clearance=fixture; session=rotated")
        );
        assert!(refreshed.token_expires_at.is_some());
        let requests = server.finish();
        assert!(requests[0].starts_with("POST /tenant/api/v1/auth/refresh HTTP/1.1"));
        assert!(requests[0].contains(r#"{"refresh_token":"fixture-refresh"}"#));
        assert!(requests[0]
            .to_ascii_lowercase()
            .contains("cookie: cf_clearance=fixture; session=fixture"));
    }

    #[test]
    fn redacts_submitted_code_from_upstream_messages() {
        let failure = inspect_response(
            "newapi",
            200,
            json!({"success":false,"message":"code fake-secret-code is invalid"}),
            "fake-secret-code",
        );
        assert!(!failure.success);
        assert!(!failure.message.contains("fake-secret-code"));
    }
}
