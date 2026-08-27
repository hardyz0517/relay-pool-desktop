use std::time::Duration;

use http::{header, HeaderName, HeaderValue, Method};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
    models::{
        credentials::ResolvedSession, station_redemption::StationRedemptionResult,
        stations::Station,
    },
    outbound::{
        AsyncOutboundClient, OutboundFailureKind, OutboundHeaderPolicy, OutboundHeaders,
        OutboundRequest, OutboundRetryPolicy, ProxyPolicy, RequestBudget, SecretHeaderValue,
    },
};

const NEW_API_USER_HEADER: HeaderName = HeaderName::from_static("new-api-user");
const SUB2API_USER_UI_REQUEST_HEADER: HeaderName = HeaderName::from_static("x-user-ui-request");
const MAX_UPSTREAM_MESSAGE_CHARS: usize = 512;

pub(crate) async fn redeem_station_code(
    outbound: &AsyncOutboundClient,
    station: &Station,
    session: &ResolvedSession,
    code: &str,
    user_agent: Option<&str>,
    proxy: ProxyPolicy,
    timeout: Duration,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> StationRedemptionResult {
    let provider = station.station_type.to_ascii_lowercase();
    let request = match build_request(
        station,
        session,
        code,
        user_agent,
        proxy,
        timeout,
        correlation_id,
    ) {
        Ok(request) => request,
        Err(message) => return result(&provider, false, message),
    };

    let response = match outbound.execute(request, cancellation).await {
        Ok(response) => response,
        Err(error) => return result(&provider, false, outbound_failure_message(&error.kind)),
    };
    let payload = serde_json::from_slice::<Value>(&response.body).unwrap_or(Value::Null);
    inspect_response(&provider, response.status.is_success(), payload, code)
}

fn build_request(
    station: &Station,
    session: &ResolvedSession,
    code: &str,
    user_agent: Option<&str>,
    proxy: ProxyPolicy,
    timeout: Duration,
    correlation_id: Option<String>,
) -> Result<OutboundRequest, &'static str> {
    let provider = station.station_type.to_ascii_lowercase();
    let (path, body) = match provider.as_str() {
        "sub2api" => ("/api/v1/redeem", json!({ "code": code })),
        "newapi" => ("/api/user/topup", json!({ "key": code })),
        _ => return Err("当前站点类型不支持兑换码。"),
    };
    let mut url = Url::parse(&station.website_url).map_err(|_| "站点地址无效，无法兑换。")?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        return Err("站点地址无效，无法兑换。");
    }
    url.set_path(path);
    url.set_query(None);
    url.set_fragment(None);

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
        url: url.into(),
        correlation_id,
        headers,
        body: serde_json::to_vec(&body).map_err(|_| "无法创建兑换请求。")?,
        proxy,
        budget: RequestBudget::from_now(timeout),
        retry_policy: OutboundRetryPolicy::Never,
    })
}

fn inspect_response(
    provider: &str,
    http_success: bool,
    payload: Value,
    submitted_code: &str,
) -> StationRedemptionResult {
    let success = match provider {
        "sub2api" => http_success && payload.get("code").and_then(Value::as_i64) == Some(0),
        "newapi" => http_success && payload.get("success").and_then(Value::as_bool) == Some(true),
        _ => false,
    };
    let message = response_message(provider, &payload);
    let fallback = if success {
        "兑换成功。"
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

fn response_message<'a>(provider: &str, payload: &'a Value) -> Option<&'a str> {
    let candidates = if provider == "sub2api" {
        [
            "/data/message",
            "/message",
            "/error/message",
            "/error/code",
            "/data/code",
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
        &["redeem_code_expired", "code expired", "has expired"],
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
        &[
            "not authenticated",
            "unauthorized",
            "not logged",
            "login required",
        ],
    ) {
        "登录状态已失效，请重新授权后再试。"
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

    #[test]
    fn parses_provider_specific_success_envelopes() {
        let sub2api = inspect_response(
            "sub2api",
            true,
            json!({"code":0,"message":"success","data":{"message":"兑换成功","value":10}}),
            "fake-code",
        );
        assert!(sub2api.success);
        assert_eq!(sub2api.message, "兑换成功");
        assert_eq!(sub2api.credited_detail.as_deref(), Some("已添加：$10.00"));

        let newapi = inspect_response(
            "newapi",
            true,
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
                false,
                json!({"code":409,"message":message}),
                "fake-code",
            );
            assert_eq!(failure.message, expected, "message: {message}");
        }

        let unknown = inspect_response(
            "newapi",
            true,
            json!({"success":false,"message":"provider-specific failure 42"}),
            "fake-code",
        );
        assert_eq!(unknown.message, "provider-specific failure 42");
    }

    #[test]
    fn redacts_submitted_code_from_upstream_messages() {
        let failure = inspect_response(
            "newapi",
            true,
            json!({"success":false,"message":"code fake-secret-code is invalid"}),
            "fake-secret-code",
        );
        assert!(!failure.success);
        assert!(!failure.message.contains("fake-secret-code"));
    }
}
