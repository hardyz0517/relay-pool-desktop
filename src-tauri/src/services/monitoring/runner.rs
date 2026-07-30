use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use crate::{
    application::{
        app_services::AppServices,
        credentials::CredentialService,
        monitoring::{
            commands::{MonitorExecutionReceipt, MonitorExecutionRequest},
            definition_bridge::target_snapshots_for_scope,
            orchestrator::{
                MonitorClock, MonitorIdGenerator, MonitorOrchestrator, ProbeTransport,
                ProbeTransportRequest, ProbeTransportResult,
            },
            planner::ProbePlanner,
            recorder::BufferedMonitoringRecorder,
            MonitoringService,
        },
        pagination::PageLimit,
        routing::RoutingService,
    },
    background_tasks::{TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskSupervisor},
    models::monitoring::TriggerKind,
    outbound::AsyncOutboundClient,
    services::monitoring::orchestrator_transport::ProbeExecutorTransport,
};
use futures_util::future::BoxFuture;
use tokio_util::sync::CancellationToken;

const RUNNER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const RUNNER_IDLE_WAKEUP: Duration = Duration::from_secs(300);
const RUNNER_MAX_SLEEP: Duration = Duration::from_secs(3600);
const RUNNER_TASK_ID: &str = "monitoring-runner-v2";
const RUNNER_TASK_KIND: &str = "monitoring_runner_v2";
const RUNNER_CONCURRENCY_KEY: &str = "monitoring-runner-v2";
const MONITOR_ALREADY_RUNNING_ERROR: &str = "monitoring execution is already running";
static LIVE_MONITORING_EXECUTIONS: OnceLock<Mutex<HashMap<String, CancellationToken>>> =
    OnceLock::new();

pub(crate) fn compose_monitoring_runner(
    services: &AppServices,
    outbound: AsyncOutboundClient,
) -> Arc<MonitoringRunner> {
    Arc::new(MonitoringRunner::new(
        services.monitoring.clone(),
        services.routing.clone(),
        services.credentials.clone(),
        outbound,
    ))
}

pub(crate) struct MonitoringRunner {
    monitoring: Arc<MonitoringService>,
    routing: Arc<RoutingService>,
    credentials: Arc<CredentialService>,
    outbound: AsyncOutboundClient,
}

impl MonitoringRunner {
    pub(crate) fn new(
        monitoring: Arc<MonitoringService>,
        routing: Arc<RoutingService>,
        credentials: Arc<CredentialService>,
        outbound: AsyncOutboundClient,
    ) -> Self {
        Self {
            monitoring,
            routing,
            credentials,
            outbound,
        }
    }

    pub(crate) fn due_monitor_ids(
        &self,
        limit: u32,
    ) -> BoxFuture<'static, Result<Vec<String>, String>> {
        let monitoring = self.monitoring.clone();
        Box::pin(async move {
            let limit = PageLimit::new(limit).map_err(|error| error.to_string())?;
            monitoring
                .due_monitor_ids_v2(limit)
                .await
                .map_err(|error| error.to_string())
        })
    }

    pub(crate) fn next_due_at_ms(&self) -> BoxFuture<'static, Result<Option<i64>, String>> {
        let monitoring = self.monitoring.clone();
        Box::pin(async move {
            monitoring
                .next_due_at_ms_v2()
                .await
                .map_err(|error| error.to_string())
        })
    }

    pub(crate) fn run_scheduled(
        &self,
        monitor_id: String,
        cancellation_token: CancellationToken,
    ) -> BoxFuture<'static, Result<MonitorExecutionReceipt, String>> {
        self.run_with_trigger(monitor_id, TriggerKind::Scheduled, None, cancellation_token)
    }

    pub(crate) fn run_manual(
        &self,
        monitor_id: String,
        manual_idempotency_key: String,
        cancellation_token: CancellationToken,
    ) -> BoxFuture<'static, Result<MonitorExecutionReceipt, String>> {
        self.run_with_trigger(
            monitor_id,
            TriggerKind::Manual,
            Some(manual_idempotency_key),
            cancellation_token,
        )
    }

    pub(crate) fn cancel_live(&self, execution_id: &str) -> bool {
        cancel_live_execution(execution_id)
    }

    fn run_with_trigger(
        &self,
        monitor_id: String,
        trigger_kind: TriggerKind,
        manual_idempotency_key: Option<String>,
        cancellation_token: CancellationToken,
    ) -> BoxFuture<'static, Result<MonitorExecutionReceipt, String>> {
        let monitoring = self.monitoring.clone();
        let routing = self.routing.clone();
        let credentials = self.credentials.clone();
        let outbound = self.outbound.clone();
        Box::pin(async move {
            let snapshot = monitoring
                .load_monitoring_planning_snapshot(&monitor_id)
                .await
                .map_err(|error| error.to_string())?;
            let daily_attempt_limit = i64::from(snapshot.risk_policy.max_daily_probe_attempts);
            let candidates = routing
                .load_runtime_candidates()
                .await
                .map_err(|error| error.to_string())?;
            let targets = target_snapshots_for_scope(&snapshot, &candidates);
            let plan = ProbePlanner
                .build_plan(snapshot.clone(), &targets, trigger_kind)
                .map_err(|error| format!("{error:?}"))?;
            let secrets = resolve_probe_secrets(credentials.as_ref(), &plan).await;
            let endpoints =
                ProbeExecutorTransport::endpoints_from_plan(&plan, &candidates, &secrets);
            let transport = BudgetedProbeTransport::new(
                ProbeExecutorTransport::new(outbound, cancellation_token.clone(), endpoints),
                monitoring.clone(),
                plan.monitor_id.clone(),
                daily_attempt_limit,
            );
            let recorder = BufferedMonitoringRecorder::default();
            let mut orchestrator = MonitorOrchestrator::new(
                SystemMonitorClock,
                UuidMonitorIdGenerator,
                recorder,
                transport,
            );
            let receipt = orchestrator
                .request_execution(MonitorExecutionRequest {
                    trigger_kind,
                    manual_idempotency_key,
                    snapshot,
                    targets,
                })
                .await
                .map_err(|error| format!("{error:?}"))?;
            let _live_registration =
                LiveExecutionRegistration::register(&receipt.execution_id, cancellation_token);
            let (_, _, recorder, _) = orchestrator.into_parts();
            if !receipt.reused_existing {
                let execution = recorder
                    .into_execution()
                    .ok_or_else(|| "Monitor execution produced no buffered result".to_string())?;
                monitoring
                    .commit_monitoring_execution(execution)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            Ok(receipt)
        })
    }
}

pub(crate) struct BudgetedProbeTransport<T> {
    inner: T,
    monitoring: Arc<MonitoringService>,
    monitor_id: String,
    daily_attempt_limit: i64,
}

impl<T> BudgetedProbeTransport<T> {
    pub(crate) fn new(
        inner: T,
        monitoring: Arc<MonitoringService>,
        monitor_id: String,
        daily_attempt_limit: i64,
    ) -> Self {
        Self {
            inner,
            monitoring,
            monitor_id,
            daily_attempt_limit,
        }
    }
}

impl<T> ProbeTransport for BudgetedProbeTransport<T>
where
    T: ProbeTransport + Send,
{
    fn send(&mut self, request: ProbeTransportRequest) -> BoxFuture<'_, ProbeTransportResult> {
        Box::pin(async move {
            match self
                .monitoring
                .reserve_monitoring_probe_budget(
                    &self.monitor_id,
                    &request.station_key_id,
                    1,
                    self.daily_attempt_limit,
                )
                .await
            {
                Ok(true) => self.inner.send(request).await,
                Ok(false) => ProbeTransportResult::failure(
                    crate::models::monitoring::FailureKind::BudgetExceeded,
                    false,
                    None,
                    0,
                ),
                Err(_) => ProbeTransportResult::failure(
                    crate::models::monitoring::FailureKind::Internal,
                    false,
                    None,
                    0,
                ),
            }
        })
    }
}

async fn resolve_probe_secrets(
    credentials: &CredentialService,
    plan: &crate::application::monitoring::planner::ProbePlan,
) -> std::collections::BTreeMap<String, String> {
    let mut secrets = std::collections::BTreeMap::new();
    for target in &plan.target_plans {
        if target.skip_failure_kind.is_some() || target.protocol_kind.is_none() {
            continue;
        }
        let Ok(secret) = credentials
            .resolve_station_key_secret(target.station_key_id.clone())
            .await
        else {
            continue;
        };
        let Ok(secret) = String::from_utf8(secret.as_bytes().to_vec()) else {
            continue;
        };
        secrets.insert(target.station_key_id.clone(), secret);
    }
    secrets
}

struct SystemMonitorClock;

impl MonitorClock for SystemMonitorClock {
    fn now_ms(&self) -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    fn advance_ms(&self, _duration_ms: u64) {}
}

struct UuidMonitorIdGenerator;

impl MonitorIdGenerator for UuidMonitorIdGenerator {
    fn next_id(&self) -> String {
        uuid::Uuid::now_v7().to_string()
    }
}

fn is_monitor_already_running_error(error: &str) -> bool {
    error == MONITOR_ALREADY_RUNNING_ERROR
}

struct LiveExecutionRegistration {
    execution_id: String,
}

impl LiveExecutionRegistration {
    fn register(execution_id: &str, token: CancellationToken) -> Result<Self, String> {
        let live = LIVE_MONITORING_EXECUTIONS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut live = live
            .lock()
            .map_err(|_| "monitoring live execution registry is unavailable".to_string())?;
        if live.insert(execution_id.to_string(), token).is_some() {
            return Err(MONITOR_ALREADY_RUNNING_ERROR.to_string());
        }
        Ok(Self {
            execution_id: execution_id.to_string(),
        })
    }
}

impl Drop for LiveExecutionRegistration {
    fn drop(&mut self) {
        if let Some(live) = LIVE_MONITORING_EXECUTIONS.get() {
            if let Ok(mut live) = live.lock() {
                live.remove(&self.execution_id);
            }
        }
    }
}

fn cancel_live_execution(execution_id: &str) -> bool {
    let Some(live) = LIVE_MONITORING_EXECUTIONS.get() else {
        return false;
    };
    let Ok(live) = live.lock() else {
        return false;
    };
    let Some(token) = live.get(execution_id) else {
        return false;
    };
    token.cancel();
    true
}

pub struct MonitoringRunnerState {
    supervisor: TaskSupervisor,
    task_id: TaskId,
}

impl MonitoringRunnerState {
    pub(crate) fn start(
        supervisor: TaskSupervisor,
        runner: Arc<MonitoringRunner>,
    ) -> Result<Self, String> {
        let task_id = TaskId::from(RUNNER_TASK_ID);
        let monitoring_runner = Arc::clone(&runner);
        supervisor
            .register(
                TaskSpec::new(task_id.clone(), RUNNER_TASK_KIND, move |context| {
                    let runner = Arc::clone(&monitoring_runner);
                    Box::pin(runner_loop(runner, context))
                })
                .with_concurrency_key(RUNNER_CONCURRENCY_KEY)
                .with_shutdown_timeout(RUNNER_SHUTDOWN_TIMEOUT),
            )
            .map_err(|error| error.to_string())?;
        supervisor
            .start(&task_id)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            supervisor,
            task_id,
        })
    }

    #[allow(dead_code)]
    pub fn stop(&self) {
        let _ = self.supervisor.cancel(&self.task_id);
    }
}

async fn runner_loop(
    runner: Arc<MonitoringRunner>,
    context: TaskRunContext,
) -> Result<(), TaskFailure> {
    loop {
        run_due_monitoring_executions_once(runner.as_ref(), &context).await;
        let delay = next_runner_delay(runner.as_ref()).await;
        tokio::select! {
            _ = context.cancellation_token.cancelled() => {
                return Err(TaskFailure::cancelled());
            }
            _ = tokio::time::sleep(delay) => {}
        }
    }
}

async fn next_runner_delay(runner: &MonitoringRunner) -> Duration {
    let Ok(next_due_at_ms) = runner.next_due_at_ms().await else {
        return RUNNER_IDLE_WAKEUP;
    };
    let Some(next_due_at_ms) = next_due_at_ms else {
        return RUNNER_IDLE_WAKEUP;
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    if next_due_at_ms <= now_ms {
        Duration::from_millis(1)
    } else {
        Duration::from_millis((next_due_at_ms - now_ms) as u64).min(RUNNER_MAX_SLEEP)
    }
}

async fn run_due_monitoring_executions_once(runner: &MonitoringRunner, context: &TaskRunContext) {
    match runner.due_monitor_ids(256).await {
        Ok(monitor_ids) => {
            for monitor_id in monitor_ids {
                if context.cancellation_token.is_cancelled() {
                    break;
                }
                if let Err(error) = runner
                    .run_scheduled(monitor_id, context.cancellation_token.clone())
                    .await
                {
                    if !is_monitor_already_running_error(&error) {
                        eprintln!("monitoring runner failed: {error}");
                    }
                }
            }
        }
        Err(error) => eprintln!("monitoring runner could not query due monitors: {error}"),
    }
}

impl Drop for MonitoringRunnerState {
    fn drop(&mut self) {
        let _ = self.supervisor.cancel(&self.task_id);
    }
}
