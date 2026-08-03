use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    application::{
        app_services::AppServices,
        credentials::CredentialService,
        monitoring::{
            commands::MonitorExecutionRequest,
            definition_bridge::target_snapshots_for_scope,
            orchestrator::{
                MonitorClock, MonitorIdGenerator, MonitorOrchestrator, ProbeTransport,
                ProbeTransportRequest, ProbeTransportResult,
            },
            planner::{MonitorPlanningSnapshot, ProbePlan, ProbePlanner, TargetCapabilitySnapshot},
            recorder::{BufferedMonitoringRecorder, MonitorExecutionReceipt},
            MonitoringService,
        },
        pagination::PageLimit,
        queries::routing_runtime::RoutingMonitoringTargetSnapshot,
        routing::RoutingService,
    },
    background_tasks::{TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskSupervisor},
    models::monitoring::{RunChannelMonitorReceipt, TriggerKind},
    outbound::{AsyncOutboundClient, AsyncOutboundClientConfig},
    services::{
        endpoint_ping::ping_station_endpoint,
        monitoring::orchestrator_transport::ProbeExecutorTransport,
    },
};
use futures_util::future::{join_all, BoxFuture};
use tokio::{
    sync::{mpsc, OwnedSemaphorePermit, Semaphore},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

const RUNNER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const RUNNER_IDLE_WAKEUP: Duration = Duration::from_secs(300);
const RUNNER_MAX_SLEEP: Duration = Duration::from_secs(3600);
const RUNNER_TASK_ID: &str = "monitoring-runner-v2";
const RUNNER_TASK_KIND: &str = "monitoring_runner_v2";
const RUNNER_CONCURRENCY_KEY: &str = "monitoring-runner-v2";
const MONITOR_ALREADY_RUNNING_ERROR: &str = "monitoring execution is already running";
const MANUAL_QUEUE_CAPACITY: usize = 128;
const RUNNER_GLOBAL_CONCURRENCY: usize = 4;
const RUNNER_STATION_CONCURRENCY: usize = 2;
const RUNNER_DUE_BATCH_LIMIT: u32 = 32;
const RUNNER_CONFLICT_BACKOFF: Duration = Duration::from_millis(250);
const ENDPOINT_PING_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn compose_monitoring_runner(services: &AppServices) -> Arc<MonitoringRunner> {
    Arc::new(MonitoringRunner::new(
        services.monitoring.clone(),
        services.routing.clone(),
        services.credentials.clone(),
        AsyncOutboundClient::new(AsyncOutboundClientConfig::monitoring_budget()),
    ))
}

pub(crate) struct MonitoringRunner {
    monitoring: Arc<MonitoringService>,
    routing: Arc<RoutingService>,
    credentials: Arc<CredentialService>,
    outbound: AsyncOutboundClient,
    manual_tx: mpsc::Sender<PreparedMonitoringExecution>,
    manual_rx: Mutex<Option<mpsc::Receiver<PreparedMonitoringExecution>>>,
    live: Arc<LiveExecutionRegistry>,
    station_permits: StationPermitRegistry,
    admission_lock: tokio::sync::Mutex<()>,
}

impl MonitoringRunner {
    pub(crate) fn new(
        monitoring: Arc<MonitoringService>,
        routing: Arc<RoutingService>,
        credentials: Arc<CredentialService>,
        outbound: AsyncOutboundClient,
    ) -> Self {
        let (manual_tx, manual_rx) = mpsc::channel(MANUAL_QUEUE_CAPACITY);
        Self {
            monitoring,
            routing,
            credentials,
            outbound,
            manual_tx,
            manual_rx: Mutex::new(Some(manual_rx)),
            live: Arc::new(LiveExecutionRegistry::default()),
            station_permits: StationPermitRegistry::default(),
            admission_lock: tokio::sync::Mutex::new(()),
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

    pub(crate) async fn enqueue_manual(
        &self,
        monitor_id: String,
        trigger_request_id: String,
    ) -> Result<RunChannelMonitorReceipt, String> {
        let _admission = self.admission_lock.lock().await;
        if let Some((execution_id, existing_monitor_id, status)) = self
            .monitoring
            .find_execution_by_trigger_request_id(&trigger_request_id)
            .await
            .map_err(|error| error.to_string())?
        {
            return Ok(RunChannelMonitorReceipt {
                execution_id,
                monitor_id: existing_monitor_id,
                status,
                trigger_request_id,
                reused_existing: true,
            });
        }

        let permit = self
            .manual_tx
            .try_reserve()
            .map_err(|_| "monitoring execution queue is full".to_string())?;
        let prepared = self
            .prepare_execution(
                monitor_id.clone(),
                TriggerKind::Manual,
                Some(trigger_request_id.clone()),
            )
            .await?;
        let execution_id = prepared.execution_id.clone();
        match self.live.register(
            execution_id.clone(),
            monitor_id.clone(),
            prepared.station_key_ids(),
            Some(trigger_request_id.clone()),
            prepared.cancellation_token.clone(),
        )? {
            LiveRegistrationOutcome::Existing {
                execution_id: existing,
                monitor_id: existing_monitor_id,
            } => {
                return Ok(RunChannelMonitorReceipt {
                    execution_id: existing,
                    monitor_id: existing_monitor_id,
                    status: "queued".to_string(),
                    trigger_request_id,
                    reused_existing: true,
                });
            }
            LiveRegistrationOutcome::Registered(registration) => {
                let mut prepared = prepared;
                prepared.registration = Some(registration);
                self.persist_queued(&prepared).await?;
                permit.send(prepared);
            }
        }
        Ok(RunChannelMonitorReceipt {
            execution_id,
            monitor_id,
            status: "queued".to_string(),
            trigger_request_id,
            reused_existing: false,
        })
    }

    pub(crate) fn cancel_live(&self, execution_id: &str) -> bool {
        self.live.cancel(execution_id)
    }

    async fn prepare_execution(
        &self,
        monitor_id: String,
        trigger_kind: TriggerKind,
        manual_idempotency_key: Option<String>,
    ) -> Result<PreparedMonitoringExecution, String> {
        let snapshot = self
            .monitoring
            .load_monitoring_planning_snapshot(&monitor_id)
            .await
            .map_err(|error| error.to_string())?;
        let routing_targets = self
            .routing
            .load_monitoring_target_snapshots()
            .await
            .map_err(|error| error.to_string())?;
        let targets = target_snapshots_for_scope(&snapshot, &routing_targets);
        let plan = ProbePlanner
            .build_plan(snapshot.clone(), &targets, trigger_kind)
            .map_err(|error| format!("{error:?}"))?;
        Ok(PreparedMonitoringExecution {
            execution_id: uuid::Uuid::now_v7().to_string(),
            manual_idempotency_key,
            snapshot,
            targets,
            routing_targets,
            plan,
            cancellation_token: CancellationToken::new(),
            registration: None,
        })
    }

    async fn persist_queued(&self, prepared: &PreparedMonitoringExecution) -> Result<(), String> {
        self.monitoring
            .queue_monitoring_execution(
                prepared.execution_id.clone(),
                prepared.manual_idempotency_key.clone(),
                &prepared.plan,
                chrono::Utc::now().timestamp_millis(),
            )
            .await
            .map_err(|error| error.to_string())
    }

    async fn execute_prepared(
        &self,
        prepared: PreparedMonitoringExecution,
    ) -> Result<MonitorExecutionReceipt, String> {
        let _station_permits = self
            .station_permits
            .acquire(&prepared.station_ids(), &prepared.cancellation_token)
            .await?;
        if prepared.cancellation_token.is_cancelled()
            || !self
                .monitoring
                .start_queued_monitoring_execution(&prepared.execution_id)
                .await
                .map_err(|error| error.to_string())?
        {
            return Err("monitoring execution was cancelled before start".to_string());
        }
        let daily_attempt_limit = i64::from(prepared.snapshot.risk_policy.max_daily_probe_attempts);
        let secrets = resolve_probe_secrets(self.credentials.as_ref(), &prepared.plan).await;
        let endpoints = ProbeExecutorTransport::endpoints_from_plan(
            &prepared.plan,
            &prepared.routing_targets,
            &secrets,
        );
        let transport = BudgetedProbeTransport::new(
            ProbeExecutorTransport::new(
                self.outbound.clone(),
                prepared.cancellation_token.clone(),
                endpoints,
            ),
            self.monitoring.clone(),
            prepared.plan.monitor_id.clone(),
            daily_attempt_limit,
        );
        let recorder = BufferedMonitoringRecorder::default();
        let mut orchestrator = MonitorOrchestrator::new(
            SystemMonitorClock,
            FixedMonitorIdGenerator(prepared.execution_id.clone()),
            recorder,
            transport,
        );
        let execution_future = orchestrator.request_execution(MonitorExecutionRequest {
            trigger_kind: prepared.plan.trigger_kind,
            manual_idempotency_key: prepared.manual_idempotency_key.clone(),
            snapshot: prepared.snapshot.clone(),
            targets: prepared.targets.clone(),
        });
        let ping_future = self.collect_endpoint_pings(&prepared);
        let (receipt, _) = tokio::join!(execution_future, ping_future);
        let receipt = receipt.map_err(|error| format!("{error:?}"))?;
        let (_, _, recorder, _) = orchestrator.into_parts();
        if prepared.cancellation_token.is_cancelled() {
            let _ = self
                .monitoring
                .cancel_execution(prepared.execution_id.clone())
                .await;
        } else if !receipt.reused_existing {
            let execution = recorder
                .into_execution()
                .ok_or_else(|| "Monitor execution produced no buffered result".to_string())?;
            self.monitoring
                .commit_monitoring_execution(execution)
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok(receipt)
    }

    async fn collect_endpoint_pings(&self, prepared: &PreparedMonitoringExecution) {
        let probes = endpoint_ping_targets(prepared)
            .into_iter()
            .map(|target| {
                let outbound = self.outbound.clone();
                let routing = Arc::clone(&self.routing);
                let cancellation_token = prepared.cancellation_token.clone();
                async move {
                    let probe = ping_station_endpoint(
                        &outbound,
                        &target.base_url,
                        ENDPOINT_PING_TIMEOUT,
                        cancellation_token.clone(),
                    )
                    .await;
                    if cancellation_token.is_cancelled() {
                        return;
                    }
                    let checked_at = chrono::Utc::now().timestamp_millis().to_string();
                    let _ = routing
                        .record_station_endpoint_health(
                            target.station_id,
                            target.endpoint_revision,
                            probe.status,
                            probe.latency_ms,
                            checked_at,
                            probe.error_summary,
                        )
                        .await;
                }
            })
            .collect::<Vec<_>>();
        join_all(probes).await;
    }

    fn take_manual_receiver(&self) -> Result<mpsc::Receiver<PreparedMonitoringExecution>, String> {
        self.manual_rx
            .lock()
            .map_err(|_| "monitoring queue is unavailable".to_string())?
            .take()
            .ok_or_else(|| "monitoring queue is already running".to_string())
    }
}

struct PreparedMonitoringExecution {
    execution_id: String,
    manual_idempotency_key: Option<String>,
    snapshot: MonitorPlanningSnapshot,
    targets: Vec<TargetCapabilitySnapshot>,
    routing_targets: Vec<RoutingMonitoringTargetSnapshot>,
    plan: ProbePlan,
    cancellation_token: CancellationToken,
    registration: Option<LiveExecutionRegistration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EndpointPingTarget {
    station_id: String,
    endpoint_revision: i64,
    base_url: String,
}

fn endpoint_ping_targets(prepared: &PreparedMonitoringExecution) -> Vec<EndpointPingTarget> {
    let mut targets = std::collections::BTreeMap::new();
    for target in &prepared.plan.target_plans {
        let Some(candidate) = prepared
            .routing_targets
            .iter()
            .find(|candidate| candidate.station_key_id == target.station_key_id)
        else {
            continue;
        };
        targets
            .entry(target.station_id.clone())
            .or_insert_with(|| EndpointPingTarget {
                station_id: target.station_id.clone(),
                endpoint_revision: target.endpoint_revision,
                base_url: candidate.api_base_url.clone(),
            });
    }
    targets.into_values().collect()
}

impl PreparedMonitoringExecution {
    fn station_key_ids(&self) -> Vec<String> {
        self.plan
            .target_plans
            .iter()
            .map(|target| target.station_key_id.clone())
            .collect()
    }

    fn station_ids(&self) -> Vec<String> {
        self.plan
            .target_plans
            .iter()
            .map(|target| target.station_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

#[derive(Clone, Default)]
struct StationPermitRegistry {
    semaphores: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
}

impl StationPermitRegistry {
    async fn acquire(
        &self,
        station_ids: &[String],
        cancellation: &CancellationToken,
    ) -> Result<Vec<OwnedSemaphorePermit>, String> {
        let semaphores =
            {
                let mut state = self
                    .semaphores
                    .lock()
                    .map_err(|_| "monitoring station permit registry is unavailable".to_string())?;
                station_ids
                    .iter()
                    .map(|station_id| {
                        Arc::clone(state.entry(station_id.clone()).or_insert_with(|| {
                            Arc::new(Semaphore::new(RUNNER_STATION_CONCURRENCY))
                        }))
                    })
                    .collect::<Vec<_>>()
            };
        let mut permits = Vec::with_capacity(semaphores.len());
        for semaphore in semaphores {
            let permit = tokio::select! {
                _ = cancellation.cancelled() => {
                    return Err("monitoring execution was cancelled while waiting for station capacity".to_string());
                }
                permit = semaphore.acquire_owned() => {
                    permit.map_err(|_| "monitoring station permit is unavailable".to_string())?
                }
            };
            permits.push(permit);
        }
        Ok(permits)
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

struct FixedMonitorIdGenerator(String);

impl MonitorIdGenerator for FixedMonitorIdGenerator {
    fn next_id(&self) -> String {
        self.0.clone()
    }
}

fn is_monitor_already_running_error(error: &str) -> bool {
    error == MONITOR_ALREADY_RUNNING_ERROR
}

#[derive(Default)]
struct LiveExecutionRegistry {
    state: Mutex<LiveExecutionState>,
}

#[derive(Default)]
struct LiveExecutionState {
    executions: HashMap<String, LiveExecution>,
    by_monitor: HashMap<String, String>,
    by_key: HashMap<String, String>,
    by_trigger_request: HashMap<String, String>,
}

struct LiveExecution {
    monitor_id: String,
    station_key_ids: Vec<String>,
    trigger_request_id: Option<String>,
    token: CancellationToken,
}

enum LiveRegistrationOutcome {
    Registered(LiveExecutionRegistration),
    Existing {
        execution_id: String,
        monitor_id: String,
    },
}

struct LiveExecutionRegistration {
    execution_id: String,
    registry: Arc<LiveExecutionRegistry>,
}

impl LiveExecutionRegistry {
    fn register(
        self: &Arc<Self>,
        execution_id: String,
        monitor_id: String,
        station_key_ids: Vec<String>,
        trigger_request_id: Option<String>,
        token: CancellationToken,
    ) -> Result<LiveRegistrationOutcome, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "monitoring live execution registry is unavailable".to_string())?;
        let existing = state
            .by_monitor
            .get(&monitor_id)
            .or_else(|| station_key_ids.iter().find_map(|key| state.by_key.get(key)))
            .or_else(|| {
                trigger_request_id
                    .as_ref()
                    .and_then(|request_id| state.by_trigger_request.get(request_id))
            })
            .cloned();
        if let Some(existing) = existing {
            let monitor_id = state
                .executions
                .get(&existing)
                .map(|execution| execution.monitor_id.clone())
                .unwrap_or(monitor_id);
            return Ok(LiveRegistrationOutcome::Existing {
                execution_id: existing,
                monitor_id,
            });
        }
        state
            .by_monitor
            .insert(monitor_id.clone(), execution_id.clone());
        for key in &station_key_ids {
            state.by_key.insert(key.clone(), execution_id.clone());
        }
        if let Some(request_id) = &trigger_request_id {
            state
                .by_trigger_request
                .insert(request_id.clone(), execution_id.clone());
        }
        state.executions.insert(
            execution_id.clone(),
            LiveExecution {
                monitor_id,
                station_key_ids,
                trigger_request_id,
                token,
            },
        );
        Ok(LiveRegistrationOutcome::Registered(
            LiveExecutionRegistration {
                execution_id,
                registry: Arc::clone(self),
            },
        ))
    }

    fn cancel(&self, execution_id: &str) -> bool {
        let Ok(state) = self.state.lock() else {
            return false;
        };
        let Some(execution) = state.executions.get(execution_id) else {
            return false;
        };
        execution.token.cancel();
        true
    }

    fn cancel_all(&self) -> Vec<String> {
        let Ok(state) = self.state.lock() else {
            return Vec::new();
        };
        let execution_ids = state.executions.keys().cloned().collect::<Vec<_>>();
        for execution in state.executions.values() {
            execution.token.cancel();
        }
        execution_ids
    }

    fn unregister(&self, execution_id: &str) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let Some(execution) = state.executions.remove(execution_id) else {
            return;
        };
        state.by_monitor.remove(&execution.monitor_id);
        for key in execution.station_key_ids {
            state.by_key.remove(&key);
        }
        if let Some(request_id) = execution.trigger_request_id {
            state.by_trigger_request.remove(&request_id);
        }
    }
}

impl Drop for LiveExecutionRegistration {
    fn drop(&mut self) {
        self.registry.unregister(&self.execution_id);
    }
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
        let manual_rx = runner.take_manual_receiver()?;
        let task_id = TaskId::from(RUNNER_TASK_ID);
        let monitoring_runner = Arc::clone(&runner);
        let receiver = Arc::new(tokio::sync::Mutex::new(manual_rx));
        supervisor
            .register(
                TaskSpec::new(task_id.clone(), RUNNER_TASK_KIND, move |context| {
                    let runner = Arc::clone(&monitoring_runner);
                    let receiver = Arc::clone(&receiver);
                    Box::pin(runner_loop(runner, receiver, context))
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

    pub fn stop(&self) {
        let _ = self.supervisor.cancel(&self.task_id);
    }
}

async fn runner_loop(
    runner: Arc<MonitoringRunner>,
    manual_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<PreparedMonitoringExecution>>>,
    context: TaskRunContext,
) -> Result<(), TaskFailure> {
    let mut executions = JoinSet::new();
    let mut schedule_backoff_until = tokio::time::Instant::now();
    loop {
        let has_capacity = executions.len() < RUNNER_GLOBAL_CONCURRENCY;
        let mut delay = if has_capacity {
            next_runner_delay(runner.as_ref()).await
        } else {
            RUNNER_IDLE_WAKEUP
        };
        let now = tokio::time::Instant::now();
        if schedule_backoff_until > now {
            delay = delay.max(schedule_backoff_until.duration_since(now));
        }
        tokio::select! {
            biased;
            _ = context.cancellation_token.cancelled() => {
                interrupt_live_executions(runner.as_ref()).await;
                drain_execution_tasks(&mut executions).await;
                return Err(TaskFailure::cancelled());
            }
            joined = executions.join_next(), if !executions.is_empty() => {
                if let Some(Err(error)) = joined {
                    eprintln!("monitoring runner worker failed: {error}");
                }
            }
            prepared = receive_manual(&manual_rx), if has_capacity => {
                if let Some(prepared) = prepared {
                    spawn_prepared_execution(&mut executions, Arc::clone(&runner), prepared);
                }
            }
            _ = tokio::time::sleep(delay), if has_capacity => {
                let available = RUNNER_GLOBAL_CONCURRENCY.saturating_sub(executions.len());
                let started = start_due_monitoring_executions(
                    Arc::clone(&runner),
                    &context,
                    &mut executions,
                    available,
                ).await;
                schedule_backoff_until = if started == 0 {
                    tokio::time::Instant::now() + RUNNER_CONFLICT_BACKOFF
                } else {
                    tokio::time::Instant::now()
                };
            }
        }
    }
}

async fn receive_manual(
    receiver: &tokio::sync::Mutex<mpsc::Receiver<PreparedMonitoringExecution>>,
) -> Option<PreparedMonitoringExecution> {
    receiver.lock().await.recv().await
}

async fn execute_prepared_and_record_failure(
    runner: &MonitoringRunner,
    prepared: PreparedMonitoringExecution,
) {
    let execution_id = prepared.execution_id.clone();
    if let Err(error) = runner.execute_prepared(prepared).await {
        let _ = runner
            .monitoring
            .interrupt_monitoring_execution(&execution_id)
            .await;
        if error != "monitoring execution was cancelled before start" {
            eprintln!("monitoring runner failed: {error}");
        }
    }
}

fn spawn_prepared_execution(
    executions: &mut JoinSet<()>,
    runner: Arc<MonitoringRunner>,
    prepared: PreparedMonitoringExecution,
) {
    executions.spawn(async move {
        execute_prepared_and_record_failure(runner.as_ref(), prepared).await;
    });
}

async fn drain_execution_tasks(executions: &mut JoinSet<()>) {
    let drain = async { while executions.join_next().await.is_some() {} };
    if tokio::time::timeout(RUNNER_SHUTDOWN_TIMEOUT, drain)
        .await
        .is_err()
    {
        executions.abort_all();
        while executions.join_next().await.is_some() {}
    }
}

async fn interrupt_live_executions(runner: &MonitoringRunner) {
    let execution_ids = runner.live.cancel_all();
    for execution_id in execution_ids {
        let _ = runner
            .monitoring
            .interrupt_monitoring_execution(&execution_id)
            .await;
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

async fn start_due_monitoring_executions(
    runner: Arc<MonitoringRunner>,
    context: &TaskRunContext,
    executions: &mut JoinSet<()>,
    available: usize,
) -> usize {
    let mut started = 0;
    match runner.due_monitor_ids(RUNNER_DUE_BATCH_LIMIT).await {
        Ok(monitor_ids) => {
            for monitor_id in monitor_ids {
                if context.cancellation_token.is_cancelled() || started >= available {
                    break;
                }
                match runner
                    .prepare_execution(monitor_id, TriggerKind::Scheduled, None)
                    .await
                {
                    Ok(mut prepared) => {
                        prepared.cancellation_token = context.cancellation_token.child_token();
                        let execution_id = prepared.execution_id.clone();
                        let monitor_id = prepared.plan.monitor_id.clone();
                        match runner.live.register(
                            execution_id,
                            monitor_id,
                            prepared.station_key_ids(),
                            None,
                            prepared.cancellation_token.clone(),
                        ) {
                            Ok(LiveRegistrationOutcome::Registered(registration)) => {
                                prepared.registration = Some(registration);
                                if let Err(error) = runner.persist_queued(&prepared).await {
                                    eprintln!("monitoring runner failed: {error}");
                                } else {
                                    spawn_prepared_execution(
                                        executions,
                                        Arc::clone(&runner),
                                        prepared,
                                    );
                                    started += 1;
                                }
                            }
                            Ok(LiveRegistrationOutcome::Existing { .. }) => {}
                            Err(error) if !is_monitor_already_running_error(&error) => {
                                eprintln!("monitoring runner failed: {error}");
                            }
                            Err(_) => {}
                        }
                    }
                    Err(error) => eprintln!("monitoring runner failed: {error}"),
                }
            }
        }
        Err(error) => eprintln!("monitoring runner could not query due monitors: {error}"),
    }
    started
}

impl Drop for MonitoringRunnerState {
    fn drop(&mut self) {
        let _ = self.supervisor.cancel(&self.task_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn station_permits_are_bounded_and_cancellable() {
        let registry = StationPermitRegistry::default();
        let station = vec!["station-1".to_string()];
        let first = registry
            .acquire(&station, &CancellationToken::new())
            .await
            .expect("first permit");
        let second = registry
            .acquire(&station, &CancellationToken::new())
            .await
            .expect("second permit");
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let blocked = registry.acquire(&station, &cancellation).await;
        assert!(blocked.is_err());
        drop((first, second));
        assert!(registry
            .acquire(&station, &CancellationToken::new())
            .await
            .is_ok());
    }

    #[test]
    fn live_registry_reuses_conflicts_and_releases_all_indexes() {
        let registry = Arc::new(LiveExecutionRegistry::default());
        let registration = match registry
            .register(
                "execution-1".to_string(),
                "monitor-1".to_string(),
                vec!["key-1".to_string()],
                Some("request-1".to_string()),
                CancellationToken::new(),
            )
            .expect("register")
        {
            LiveRegistrationOutcome::Registered(registration) => registration,
            LiveRegistrationOutcome::Existing { .. } => panic!("unexpected existing execution"),
        };
        for (monitor_id, key_id, request_id) in [
            ("monitor-1", "key-2", None),
            ("monitor-2", "key-1", None),
            ("monitor-2", "key-2", Some("request-1")),
        ] {
            assert!(matches!(
                registry
                    .register(
                        "execution-2".to_string(),
                        monitor_id.to_string(),
                        vec![key_id.to_string()],
                        request_id.map(str::to_string),
                        CancellationToken::new(),
                    )
                    .expect("reuse conflict"),
                LiveRegistrationOutcome::Existing { execution_id, monitor_id }
                    if execution_id == "execution-1" && monitor_id == "monitor-1"
            ));
        }
        drop(registration);
        assert!(matches!(
            registry
                .register(
                    "execution-2".to_string(),
                    "monitor-1".to_string(),
                    vec!["key-1".to_string()],
                    Some("request-1".to_string()),
                    CancellationToken::new(),
                )
                .expect("register after release"),
            LiveRegistrationOutcome::Registered(_)
        ));
    }
}
