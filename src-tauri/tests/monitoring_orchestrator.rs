use futures_util::future::BoxFuture;
use std::{
    cell::Cell,
    collections::{BTreeMap, VecDeque},
};

#[path = "../src/models/monitoring/outcome.rs"]
pub mod model_outcome;

mod models {
    pub mod monitoring {
        use serde::{Deserialize, Serialize};

        pub use crate::model_outcome::{
            FailureKind, ProbeOutcome, ProtocolKind, SemanticConfidence,
        };

        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct DefinitionRevision(pub u64);

        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum ClientProfileId {
            StandardApi,
            CodexCliCompat,
            ClaudeCodeCompat,
            GeminiCliCompat,
            GrokCliCompat,
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct ClientProfileRef {
            pub id: ClientProfileId,
            pub version: u32,
        }

        impl ClientProfileRef {
            pub fn new(id: ClientProfileId, version: u32) -> Result<Self, String> {
                if version == 0 {
                    return Err("client_profile_version must be positive".to_string());
                }
                Ok(Self { id, version })
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        pub enum TargetScope {
            Station {
                station_id: String,
            },
            StationKey {
                station_id: String,
                station_key_id: String,
            },
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum TriggerKind {
            Scheduled,
            Manual,
            StartupRecovery,
            LegacyImport,
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct RetryPolicy {
            pub max_attempts_per_model: u8,
            pub base_delay_ms: u64,
            pub max_delay_ms: u64,
        }

        impl RetryPolicy {
            pub fn new(
                max_attempts_per_model: u8,
                base_delay_ms: u64,
                max_delay_ms: u64,
            ) -> Result<Self, String> {
                Ok(Self {
                    max_attempts_per_model,
                    base_delay_ms,
                    max_delay_ms,
                })
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct RiskPolicy {
            pub max_daily_probe_attempts: u32,
            pub require_manual_confirmation_for_high_frequency: bool,
        }

        impl RiskPolicy {
            pub fn new(max_daily_probe_attempts: u32) -> Result<Self, String> {
                Ok(Self {
                    max_daily_probe_attempts,
                    require_manual_confirmation_for_high_frequency: true,
                })
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum HealthWritebackMode {
            Disabled,
            ObserveOnly,
            Authoritative,
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct HealthPolicy {
            pub writeback_mode: HealthWritebackMode,
            pub failure_threshold: u8,
            pub recovery_threshold: u8,
        }

        impl HealthPolicy {
            pub fn new(
                writeback_mode: HealthWritebackMode,
                failure_threshold: u8,
                recovery_threshold: u8,
            ) -> Result<Self, String> {
                Ok(Self {
                    writeback_mode,
                    failure_threshold,
                    recovery_threshold,
                })
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct SchedulePolicy {
            pub interval_seconds: u64,
            pub jitter_seconds: u64,
            pub execution_timeout_ms: u64,
            pub attempt_timeout_ms: u64,
            pub slow_latency_threshold_ms: u64,
        }

        impl SchedulePolicy {
            pub fn new(
                interval_seconds: i64,
                jitter_seconds: i64,
                execution_timeout_ms: i64,
                attempt_timeout_ms: i64,
                slow_latency_threshold_ms: i64,
            ) -> Result<Self, String> {
                Ok(Self {
                    interval_seconds: interval_seconds as u64,
                    jitter_seconds: jitter_seconds as u64,
                    execution_timeout_ms: execution_timeout_ms as u64,
                    attempt_timeout_ms: attempt_timeout_ms as u64,
                    slow_latency_threshold_ms: slow_latency_threshold_ms as u64,
                })
            }
        }
    }
}

#[path = "../src/services/monitoring/auth.rs"]
pub mod monitoring_auth;
#[path = "../src/services/monitoring/profiles/mod.rs"]
pub mod monitoring_profiles;
mod services {
    pub mod monitoring {
        pub mod auth {
            pub use crate::monitoring_auth::*;
        }
        pub mod profiles {
            pub use crate::monitoring_profiles::*;
        }
    }
}

#[path = "../src/application/monitoring/commands.rs"]
pub mod commands;
#[path = "../src/application/monitoring/orchestrator.rs"]
pub mod orchestrator;
#[path = "../src/application/monitoring/planner.rs"]
pub mod planner;
#[path = "../src/application/monitoring/recorder.rs"]
pub mod recorder;

use commands::MonitorExecutionRequest;
use models::monitoring::{
    ClientProfileId, ClientProfileRef, DefinitionRevision, FailureKind, HealthPolicy,
    HealthWritebackMode, ProbeOutcome, ProtocolKind, RetryPolicy, RiskPolicy, SchedulePolicy,
    TargetScope, TriggerKind,
};
use orchestrator::{
    MonitorClock, MonitorIdGenerator, MonitorOrchestrator, ProbeTransport, ProbeTransportRequest,
    ProbeTransportResult,
};
use planner::{MonitorPlanningSnapshot, ProbePlanner, ProtocolSelection, TargetCapabilitySnapshot};
use recorder::{
    MonitorExecutionReceipt, MonitoringRecorder, RecordedAttempt, RecordedExecutionSummary,
    RecordedTargetResult,
};

fn available_transport_result(latency_ms: u64) -> ProbeTransportResult {
    ProbeTransportResult {
        outcome: ProbeOutcome::Available,
        failure_kind: None,
        retryable: false,
        retry_after_ms: None,
        latency_ms,
        semantic_confidence: models::monitoring::SemanticConfidence::ProtocolValidated,
    }
}

#[test]
fn planner_freezes_station_scope_targets_and_profile_hashes() {
    let snapshot = snapshot(ProtocolSelection::Explicit(ProtocolKind::OpenAiResponses));
    let mut targets = vec![target("key-a", Some(ProtocolKind::OpenAiResponses))];
    let plan = ProbePlanner
        .build_plan(snapshot, &targets, TriggerKind::Scheduled)
        .expect("plan");

    targets[0].station_key_id = "key-mutated-after-planning".to_string();

    assert_eq!(plan.target_plans.len(), 1);
    assert_eq!(plan.target_plans[0].station_key_id, "key-a");
    assert_eq!(
        plan.target_plans[0].protocol_kind,
        Some(ProtocolKind::OpenAiResponses)
    );
    assert!(plan.target_plans[0].request_profile_hash.is_some());
    assert_eq!(plan.model_plans.len(), 2);
}

#[tokio::test]
async fn slow_success_is_degraded_and_attempt_uses_its_own_deadline() {
    let mut transport = FakeTransport::default();
    transport.push_for_key("key-a", available_transport_result(16_000));
    let mut orchestrator = harness(transport);
    let mut request = default_request(vec![target("key-a", Some(ProtocolKind::OpenAiResponses))]);
    request.snapshot.schedule_policy =
        SchedulePolicy::new(300, 0, 60_000, 45_000, 6_000).expect("schedule");

    orchestrator
        .request_execution(request)
        .await
        .expect("execution");
    let (_, _, recorder, transport) = orchestrator.into_parts();

    assert_eq!(transport.requests.len(), 1);
    assert_eq!(transport.requests[0].deadline_at_ms, 46_000);
    assert_eq!(recorder.attempts[0].outcome, ProbeOutcome::Degraded);
    assert_eq!(
        recorder.attempts[0].failure_kind,
        Some(FailureKind::SlowLatency)
    );
    assert_eq!(recorder.targets[0].terminal_outcome, ProbeOutcome::Degraded);
    assert_eq!(
        recorder.targets[0].terminal_failure_kind,
        Some(FailureKind::SlowLatency)
    );
    assert_eq!(recorder.summaries[0].degraded_count, 1);
    assert_eq!(recorder.summaries[0].unavailable_count, 0);
}

#[tokio::test]
async fn sub2api_slow_latency_boundary_degrades_at_six_seconds() {
    let mut just_fast_transport = FakeTransport::default();
    just_fast_transport.push_for_key("key-a", available_transport_result(5_999));
    let mut just_fast = harness(just_fast_transport);
    let mut request = default_request(vec![target("key-a", Some(ProtocolKind::OpenAiResponses))]);
    request.snapshot.schedule_policy =
        SchedulePolicy::new(300, 0, 60_000, 45_000, 6_000).expect("schedule");

    just_fast
        .request_execution(request.clone())
        .await
        .expect("just fast execution");
    let (_, _, recorder, _) = just_fast.into_parts();
    assert_eq!(recorder.attempts[0].outcome, ProbeOutcome::Available);
    assert_eq!(
        recorder.targets[0].terminal_outcome,
        ProbeOutcome::Available
    );

    let mut boundary_transport = FakeTransport::default();
    boundary_transport.push_for_key("key-a", available_transport_result(6_000));
    let mut boundary = harness(boundary_transport);
    boundary
        .request_execution(request)
        .await
        .expect("boundary execution");
    let (_, _, recorder, _) = boundary.into_parts();
    assert_eq!(recorder.attempts[0].outcome, ProbeOutcome::Degraded);
    assert_eq!(
        recorder.attempts[0].failure_kind,
        Some(FailureKind::SlowLatency)
    );
    assert_eq!(recorder.targets[0].terminal_outcome, ProbeOutcome::Degraded);
}

#[tokio::test]
async fn profile_protocol_mismatch_is_rejected_before_transport() {
    let mut request = default_request(vec![target("key-a", Some(ProtocolKind::AnthropicMessages))]);
    request.snapshot.client_profile =
        ClientProfileRef::new(ClientProfileId::GeminiCliCompat, 1).expect("profile");
    request.snapshot.protocol_selection = ProtocolSelection::Explicit(ProtocolKind::OpenAiChat);
    let mut orchestrator = harness(FakeTransport::default());

    let result = orchestrator.request_execution(request).await;
    let (_, _, recorder, transport) = orchestrator.into_parts();

    assert!(result.is_err());
    assert!(transport.requests.is_empty());
    assert!(recorder.attempts.is_empty());
}

#[tokio::test]
async fn auth_failure_is_not_retried_or_hidden_by_fallback() {
    let mut transport = FakeTransport::default();
    transport.push_for_key(
        "key-a",
        ProbeTransportResult::failure(FailureKind::Auth, true, None, 12),
    );
    let mut orchestrator = harness(transport);

    orchestrator
        .request_execution(default_request(vec![target(
            "key-a",
            Some(ProtocolKind::OpenAiResponses),
        )]))
        .await
        .expect("execution");
    let (_, _, recorder, transport) = orchestrator.into_parts();

    assert_eq!(transport.requests.len(), 1);
    assert_eq!(recorder.attempts.len(), 1);
    assert_eq!(
        recorder.targets[0].terminal_failure_kind,
        Some(FailureKind::Auth)
    );
    assert!(!recorder.targets[0].used_fallback);
}

#[tokio::test]
async fn rate_limit_retries_same_model_with_retry_after_but_never_fallbacks() {
    let mut transport = FakeTransport::default();
    transport.push_for_key(
        "key-a",
        ProbeTransportResult::failure(FailureKind::RateLimit, true, Some(300), 20),
    );
    transport.push_for_key("key-a", available_transport_result(25));
    let mut orchestrator = harness(transport);

    orchestrator
        .request_execution(default_request(vec![target(
            "key-a",
            Some(ProtocolKind::OpenAiResponses),
        )]))
        .await
        .expect("execution");
    let (_, _, recorder, transport) = orchestrator.into_parts();

    assert_eq!(transport.requests.len(), 2);
    assert!(transport
        .requests
        .iter()
        .all(|request| request.model == "gpt-primary"));
    assert_eq!(recorder.targets[0].terminal_outcome, ProbeOutcome::Degraded);
    assert_eq!(
        recorder.targets[0].terminal_failure_kind,
        Some(FailureKind::RecoveredAfterRetry)
    );
    assert!(!recorder.targets[0].used_fallback);
}

#[tokio::test]
async fn retry_after_that_exceeds_deadline_does_not_start_next_attempt_or_fallback() {
    let mut transport = FakeTransport::default();
    transport.push_for_key(
        "key-a",
        ProbeTransportResult::failure(FailureKind::RateLimit, true, Some(29_500), 20),
    );
    let mut orchestrator = harness(transport);

    orchestrator
        .request_execution(default_request(vec![target(
            "key-a",
            Some(ProtocolKind::OpenAiResponses),
        )]))
        .await
        .expect("execution");
    let (_, _, recorder, transport) = orchestrator.into_parts();

    assert_eq!(transport.requests.len(), 1);
    assert_eq!(
        recorder.targets[0].terminal_failure_kind,
        Some(FailureKind::RateLimit)
    );
    assert!(!recorder.targets[0].used_fallback);
}

#[tokio::test]
async fn retry_and_fallback_recovery_is_degraded_with_one_target_denominator() {
    let mut transport = FakeTransport::default();
    transport.push_for_key(
        "key-a",
        ProbeTransportResult::failure(FailureKind::ServerError, true, None, 10),
    );
    transport.push_for_key(
        "key-a",
        ProbeTransportResult::failure(FailureKind::ServerError, true, None, 10),
    );
    transport.push_for_key("key-a", available_transport_result(10));
    let mut orchestrator = harness(transport);

    orchestrator
        .request_execution(default_request(vec![target(
            "key-a",
            Some(ProtocolKind::OpenAiResponses),
        )]))
        .await
        .expect("execution");
    let (_, _, recorder, transport) = orchestrator.into_parts();

    assert_eq!(transport.requests.len(), 3);
    assert_eq!(transport.requests[2].model, "gpt-fallback");
    assert_eq!(recorder.targets[0].attempt_count, 3);
    assert_eq!(recorder.targets[0].terminal_outcome, ProbeOutcome::Degraded);
    assert_eq!(recorder.summaries[0].target_count, 1);
    assert_eq!(recorder.summaries[0].degraded_count, 1);
}

#[tokio::test]
async fn remaining_deadline_must_fit_a_full_attempt_before_starting_request() {
    let mut transport = FakeTransport::default();
    transport.push_for_key(
        "key-a",
        ProbeTransportResult::failure(FailureKind::Network, true, None, 19_900),
    );
    let mut orchestrator = harness(transport);

    orchestrator
        .request_execution(default_request(vec![target(
            "key-a",
            Some(ProtocolKind::OpenAiResponses),
        )]))
        .await
        .expect("execution");
    let (_, _, recorder, transport) = orchestrator.into_parts();

    assert_eq!(transport.requests.len(), 1);
    assert_eq!(recorder.attempts.len(), 1);
    assert_eq!(
        recorder.targets[0].terminal_failure_kind,
        Some(FailureKind::Network)
    );
}

#[tokio::test]
async fn station_target_order_does_not_change_execution_summary() {
    let first = run_two_targets(vec!["key-a", "key-b"]).await;
    let second = run_two_targets(vec!["key-b", "key-a"]).await;

    assert_eq!(first.summary_outcome, second.summary_outcome);
    assert_eq!(first.available_count, second.available_count);
    assert_eq!(first.degraded_count, second.degraded_count);
    assert_eq!(first.unavailable_count, second.unavailable_count);
    assert_eq!(first.skipped_count, second.skipped_count);
}

#[tokio::test]
async fn manual_idempotency_key_returns_existing_execution_without_transport() {
    let mut recorder = FakeRecorder::default();
    recorder.manual.insert(
        "run-now:monitor-1".to_string(),
        MonitorExecutionReceipt {
            execution_id: "execution-existing".to_string(),
            reused_existing: false,
        },
    );
    let mut orchestrator = MonitorOrchestrator::new(
        FakeClock::new(1_000),
        FakeIds::default(),
        recorder,
        FakeTransport::default(),
    );

    let receipt = orchestrator
        .request_execution(execution_request(
            TriggerKind::Manual,
            Some("run-now:monitor-1"),
            snapshot(ProtocolSelection::Explicit(ProtocolKind::OpenAiResponses)),
            vec![target("key-a", Some(ProtocolKind::OpenAiResponses))],
        ))
        .await
        .expect("execution");
    let (_, _, recorder, transport) = orchestrator.into_parts();

    assert_eq!(receipt.execution_id, "execution-existing");
    assert!(receipt.reused_existing);
    assert!(transport.requests.is_empty());
    assert!(recorder.attempts.is_empty());
}

async fn run_two_targets(order: Vec<&str>) -> RecordedExecutionSummary {
    let mut transport = FakeTransport::default();
    transport.push_for_key("key-a", available_transport_result(10));
    transport.push_for_key(
        "key-b",
        ProbeTransportResult::failure(FailureKind::ServerError, false, None, 10),
    );
    let targets = order
        .into_iter()
        .map(|key| target(key, Some(ProtocolKind::OpenAiResponses)))
        .collect();
    let mut orchestrator = harness(transport);
    orchestrator
        .request_execution(default_request(targets))
        .await
        .expect("execution");
    let (_, _, recorder, _) = orchestrator.into_parts();
    recorder.summaries[0].clone()
}

fn harness(
    transport: FakeTransport,
) -> MonitorOrchestrator<FakeClock, FakeIds, FakeRecorder, FakeTransport> {
    MonitorOrchestrator::new(
        FakeClock::new(1_000),
        FakeIds::default(),
        FakeRecorder::default(),
        transport,
    )
}

fn default_request(targets: Vec<TargetCapabilitySnapshot>) -> MonitorExecutionRequest {
    execution_request(
        TriggerKind::Scheduled,
        None,
        snapshot(ProtocolSelection::Explicit(ProtocolKind::OpenAiResponses)),
        targets,
    )
}

fn execution_request(
    trigger_kind: TriggerKind,
    manual_idempotency_key: Option<&str>,
    snapshot: MonitorPlanningSnapshot,
    targets: Vec<TargetCapabilitySnapshot>,
) -> MonitorExecutionRequest {
    MonitorExecutionRequest {
        trigger_kind,
        manual_idempotency_key: manual_idempotency_key.map(str::to_string),
        snapshot,
        targets,
    }
}

fn snapshot(protocol_selection: ProtocolSelection) -> MonitorPlanningSnapshot {
    MonitorPlanningSnapshot {
        id: "monitor-1".to_string(),
        revision: DefinitionRevision(1),
        target_scope: TargetScope::Station {
            station_id: "station-1".to_string(),
        },
        protocol_selection,
        client_profile: ClientProfileRef::new(ClientProfileId::StandardApi, 1).expect("profile"),
        primary_model: "gpt-primary".to_string(),
        fallback_models: vec!["gpt-fallback".to_string()],
        schedule_policy: SchedulePolicy::new(300, 0, 60_000, 45_000, 6_000).expect("schedule"),
        retry_policy: RetryPolicy::new(2, 200, 2_000).expect("retry"),
        risk_policy: RiskPolicy::new(100).expect("risk"),
        health_policy: HealthPolicy::new(HealthWritebackMode::ObserveOnly, 2, 2).expect("health"),
    }
}

fn target(key: &str, protocol: Option<ProtocolKind>) -> TargetCapabilitySnapshot {
    TargetCapabilitySnapshot {
        station_id: "station-1".to_string(),
        station_key_id: key.to_string(),
        endpoint_revision: 7,
        provider_protocol: protocol,
        endpoint_protocol: protocol,
    }
}

#[derive(Default)]
struct FakeIds {
    next: Cell<u32>,
}

impl MonitorIdGenerator for FakeIds {
    fn next_id(&self) -> String {
        let next = self.next.get() + 1;
        self.next.set(next);
        format!("execution-{next}")
    }
}

#[derive(Clone)]
struct FakeClock {
    now_ms: Cell<i64>,
}

impl FakeClock {
    fn new(now_ms: i64) -> Self {
        Self {
            now_ms: Cell::new(now_ms),
        }
    }
}

impl MonitorClock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.now_ms.get()
    }

    fn advance_ms(&self, duration_ms: u64) {
        self.now_ms
            .set(self.now_ms.get().saturating_add(duration_ms as i64));
    }
}

#[derive(Default)]
struct FakeTransport {
    responses_by_key: BTreeMap<String, VecDeque<ProbeTransportResult>>,
    requests: Vec<ProbeTransportRequest>,
}

impl FakeTransport {
    fn push_for_key(&mut self, key: &str, response: ProbeTransportResult) {
        self.responses_by_key
            .entry(key.to_string())
            .or_default()
            .push_back(response);
    }
}

impl ProbeTransport for FakeTransport {
    fn send(&mut self, request: ProbeTransportRequest) -> BoxFuture<'_, ProbeTransportResult> {
        let response = self
            .responses_by_key
            .get_mut(&request.station_key_id)
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| available_transport_result(1));
        self.requests.push(request);
        Box::pin(async move { response })
    }
}

#[derive(Default)]
struct FakeRecorder {
    manual: BTreeMap<String, MonitorExecutionReceipt>,
    attempts: Vec<RecordedAttempt>,
    targets: Vec<RecordedTargetResult>,
    summaries: Vec<RecordedExecutionSummary>,
}

impl MonitoringRecorder for FakeRecorder {
    fn find_manual_execution(&self, idempotency_key: &str) -> Option<MonitorExecutionReceipt> {
        self.manual.get(idempotency_key).cloned()
    }

    fn begin_execution(
        &mut self,
        execution_id: String,
        _plan: &planner::ProbePlan,
        manual_idempotency_key: Option<&str>,
        _started_at_ms: i64,
    ) -> MonitorExecutionReceipt {
        let receipt = MonitorExecutionReceipt {
            execution_id,
            reused_existing: false,
        };
        if let Some(key) = manual_idempotency_key {
            self.manual.insert(key.to_string(), receipt.clone());
        }
        receipt
    }

    fn append_attempt(&mut self, attempt: RecordedAttempt) {
        self.attempts.push(attempt);
    }

    fn finalize_target(&mut self, result: RecordedTargetResult) {
        self.targets.push(result);
    }

    fn finalize_execution(&mut self, summary: RecordedExecutionSummary) {
        self.summaries.push(summary);
    }
}
