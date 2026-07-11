use serde_json::{json, Value};
use std::{
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    AuthRejected,
    AuthRefreshFailed,
    NetworkTimeout,
    RateLimited,
    Upstream5xx,
    InvalidJson,
    PermanentHttp,
    TaskBudgetExhausted,
}

impl FailureKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::AuthRejected => "auth_rejected",
            Self::AuthRefreshFailed => "auth_refresh_failed",
            Self::NetworkTimeout => "network_timeout",
            Self::RateLimited => "rate_limited",
            Self::Upstream5xx => "upstream_5xx",
            Self::InvalidJson => "invalid_json",
            Self::PermanentHttp => "permanent_http",
            Self::TaskBudgetExhausted => "task_budget_exhausted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    AuthRefresh,
    TransientRetry,
}

impl RecoveryAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::AuthRefresh => "auth_refresh",
            Self::TransientRetry => "transient_retry",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RequestPolicy {
    pub max_attempts: usize,
    pub malformed_json_max_attempts: usize,
    pub task_budget: Duration,
    pub retry_delays: Vec<Duration>,
}

#[derive(Debug, Clone)]
pub struct CollectionAttemptBudget {
    pub started_at: Instant,
    pub limit: Duration,
}

impl CollectionAttemptBudget {
    pub fn new(limit: Duration) -> Self {
        Self {
            started_at: Instant::now(),
            limit,
        }
    }

    pub fn remaining(&self) -> Option<Duration> {
        self.limit
            .checked_sub(self.started_at.elapsed())
            .filter(|remaining| !remaining.is_zero())
    }
}

#[derive(Debug, Clone)]
pub struct AttemptRecord {
    pub attempt: usize,
    pub status: Option<u16>,
    pub ok: bool,
    pub duration_ms: i64,
    pub failure_kind: Option<FailureKind>,
}

#[derive(Debug, Clone)]
pub struct EndpointJsonResult {
    pub url: String,
    pub status: Option<u16>,
    pub ok: bool,
    pub duration_ms: i64,
    pub payload: Option<Value>,
    pub error_message: Option<String>,
    pub retry_after: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct RequestExecution {
    pub path: String,
    pub result: EndpointJsonResult,
    pub attempts: Vec<AttemptRecord>,
    pub failure_kind: Option<FailureKind>,
    pub recovery_actions: Vec<RecoveryAction>,
    pub duration_ms: i64,
}

impl RequestExecution {
    pub fn to_redacted_json(&self) -> Value {
        json!({
            "url": self.result.url,
            "status": self.result.status,
            "ok": self.result.ok,
            "durationMs": self.duration_ms,
            "path": self.path,
            "attemptCount": self.attempts.len(),
            "failureKind": self.failure_kind.map(FailureKind::as_str),
            "recoveryActions": self.recovery_actions.iter().map(|action| action.as_str()).collect::<Vec<_>>(),
            "attempts": self.attempts.iter().map(|attempt| json!({
                "attempt": attempt.attempt,
                "status": attempt.status,
                "ok": attempt.ok,
                "durationMs": attempt.duration_ms,
                "failureKind": attempt.failure_kind.map(FailureKind::as_str),
            })).collect::<Vec<_>>(),
        })
    }
}

pub type AuthRefresh<'a, C> = &'a mut dyn FnMut(&C) -> Option<C>;

pub fn execute_json_request<C, R>(
    path: &str,
    mut credential: C,
    mut auth_refresh: Option<AuthRefresh<'_, C>>,
    request: &mut R,
    policy: RequestPolicy,
) -> RequestExecution
where
    R: FnMut(&C, Duration) -> EndpointJsonResult,
{
    let started_at = Instant::now();
    let budget = CollectionAttemptBudget::new(policy.task_budget);
    let max_attempts = policy.max_attempts.min(3);
    let malformed_max = policy.malformed_json_max_attempts.min(2);
    let mut attempts = Vec::new();
    let mut actions = Vec::new();
    let mut auth_refreshed = false;
    let mut malformed_attempts = 0;
    let mut last_result = budget_exhausted_result(path);
    let mut final_failure = None;

    while attempts.len() < max_attempts {
        let Some(timeout) = budget.remaining() else {
            final_failure = Some(FailureKind::TaskBudgetExhausted);
            break;
        };
        let result = request(&credential, timeout);
        let mut failure = classify(&result);
        if result.ok && result.payload.is_none() {
            failure = Some(FailureKind::InvalidJson);
            malformed_attempts += 1;
        }
        attempts.push(AttemptRecord {
            attempt: attempts.len() + 1,
            status: result.status,
            ok: result.ok && failure.is_none(),
            duration_ms: result.duration_ms,
            failure_kind: failure,
        });
        last_result = result;

        if failure.is_none() {
            final_failure = None;
            break;
        }
        if failure == Some(FailureKind::AuthRejected) {
            if auth_refreshed {
                final_failure = failure;
                break;
            }
            auth_refreshed = true;
            match auth_refresh
                .as_mut()
                .and_then(|refresh| refresh(&credential))
            {
                Some(refreshed) => {
                    credential = refreshed;
                    actions.push(RecoveryAction::AuthRefresh);
                    continue;
                }
                None => {
                    final_failure = Some(FailureKind::AuthRefreshFailed);
                    break;
                }
            }
        }

        final_failure = failure;
        let retryable = matches!(
            failure,
            Some(FailureKind::NetworkTimeout | FailureKind::RateLimited | FailureKind::Upstream5xx)
        ) || (failure == Some(FailureKind::InvalidJson)
            && malformed_attempts < malformed_max);
        if !retryable || attempts.len() >= max_attempts {
            break;
        }
        let delay = last_result.retry_after.unwrap_or_else(|| {
            policy
                .retry_delays
                .get(attempts.len() - 1)
                .copied()
                .unwrap_or_default()
        });
        let Some(remaining) = budget.remaining() else {
            final_failure = Some(FailureKind::TaskBudgetExhausted);
            break;
        };
        if delay >= remaining {
            final_failure = Some(FailureKind::TaskBudgetExhausted);
            break;
        }
        actions.push(RecoveryAction::TransientRetry);
        if !delay.is_zero() {
            thread::sleep(delay);
        }
    }

    if attempts.is_empty() {
        final_failure = Some(FailureKind::TaskBudgetExhausted);
    }
    RequestExecution {
        path: path.to_string(),
        result: last_result,
        attempts,
        failure_kind: final_failure,
        recovery_actions: actions,
        duration_ms: started_at.elapsed().as_millis() as i64,
    }
}

fn classify(result: &EndpointJsonResult) -> Option<FailureKind> {
    match result.status {
        None => Some(FailureKind::NetworkTimeout),
        Some(401 | 403) => Some(FailureKind::AuthRejected),
        Some(429) => Some(FailureKind::RateLimited),
        Some(500..=599) => Some(FailureKind::Upstream5xx),
        Some(400..=499) => Some(FailureKind::PermanentHttp),
        Some(_) if result.ok => None,
        Some(_) => Some(FailureKind::PermanentHttp),
    }
}

fn budget_exhausted_result(path: &str) -> EndpointJsonResult {
    EndpointJsonResult {
        url: path.to_string(),
        status: None,
        ok: false,
        duration_ms: 0,
        payload: None,
        error_message: Some("task budget exhausted".to_string()),
        retry_after: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn policy() -> RequestPolicy {
        RequestPolicy {
            max_attempts: 3,
            malformed_json_max_attempts: 2,
            task_budget: Duration::from_millis(50),
            retry_delays: vec![Duration::ZERO, Duration::ZERO],
        }
    }

    fn result(status: Option<u16>, payload: Option<serde_json::Value>) -> EndpointJsonResult {
        EndpointJsonResult {
            url: "https://example.test/v1/models".to_string(),
            status,
            ok: status.is_some_and(|value| (200..400).contains(&value)),
            duration_ms: 1,
            payload,
            error_message: None,
            retry_after: None,
        }
    }

    #[test]
    fn request_recovery_refreshes_auth_once_then_succeeds() {
        let mut refreshes = 0;
        let mut refresh = |_: &String| {
            refreshes += 1;
            Some("fresh-secret".to_string())
        };
        let mut request = |credential: &String, _: Duration| {
            if credential == "stale-secret" {
                result(Some(401), None)
            } else {
                result(Some(200), Some(json!({"secret": "response-body"})))
            }
        };

        let execution = execute_json_request(
            "/v1/models",
            "stale-secret".to_string(),
            Some(&mut refresh),
            &mut request,
            policy(),
        );

        assert!(execution.result.ok);
        assert_eq!(execution.attempts.len(), 2);
        assert_eq!(refreshes, 1);
        assert_eq!(
            execution.recovery_actions,
            vec![RecoveryAction::AuthRefresh]
        );
        let redacted = execution.to_redacted_json().to_string();
        assert!(!redacted.contains("stale-secret"));
        assert!(!redacted.contains("fresh-secret"));
        assert!(!redacted.contains("response-body"));
    }

    #[test]
    fn request_recovery_stops_when_retry_after_exceeds_budget() {
        let mut calls = 0;
        let mut request = |_: &String, _: Duration| {
            calls += 1;
            EndpointJsonResult {
                retry_after: Some(Duration::from_secs(1)),
                ..result(Some(429), None)
            }
        };

        let execution = execute_json_request(
            "/v1/models",
            "secret".to_string(),
            None,
            &mut request,
            policy(),
        );

        assert_eq!(calls, 1);
        assert_eq!(
            execution.failure_kind,
            Some(FailureKind::TaskBudgetExhausted)
        );
    }

    #[test]
    fn request_recovery_classifies_endpoint_outcomes() {
        let cases = [
            (None, false, None, Some(FailureKind::NetworkTimeout)),
            (Some(429), false, None, Some(FailureKind::RateLimited)),
            (Some(502), false, None, Some(FailureKind::Upstream5xx)),
            (Some(400), false, None, Some(FailureKind::PermanentHttp)),
            (Some(404), false, None, Some(FailureKind::PermanentHttp)),
            (Some(422), false, None, Some(FailureKind::PermanentHttp)),
            (Some(200), true, None, Some(FailureKind::InvalidJson)),
            (Some(200), true, Some(json!({"ok": true})), None),
        ];

        for (status, ok, payload, expected) in cases {
            let mut policy = policy();
            policy.max_attempts = 1;
            let mut request = |_: &String, _: Duration| EndpointJsonResult {
                ok,
                ..result(status, payload.clone())
            };
            let execution = execute_json_request(
                "/v1/models",
                "secret".to_string(),
                None,
                &mut request,
                policy,
            );
            assert_eq!(execution.failure_kind, expected, "status {status:?}");
        }
    }

    #[test]
    fn request_recovery_caps_transient_attempts_at_three() {
        let mut calls = 0;
        let mut request = |_: &String, _: Duration| {
            calls += 1;
            result(Some(502), None)
        };
        let mut policy = policy();
        policy.max_attempts = 10;

        let execution = execute_json_request(
            "/v1/models",
            "secret".to_string(),
            None,
            &mut request,
            policy,
        );

        assert_eq!(calls, 3);
        assert_eq!(execution.attempts.len(), 3);
        assert_eq!(execution.failure_kind, Some(FailureKind::Upstream5xx));
    }

    #[test]
    fn request_recovery_caps_malformed_json_attempts_at_two() {
        let mut calls = 0;
        let mut request = |_: &String, _: Duration| {
            calls += 1;
            result(Some(200), None)
        };

        let execution = execute_json_request(
            "/v1/models",
            "secret".to_string(),
            None,
            &mut request,
            policy(),
        );

        assert_eq!(calls, 2);
        assert_eq!(execution.failure_kind, Some(FailureKind::InvalidJson));
    }

    #[test]
    fn request_recovery_does_not_request_after_budget_exhaustion() {
        let mut calls = 0;
        let mut request = |_: &String, _: Duration| {
            calls += 1;
            result(Some(200), Some(json!({"ok": true})))
        };
        let mut policy = policy();
        policy.task_budget = Duration::ZERO;

        let execution = execute_json_request(
            "/v1/models",
            "secret".to_string(),
            None,
            &mut request,
            policy,
        );

        assert_eq!(calls, 0);
        assert!(execution.attempts.is_empty());
        assert_eq!(
            execution.failure_kind,
            Some(FailureKind::TaskBudgetExhausted)
        );
    }

    #[test]
    fn request_recovery_stops_when_refreshed_credential_is_rejected() {
        let mut refreshes = 0;
        let mut refresh = |_: &String| {
            refreshes += 1;
            Some("fresh-secret".to_string())
        };
        let mut request = |_: &String, _: Duration| result(Some(401), None);

        let execution = execute_json_request(
            "/v1/models",
            "stale-secret".to_string(),
            Some(&mut refresh),
            &mut request,
            policy(),
        );

        assert_eq!(refreshes, 1);
        assert_eq!(execution.attempts.len(), 2);
        assert_eq!(execution.failure_kind, Some(FailureKind::AuthRejected));
    }
}
