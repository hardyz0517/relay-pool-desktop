#[cfg(test)]
use super::{
    routing_failure::{
        ClassifiedRouteFailure, RouteFailureAction, RouteFailureKind, RouteFailureScope,
    },
    routing_types::RouteHealthState,
};
use crate::models::routing::StationKeyHealth;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct RouteHealthTransition {
    pub(crate) state: RouteHealthState,
    pub(crate) cooldown_until_ms: Option<i64>,
}

#[cfg(test)]
pub(crate) fn apply_health_transition(
    current: RouteHealthState,
    failure: &ClassifiedRouteFailure,
    consecutive_failures: i64,
    now_ms: i64,
) -> RouteHealthTransition {
    if failure.scope == RouteFailureScope::RequestOnly {
        return RouteHealthTransition {
            state: current,
            cooldown_until_ms: None,
        };
    }

    match failure.action {
        RouteFailureAction::IgnoreForKeyHealth => RouteHealthTransition {
            state: current,
            cooldown_until_ms: None,
        },
        RouteFailureAction::HardFail => RouteHealthTransition {
            state: RouteHealthState::Offline,
            cooldown_until_ms: Some(now_ms + 15 * 60 * 1000),
        },
        RouteFailureAction::Cooldown => RouteHealthTransition {
            state: RouteHealthState::Cooldown,
            cooldown_until_ms: Some(now_ms + failure.retry_after_ms.unwrap_or(5 * 60 * 1000)),
        },
        RouteFailureAction::Observe => {
            if consecutive_failures >= 3 {
                RouteHealthTransition {
                    state: RouteHealthState::Cooldown,
                    cooldown_until_ms: Some(now_ms + 2 * 60 * 1000),
                }
            } else {
                RouteHealthTransition {
                    state: RouteHealthState::Degraded,
                    cooldown_until_ms: None,
                }
            }
        }
    }
}

pub(crate) fn error_summary_indicates_offline(summary: &str) -> bool {
    let lower = summary.to_ascii_lowercase();
    lower.contains("auth_error")
        || lower.contains("insufficient_balance")
        || lower.contains("http 401")
        || lower.contains("http 402")
        || lower.contains("http 403")
}

pub(crate) fn health_is_blocked(health: Option<&StationKeyHealth>, now_ms: i64) -> bool {
    let Some(health) = health else {
        return false;
    };
    let cooldown_active = health
        .cooldown_until
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .is_some_and(|until_ms| until_ms > now_ms);
    let offline = health
        .last_error_summary
        .as_deref()
        .is_some_and(error_summary_indicates_offline);
    cooldown_active || offline
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_ambiguous_failure_moves_healthy_key_to_observing() {
        let transition = apply_health_transition(
            RouteHealthState::Ready,
            &ClassifiedRouteFailure::timeout_observe(),
            1,
            0,
        );

        assert_eq!(transition.state, RouteHealthState::Degraded);
        assert_eq!(transition.cooldown_until_ms, None);
    }

    #[test]
    fn repeated_ambiguous_failures_move_to_cooling() {
        let transition = apply_health_transition(
            RouteHealthState::Degraded,
            &ClassifiedRouteFailure::timeout_observe(),
            3,
            1000,
        );

        assert_eq!(transition.state, RouteHealthState::Cooldown);
        assert!(transition
            .cooldown_until_ms
            .is_some_and(|value| value > 1000));
    }

    #[test]
    fn rate_limit_uses_retry_after_for_cooldown() {
        let failure = ClassifiedRouteFailure {
            kind: RouteFailureKind::RateLimited,
            action: RouteFailureAction::Cooldown,
            scope: RouteFailureScope::KeyHealth,
            retryable_before_output: true,
            retry_after_ms: Some(90_000),
        };

        let transition = apply_health_transition(RouteHealthState::Ready, &failure, 1, 1_000);

        assert_eq!(transition.state, RouteHealthState::Cooldown);
        assert_eq!(transition.cooldown_until_ms, Some(91_000));
    }

    #[test]
    fn rate_limit_honors_short_retry_after_without_minimum_floor() {
        let failure = ClassifiedRouteFailure {
            kind: RouteFailureKind::RateLimited,
            action: RouteFailureAction::Cooldown,
            scope: RouteFailureScope::KeyHealth,
            retryable_before_output: true,
            retry_after_ms: Some(10_000),
        };

        let transition = apply_health_transition(RouteHealthState::Ready, &failure, 1, 1_000);

        assert_eq!(transition.cooldown_until_ms, Some(11_000));
    }

    #[test]
    fn hard_failure_summary_marks_key_offline() {
        assert!(error_summary_indicates_offline(
            "auth_error: upstream returned HTTP 401"
        ));
        assert!(error_summary_indicates_offline(
            "insufficient_balance: upstream returned HTTP 402"
        ));
        assert!(!error_summary_indicates_offline(
            "temporary_network: upstream timeout"
        ));
    }

    #[test]
    fn health_block_uses_current_time_instead_of_a_fixed_epoch_threshold() {
        let mut health = station_key_health();
        health.cooldown_until = Some("61000".to_string());
        assert!(health_is_blocked(Some(&health), 60_000));

        health.cooldown_until = Some("59999".to_string());
        assert!(!health_is_blocked(Some(&health), 60_000));

        health.cooldown_until = Some("invalid".to_string());
        assert!(!health_is_blocked(Some(&health), 60_000));
    }

    #[test]
    fn explicit_offline_health_is_blocked() {
        let mut health = station_key_health();
        health.last_error_summary = Some("connection refused http 401".to_string());
        assert!(health_is_blocked(Some(&health), 60_000));
    }

    fn station_key_health() -> crate::models::routing::StationKeyHealth {
        crate::models::routing::StationKeyHealth {
            station_key_id: "key".to_string(),
            last_success_at: None,
            last_failure_at: None,
            consecutive_failures: 0,
            success_count: 0,
            failure_count: 0,
            avg_latency_ms: None,
            last_error_summary: None,
            cooldown_until: None,
            updated_at: "0".to_string(),
        }
    }
}
