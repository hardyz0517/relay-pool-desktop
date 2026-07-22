use std::{
    thread,
    time::{Duration, Instant},
};

use serde_json::{json, Value};
use ureq::Agent;

use crate::services::outbound::{credential_agent_builder_for_proxy, ProxyConfig};

const LOGIN_PATHS: [&str; 3] = ["/api/v1/auth/login", "/auth/login", "/api/login"];

#[derive(Clone, Copy)]
struct LoginConfig {
    connect_timeout: Duration,
    read_timeout: Duration,
}

impl Default for LoginConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_millis(1_200),
            read_timeout: Duration::from_millis(2_200),
        }
    }
}

struct LoginAttempt {
    token: Option<String>,
    login_message: Option<String>,
    manual_required: Option<String>,
}

pub(crate) struct LoginProbeOutcome {
    pub token_present: bool,
    pub login_message: Option<String>,
    pub manual_required: Option<String>,
}

pub(crate) struct LoginTokenOutcome {
    pub access_token: Option<String>,
}

pub(crate) fn test_login_credentials(
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<LoginProbeOutcome, String> {
    let config = LoginConfig::default();
    let agent = credential_agent_builder_for_proxy(&ProxyConfig::direct())?
        .timeout_connect(config.connect_timeout)
        .timeout_read(config.read_timeout)
        .timeout_write(config.connect_timeout)
        .build();
    let attempt = attempt_login(&agent, base_url, username, password)?;
    Ok(LoginProbeOutcome {
        token_present: attempt.token.is_some(),
        login_message: attempt.login_message,
        manual_required: attempt.manual_required,
    })
}

pub(crate) fn login_access_token_with_proxy(
    base_url: &str,
    username: &str,
    password: &str,
    proxy: &ProxyConfig,
) -> Result<LoginTokenOutcome, String> {
    let config = LoginConfig::default();
    let agent = credential_agent_builder_for_proxy(proxy)?
        .timeout_connect(config.connect_timeout)
        .timeout_read(config.read_timeout)
        .timeout_write(config.connect_timeout)
        .build();
    let attempt = attempt_login(&agent, base_url, username, password)?;
    Ok(LoginTokenOutcome {
        access_token: attempt.token,
    })
}

pub(crate) fn login_access_token_with_budget_and_proxy(
    base_url: &str,
    username: &str,
    password: &str,
    budget: Duration,
    proxy: &ProxyConfig,
) -> Result<LoginTokenOutcome, String> {
    let deadline = LoginAttemptDeadline::new(budget);
    let attempt = attempt_login_with_budget(
        base_url,
        username,
        password,
        LoginConfig::default(),
        &deadline,
        proxy,
    )?;
    Ok(LoginTokenOutcome {
        access_token: attempt.token,
    })
}

fn attempt_login(
    agent: &Agent,
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<LoginAttempt, String> {
    for path in LOGIN_PATHS {
        for field in ["email", "username", "user"] {
            let url = join_url(base_url, path);
            let response = match agent
                .post(&url)
                .send_json(json!({ field: username, "password": password }))
            {
                Ok(response) => response,
                Err(ureq::Error::Status(_, response)) => response,
                Err(error) => return Err(format!("login request failed: {error}")),
            };
            let status = response.status();
            let body = response
                .into_string()
                .map_err(|error| format!("failed to read login response: {error}"))?;
            if let Ok(parsed) = serde_json::from_str::<Value>(&body) {
                let attempt = login_attempt_from_response(path, status, &parsed);
                if attempt.token.is_some() || attempt.manual_required.is_some() {
                    return Ok(attempt);
                }
            }
            if (200..300).contains(&status) {
                return Ok(missing_token_attempt());
            }
        }
    }
    Ok(rejected_login_attempt())
}

struct LoginAttemptDeadline {
    started_at: Instant,
    budget: Duration,
}

impl LoginAttemptDeadline {
    fn new(budget: Duration) -> Self {
        Self {
            started_at: Instant::now(),
            budget,
        }
    }

    fn remaining(&self) -> Result<Duration, String> {
        self.budget
            .checked_sub(self.started_at.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| "task_budget_exhausted".to_string())
    }
}

fn attempt_login_with_budget(
    base_url: &str,
    username: &str,
    password: &str,
    config: LoginConfig,
    deadline: &LoginAttemptDeadline,
    proxy: &ProxyConfig,
) -> Result<LoginAttempt, String> {
    for path in LOGIN_PATHS {
        for field in ["email", "username", "user"] {
            let url = join_url(base_url, path);
            let payload = json!({ field: username, "password": password });
            let mut transient_attempted = false;
            loop {
                match login_candidate_request(&url, payload.clone(), config, deadline, proxy)? {
                    LoginCandidateResponse::Response(response) => {
                        let response = *response;
                        let status = response.status();
                        if status == 429 {
                            if transient_attempted {
                                return Ok(transient_login_attempt("rate_limited"));
                            }
                            transient_attempted = true;
                            let delay = retry_after_duration(&response).unwrap_or_default();
                            if delay >= deadline.remaining()? {
                                return Err("task_budget_exhausted".to_string());
                            }
                            if !delay.is_zero() {
                                thread::sleep(delay);
                            }
                            continue;
                        }
                        if (500..=599).contains(&status) {
                            if transient_attempted {
                                return Ok(transient_login_attempt("upstream_5xx"));
                            }
                            transient_attempted = true;
                            continue;
                        }
                        let body = response
                            .into_string()
                            .map_err(|error| format!("failed to read login response: {error}"))?;
                        if let Ok(parsed) = serde_json::from_str::<Value>(&body) {
                            let attempt = login_attempt_from_response(path, status, &parsed);
                            if attempt.token.is_some() || attempt.manual_required.is_some() {
                                return Ok(attempt);
                            }
                        }
                        if (200..300).contains(&status) {
                            return Ok(missing_token_attempt());
                        }
                        break;
                    }
                    LoginCandidateResponse::Transient(kind) => {
                        if transient_attempted {
                            return Ok(transient_login_attempt(&kind));
                        }
                        transient_attempted = true;
                    }
                }
            }
        }
    }
    Ok(rejected_login_attempt())
}

enum LoginCandidateResponse {
    Response(Box<ureq::Response>),
    Transient(String),
}

fn login_candidate_request(
    url: &str,
    payload: Value,
    config: LoginConfig,
    deadline: &LoginAttemptDeadline,
    proxy: &ProxyConfig,
) -> Result<LoginCandidateResponse, String> {
    let remaining = deadline.remaining()?;
    let timeout = remaining
        .min(config.read_timeout)
        .max(Duration::from_millis(1));
    let connect_timeout = timeout
        .min(config.connect_timeout)
        .max(Duration::from_millis(1));
    let agent = credential_agent_builder_for_proxy(proxy)?
        .timeout_connect(connect_timeout)
        .timeout_read(timeout)
        .timeout_write(connect_timeout)
        .build();
    match agent.post(url).send_json(payload) {
        Ok(response) => Ok(LoginCandidateResponse::Response(Box::new(response))),
        Err(ureq::Error::Status(_, response)) => {
            Ok(LoginCandidateResponse::Response(Box::new(response)))
        }
        Err(error) if deadline.remaining().is_err() => {
            let _ = error;
            Err("task_budget_exhausted".to_string())
        }
        Err(error) => Ok(LoginCandidateResponse::Transient(format!(
            "network_timeout:{error}"
        ))),
    }
}

fn login_attempt_from_response(path: &str, status: u16, parsed: &Value) -> LoginAttempt {
    if let Some(token) = extract_token(parsed) {
        return LoginAttempt {
            token: Some(token),
            login_message: Some(format!("login token received from {path}")),
            manual_required: None,
        };
    }
    if is_region_restricted_login(parsed, status) {
        return LoginAttempt {
            token: None,
            login_message: Some(shorten_error(&parsed.to_string())),
            manual_required: Some(
                "login is region restricted; configure a collector proxy and retry".to_string(),
            ),
        };
    }
    if needs_manual_login(parsed, status) {
        return LoginAttempt {
            token: None,
            login_message: Some(shorten_error(&parsed.to_string())),
            manual_required: Some(
                "captcha, 2FA, or another interactive login step is required".to_string(),
            ),
        };
    }
    LoginAttempt {
        token: None,
        login_message: Some(shorten_error(&parsed.to_string())),
        manual_required: None,
    }
}

fn missing_token_attempt() -> LoginAttempt {
    LoginAttempt {
        token: None,
        login_message: Some("login succeeded but the response contained no token".to_string()),
        manual_required: Some("the login response contained no usable token".to_string()),
    }
}

fn rejected_login_attempt() -> LoginAttempt {
    LoginAttempt {
        token: None,
        login_message: Some("no login endpoint returned a usable token".to_string()),
        manual_required: Some(
            "credentials were rejected or the login contract changed".to_string(),
        ),
    }
}

fn transient_login_attempt(kind: &str) -> LoginAttempt {
    LoginAttempt {
        token: None,
        login_message: Some(kind.to_string()),
        manual_required: Some(kind.to_string()),
    }
}

fn is_region_restricted_login(value: &Value, status: u16) -> bool {
    if status != 403 {
        return false;
    }
    let text = value.to_string().to_lowercase();
    text.contains("region_restricted") || text.contains("region")
}

fn retry_after_duration(response: &ureq::Response) -> Option<Duration> {
    response
        .header("retry-after")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn extract_token(value: &Value) -> Option<String> {
    value
        .get("access_token")
        .or_else(|| value.get("token"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| value.get("data").and_then(extract_token))
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

fn join_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/'),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_nested_access_tokens() {
        assert_eq!(
            extract_token(&json!({"data": {"access_token": "fresh-token"}})).as_deref(),
            Some("fresh-token"),
        );
    }

    #[test]
    fn classifies_interactive_and_region_restricted_failures() {
        let captcha = login_attempt_from_response(
            LOGIN_PATHS[0],
            400,
            &json!({"reason": "GEETEST_VERIFICATION_FAILED"}),
        );
        assert!(captcha.manual_required.is_some());

        let region = login_attempt_from_response(
            LOGIN_PATHS[0],
            403,
            &json!({"reason": "REGION_RESTRICTED"}),
        );
        assert!(region
            .manual_required
            .as_deref()
            .is_some_and(|message| message.contains("proxy")));
    }

    #[test]
    fn zero_budget_fails_closed_before_network_access() {
        let result = login_access_token_with_budget_and_proxy(
            "http://127.0.0.1:1",
            "user@example.test",
            "secret",
            Duration::ZERO,
            &ProxyConfig::direct(),
        );
        assert!(matches!(result, Err(error) if error == "task_budget_exhausted"));
    }
}
