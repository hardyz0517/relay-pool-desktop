mod monitoring {
    #![allow(dead_code, unused_imports)]

    #[path = "../../src/models/monitoring/definition.rs"]
    pub mod definition;
    #[path = "../../src/models/monitoring/execution.rs"]
    pub mod execution;
    #[path = "../../src/models/monitoring/outcome.rs"]
    pub mod outcome;
    #[path = "../../src/models/monitoring/policy.rs"]
    pub mod policy;

    pub use definition::{
        ClientProfileId, ClientProfileRef, DefinitionRevision, MonitorDefinition,
        MonitorDefinitionDraft, TargetScopeKind,
    };
    pub use execution::{
        AttemptOrdinal, AttemptRole, AvailabilitySummary, ExecutionSummary, MonitorExecutionStatus,
        MonitorTargetResult, ProbeAttempt,
    };
    pub use outcome::{FailureKind, ProbeOutcome, ProtocolKind, SemanticConfidence};
    pub use policy::{
        HealthPolicy, HealthWritebackMode, RetryPolicy, RiskPolicy, SchedulePolicy,
        DEFAULT_MONITOR_ATTEMPT_TIMEOUT_MS, DEFAULT_MONITOR_EXECUTION_TIMEOUT_MS,
        DEFAULT_MONITOR_SLOW_LATENCY_THRESHOLD_MS,
    };
}

use monitoring::{
    AttemptOrdinal, AttemptRole, AvailabilitySummary, ClientProfileId, ClientProfileRef,
    DefinitionRevision, ExecutionSummary, FailureKind, HealthPolicy, HealthWritebackMode,
    MonitorDefinition, MonitorDefinitionDraft, MonitorExecutionStatus, MonitorTargetResult,
    ProbeAttempt, ProbeOutcome, ProtocolKind, RetryPolicy, RiskPolicy, SchedulePolicy,
    SemanticConfidence, TargetScopeKind, DEFAULT_MONITOR_ATTEMPT_TIMEOUT_MS,
    DEFAULT_MONITOR_EXECUTION_TIMEOUT_MS, DEFAULT_MONITOR_SLOW_LATENCY_THRESHOLD_MS,
};

fn profile(id: ClientProfileId) -> ClientProfileRef {
    ClientProfileRef::new(id, 1).expect("profile")
}

fn schedule() -> SchedulePolicy {
    SchedulePolicy::new(
        300,
        30,
        DEFAULT_MONITOR_EXECUTION_TIMEOUT_MS as i64,
        DEFAULT_MONITOR_ATTEMPT_TIMEOUT_MS as i64,
        DEFAULT_MONITOR_SLOW_LATENCY_THRESHOLD_MS as i64,
    )
    .expect("schedule")
}

fn draft() -> MonitorDefinitionDraft {
    MonitorDefinitionDraft {
        id: "monitor-1".to_string(),
        revision: DefinitionRevision(1),
        target_scope: TargetScopeKind::StationKey,
        station_id: Some("station-1".to_string()),
        station_key_id: Some("key-1".to_string()),
        protocol_kind: ProtocolKind::OpenAiResponses,
        client_profile: profile(ClientProfileId::StandardApi),
        primary_model: "gpt-4.1-mini".to_string(),
        fallback_models: vec!["gpt-4.1".to_string()],
        schedule_policy: schedule(),
        retry_policy: RetryPolicy::new(2, 200, 2_000).expect("retry"),
        risk_policy: RiskPolicy::new(200).expect("risk"),
        health_policy: HealthPolicy::new(HealthWritebackMode::ObserveOnly, 2, 2).expect("health"),
    }
}

fn attempt(
    id: &str,
    key: &str,
    model: &str,
    role: AttemptRole,
    ordinal: u8,
    outcome: ProbeOutcome,
    failure_kind: Option<FailureKind>,
) -> ProbeAttempt {
    ProbeAttempt::new(
        id,
        key,
        model,
        role,
        AttemptOrdinal(ordinal),
        outcome,
        failure_kind,
    )
    .expect("attempt")
}

#[test]
fn definition_rejects_scope_key_mismatch_empty_primary_fallback_limit_and_profile_conflict() {
    let mut station_scope_with_key = draft();
    station_scope_with_key.target_scope = TargetScopeKind::Station;
    assert!(MonitorDefinition::from_draft(station_scope_with_key).is_err());

    let mut missing_key = draft();
    missing_key.station_key_id = None;
    assert!(MonitorDefinition::from_draft(missing_key).is_err());

    let mut empty_primary = draft();
    empty_primary.primary_model = " ".to_string();
    assert!(MonitorDefinition::from_draft(empty_primary).is_err());

    let mut too_many_fallbacks = draft();
    too_many_fallbacks.fallback_models = vec![
        "m1".to_string(),
        "m2".to_string(),
        "m3".to_string(),
        "m4".to_string(),
    ];
    assert!(MonitorDefinition::from_draft(too_many_fallbacks).is_err());

    let mut conflict = draft();
    conflict.protocol_kind = ProtocolKind::AnthropicMessages;
    conflict.client_profile = profile(ClientProfileId::CodexCliCompat);
    assert!(MonitorDefinition::from_draft(conflict).is_err());
}

#[test]
fn definition_deduplicates_fallbacks_and_proves_primary_attempt_can_fit_deadline() {
    let mut input = draft();
    input.fallback_models = vec![
        "gpt-4.1-mini".to_string(),
        "fallback-a".to_string(),
        "fallback-a".to_string(),
        "fallback-b".to_string(),
    ];

    let definition = MonitorDefinition::from_draft(input).expect("definition");

    assert_eq!(definition.fallback_models, ["fallback-a", "fallback-b"]);
    assert_eq!(definition.theoretical_max_attempts(), 9);
    assert!(definition.primary_attempt_fits_deadline());
}

#[test]
fn schedule_and_retry_policies_reject_negative_or_out_of_bounds_values() {
    assert!(SchedulePolicy::new(-1, 0, 30_000, 10_000, 5_000).is_err());
    assert!(SchedulePolicy::new(300, -1, 30_000, 10_000, 5_000).is_err());
    assert!(SchedulePolicy::new(300, 76, 30_000, 10_000, 5_000).is_err());
    assert!(SchedulePolicy::new(3_000, 601, 30_000, 10_000, 5_000).is_err());
    assert!(SchedulePolicy::new(300, 30, 10_000, 10_000, 5_000).is_err());
    assert!(RetryPolicy::new(0, 200, 2_000).is_err());
    assert!(RetryPolicy::new(4, 200, 2_000).is_err());
    assert!(RetryPolicy::new(1, 2_000, 200).is_err());
}

#[test]
fn schedule_default_matches_sub2api_monitor_latency_policy() {
    let schedule = SchedulePolicy::default();
    assert_eq!(schedule.attempt_timeout_ms, 45_000);
    assert_eq!(schedule.execution_timeout_ms, 60_000);
    assert_eq!(schedule.slow_latency_threshold_ms, 6_000);
}

#[test]
fn target_result_enforces_attempt_ownership_and_zero_attempt_skips() {
    let owned = attempt(
        "attempt-1",
        "key-1",
        "gpt-4.1-mini",
        AttemptRole::Primary,
        0,
        ProbeOutcome::Available,
        None,
    );
    let result = MonitorTargetResult::from_attempts(
        "execution-1",
        "station-1",
        "key-1",
        ProtocolKind::OpenAiResponses,
        &[owned],
    )
    .expect("target result");
    assert_eq!(result.decisive_attempt_id.as_deref(), Some("attempt-1"));

    let foreign_attempt = attempt(
        "attempt-2",
        "key-other",
        "gpt-4.1-mini",
        AttemptRole::Primary,
        0,
        ProbeOutcome::Unavailable,
        Some(FailureKind::Auth),
    );
    assert!(MonitorTargetResult::from_attempts(
        "execution-1",
        "station-1",
        "key-1",
        ProtocolKind::OpenAiResponses,
        &[foreign_attempt],
    )
    .is_err());

    assert!(MonitorTargetResult::from_attempts(
        "execution-1",
        "station-1",
        "key-1",
        ProtocolKind::OpenAiResponses,
        &[],
    )
    .is_err());
    let skipped = MonitorTargetResult::skipped(
        "execution-1",
        "station-1",
        "key-1",
        ProtocolKind::OpenAiResponses,
        FailureKind::NeedsConfiguration,
    )
    .expect("skipped");
    assert_eq!(skipped.terminal_outcome, ProbeOutcome::Skipped);
    assert_eq!(skipped.attempt_count, 0);
}

#[test]
fn retry_or_fallback_success_becomes_degraded_and_skipped_is_not_denominator() {
    let first = attempt(
        "attempt-1",
        "key-1",
        "gpt-4.1-mini",
        AttemptRole::Primary,
        0,
        ProbeOutcome::Unavailable,
        Some(FailureKind::RateLimit),
    );
    let fallback = attempt(
        "attempt-2",
        "key-1",
        "gpt-4.1",
        AttemptRole::Fallback { index: 0 },
        0,
        ProbeOutcome::Available,
        None,
    );
    let recovered = MonitorTargetResult::from_attempts(
        "execution-1",
        "station-1",
        "key-1",
        ProtocolKind::OpenAiResponses,
        &[first, fallback],
    )
    .expect("recovered");
    assert_eq!(recovered.terminal_outcome, ProbeOutcome::Degraded);
    assert_eq!(
        recovered.terminal_failure_kind,
        Some(FailureKind::RecoveredAfterRetry)
    );
    assert!(recovered.used_fallback);

    let skipped = MonitorTargetResult::skipped(
        "execution-1",
        "station-1",
        "key-2",
        ProtocolKind::OpenAiResponses,
        FailureKind::NeedsConfiguration,
    )
    .expect("skipped");
    let summary = AvailabilitySummary::from_target_results(&[recovered, skipped]);
    assert_eq!(summary.denominator, 1);
    assert_eq!(summary.route_available_count, 1);
    assert_eq!(summary.availability_percent, Some(100.0));
}

#[test]
fn execution_summary_is_order_insensitive_and_rejects_duplicate_targets() {
    let available = MonitorTargetResult::from_attempts(
        "execution-1",
        "station-1",
        "key-1",
        ProtocolKind::OpenAiResponses,
        &[attempt(
            "attempt-1",
            "key-1",
            "gpt-4.1-mini",
            AttemptRole::Primary,
            0,
            ProbeOutcome::Available,
            None,
        )],
    )
    .expect("available");
    let unavailable = MonitorTargetResult::from_attempts(
        "execution-1",
        "station-1",
        "key-2",
        ProtocolKind::OpenAiResponses,
        &[attempt(
            "attempt-2",
            "key-2",
            "gpt-4.1-mini",
            AttemptRole::Primary,
            0,
            ProbeOutcome::Unavailable,
            Some(FailureKind::Timeout),
        )],
    )
    .expect("unavailable");

    let first = ExecutionSummary::from_target_results(
        "execution-1",
        2,
        &[available.clone(), unavailable.clone()],
    )
    .expect("summary");
    let second = ExecutionSummary::from_target_results(
        "execution-1",
        2,
        &[unavailable.clone(), available.clone()],
    )
    .expect("summary");

    assert_eq!(first.status, MonitorExecutionStatus::Completed);
    assert_eq!(first.available_count, second.available_count);
    assert_eq!(first.unavailable_count, second.unavailable_count);
    assert_eq!(first.summary_outcome, second.summary_outcome);
    assert!(ExecutionSummary::from_target_results(
        "execution-1",
        2,
        &[available.clone(), available],
    )
    .is_err());
}

#[test]
fn legacy_http_only_results_do_not_allow_authoritative_health_writeback() {
    let legacy = attempt(
        "attempt-1",
        "key-1",
        "gpt-4.1-mini",
        AttemptRole::Primary,
        0,
        ProbeOutcome::Available,
        None,
    )
    .legacy_http_only();
    let result = MonitorTargetResult::from_attempts(
        "execution-legacy",
        "station-1",
        "key-1",
        ProtocolKind::GenericOpenAi,
        &[legacy],
    )
    .expect("legacy result");

    assert_eq!(
        result.semantic_confidence,
        SemanticConfidence::LegacyHttpOnly
    );
    assert!(!result
        .semantic_confidence
        .allows_authoritative_health_writeback());
}
