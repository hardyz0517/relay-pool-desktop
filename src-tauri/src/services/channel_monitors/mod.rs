pub mod probe;
pub mod redaction;
pub mod templates;

use std::{
    collections::HashSet,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use crate::{
    application::{
        app_services::AppServices, credentials::CredentialService, error::ApplicationError,
        monitoring::MonitoringService, pagination::PageLimit, pricing::PricingService,
        routing::RoutingService,
    },
    background_tasks::{TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskSupervisor},
    models::{
        channel_monitors::{
            ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun,
            CompletedMonitorProbe, CompletedMonitorRequestEvidence, CreateChannelMonitorRunInput,
            MonitorProbeUsageEvidence, MonitorRequestPricingEvidence,
        },
        pricing::RequestUsage,
        routing::RuntimeRoutingCandidate,
    },
    outbound::AsyncOutboundClient,
    services::channel_monitors::{
        probe::{run_monitor_probe, MonitorProbeResult},
        redaction::redact_monitor_text,
        templates::{render_monitor_request, MonitorTemplateContext, RenderedMonitorRequest},
    },
    services::pricing::request_cost_from_pricing_parts_and_usage,
};
use futures_util::future::{join_all, BoxFuture};
use tokio_util::sync::CancellationToken;

const RUNNER_POLL_INTERVAL: Duration = Duration::from_secs(30);
const RUNNER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const RUNNER_TASK_ID: &str = "channel-monitor-runner";
const RUNNER_TASK_KIND: &str = "channel_monitor_runner";
const RUNNER_CONCURRENCY_KEY: &str = "channel-monitor-runner";
const DEFAULT_MONITOR_MODEL: &str = "gpt-4.1-mini";
const DEFAULT_MONITOR_CHALLENGE: &str = "ping";
const MONITOR_ALREADY_RUNNING_ERROR: &str = "Channel monitor is already running";
static ACTIVE_MONITOR_RUNS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(crate) fn v2_runner_port(
    services: &AppServices,
    outbound: AsyncOutboundClient,
) -> Arc<dyn ChannelMonitorRunnerPort> {
    let persistence: Arc<dyn ChannelMonitorPersistencePort> = Arc::new(
        V2ChannelMonitorPersistenceAdapter::new((*services.monitoring).clone()),
    );
    let probes: Arc<dyn ChannelMonitorProbePort> = Arc::new(V2ChannelMonitorProbeAdapter::new(
        services.routing.clone(),
        services.credentials.clone(),
        services.pricing.clone(),
        outbound,
    ));
    Arc::new(V2ChannelMonitorRunnerAdapter::new(persistence, probes))
}

#[allow(dead_code)]
pub(crate) trait ChannelMonitorPersistencePort: Send + Sync {
    fn get_monitor(
        &self,
        monitor_id: String,
    ) -> futures_util::future::BoxFuture<'static, Result<ChannelMonitor, ApplicationError>>;

    fn get_template(
        &self,
        template_id: String,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<ChannelMonitorRequestTemplate, ApplicationError>,
    >;

    fn due_monitors(
        &self,
        limit: u32,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<ChannelMonitor>, ApplicationError>>;

    fn record_probe_outcome(
        &self,
        outcome: CompletedMonitorProbe,
    ) -> futures_util::future::BoxFuture<'static, Result<ChannelMonitorRun, ApplicationError>>;
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct V2ChannelMonitorPersistenceAdapter {
    monitoring: MonitoringService,
}

#[allow(dead_code)]
impl V2ChannelMonitorPersistenceAdapter {
    pub(crate) fn new(monitoring: MonitoringService) -> Self {
        Self { monitoring }
    }
}

impl ChannelMonitorPersistencePort for V2ChannelMonitorPersistenceAdapter {
    fn get_monitor(
        &self,
        monitor_id: String,
    ) -> futures_util::future::BoxFuture<'static, Result<ChannelMonitor, ApplicationError>> {
        let monitoring = self.monitoring.clone();
        Box::pin(async move { monitoring.get_monitor(&monitor_id).await })
    }

    fn get_template(
        &self,
        template_id: String,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<ChannelMonitorRequestTemplate, ApplicationError>,
    > {
        let monitoring = self.monitoring.clone();
        Box::pin(async move { monitoring.get_template(&template_id).await })
    }

    fn due_monitors(
        &self,
        limit: u32,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<ChannelMonitor>, ApplicationError>>
    {
        let monitoring = self.monitoring.clone();
        Box::pin(async move {
            let limit = PageLimit::new(limit)?;
            monitoring.due_monitors(limit).await
        })
    }

    fn record_probe_outcome(
        &self,
        outcome: CompletedMonitorProbe,
    ) -> futures_util::future::BoxFuture<'static, Result<ChannelMonitorRun, ApplicationError>> {
        let monitoring = self.monitoring.clone();
        Box::pin(async move { monitoring.record_probe_outcome(outcome).await })
    }
}

/// V2-owned monitor read boundary shared by manual and background execution.
/// Keeping this composition helper separate prevents a run from accidentally
/// crossing back into the removed legacy persistence path.
pub(crate) async fn load_monitor_v2(
    port: &dyn ChannelMonitorPersistencePort,
    monitor_id: String,
) -> Result<(ChannelMonitor, ChannelMonitorRequestTemplate), ApplicationError> {
    let monitor = port.get_monitor(monitor_id).await?;
    let template = port.get_template(monitor.template_id.clone()).await?;
    Ok((monitor, template))
}

/// Records one monitor result through the V2 application service.
pub(crate) async fn record_monitor_run_v2(
    port: &dyn ChannelMonitorPersistencePort,
    outcome: CompletedMonitorProbe,
) -> Result<ChannelMonitorRun, ApplicationError> {
    port.record_probe_outcome(outcome).await
}

pub(crate) trait ChannelMonitorProbePort: Send + Sync + 'static {
    /// Executes network probes only. Persistence remains owned by the V2
    /// runner adapter, so a probe implementation cannot partially write V1.
    fn probe(
        &self,
        monitor: ChannelMonitor,
        template: ChannelMonitorRequestTemplate,
        cancellation_token: CancellationToken,
    ) -> BoxFuture<'static, Result<Vec<CompletedMonitorProbe>, String>>;
}

#[derive(Clone)]
pub(crate) struct V2ChannelMonitorProbeAdapter {
    routing: Arc<RoutingService>,
    credentials: Arc<CredentialService>,
    pricing: Arc<PricingService>,
    outbound: AsyncOutboundClient,
}

impl V2ChannelMonitorProbeAdapter {
    pub(crate) fn new(
        routing: Arc<RoutingService>,
        credentials: Arc<CredentialService>,
        pricing: Arc<PricingService>,
        outbound: AsyncOutboundClient,
    ) -> Self {
        Self {
            routing,
            credentials,
            pricing,
            outbound,
        }
    }
}

impl ChannelMonitorProbePort for V2ChannelMonitorProbeAdapter {
    fn probe(
        &self,
        monitor: ChannelMonitor,
        template: ChannelMonitorRequestTemplate,
        cancellation_token: CancellationToken,
    ) -> BoxFuture<'static, Result<Vec<CompletedMonitorProbe>, String>> {
        let routing = self.routing.clone();
        let credentials = self.credentials.clone();
        let pricing = self.pricing.clone();
        let outbound = self.outbound.clone();
        Box::pin(async move {
            let targets = routing
                .load_runtime_candidates()
                .await
                .map_err(|error| error.to_string())?
                .into_iter()
                .filter(|candidate| match monitor.target_type.as_str() {
                    "station_key" => {
                        monitor.station_key_id.as_deref() == Some(candidate.station_key_id.as_str())
                    }
                    "station" => candidate.station_id == monitor.station_id,
                    _ => false,
                })
                .collect::<Vec<_>>();

            if targets.is_empty() {
                let now = now_string_v2();
                let model = monitor_model(&monitor);
                return Ok(vec![CompletedMonitorProbe {
                    run: CreateChannelMonitorRunInput {
                        monitor_id: monitor.id,
                        template_id: monitor.template_id,
                        station_id: monitor.station_id,
                        station_key_id: monitor.station_key_id,
                        status: "skipped".to_string(),
                        started_at: now.clone(),
                        finished_at: Some(now),
                        duration_ms: Some(0),
                        http_status: None,
                        latency_ms: None,
                        response_model: Some(model),
                        fallback_model: None,
                        error_message: Some(
                            "No enabled station keys matched monitor target".to_string(),
                        ),
                    },
                    request: None,
                }]);
            }

            let max_concurrency = if monitor.target_type == "station" {
                monitor.max_concurrency.clamp(1, 16) as usize
            } else {
                1
            };
            let mut runs = Vec::with_capacity(targets.len());
            for batch in targets.chunks(max_concurrency) {
                let mut workers = Vec::with_capacity(batch.len());
                for target in batch {
                    let started_at = now_string_v2();
                    let model = monitor_model(&monitor);
                    let api_key = match credentials
                        .resolve_station_key_secret(target.station_key_id.clone())
                        .await
                    {
                        Ok(secret) => match String::from_utf8(secret.as_bytes().to_vec()) {
                            Ok(secret) => zeroize::Zeroizing::new(secret),
                            Err(_) => {
                                runs.push(failed_probe_input_v2(
                                    &monitor,
                                    target,
                                    started_at,
                                    model,
                                    "station key secret is not valid UTF-8".to_string(),
                                ));
                                continue;
                            }
                        },
                        Err(error) => {
                            runs.push(failed_probe_input_v2(
                                &monitor,
                                target,
                                started_at,
                                model,
                                short_error(&error.to_string()),
                            ));
                            continue;
                        }
                    };
                    let context = MonitorTemplateContext {
                        model: model.clone(),
                        max_tokens: 1,
                        stream: true,
                        challenge: DEFAULT_MONITOR_CHALLENGE.to_string(),
                    };
                    let request = match render_monitor_request(&template, &context) {
                        Ok(request) => request,
                        Err(error) => {
                            runs.push(failed_probe_input_v2(
                                &monitor,
                                target,
                                started_at,
                                model,
                                short_error(&error),
                            ));
                            continue;
                        }
                    };
                    let target = target.clone();
                    let monitor = monitor.clone();
                    let endpoint = template.endpoint_kind.clone();
                    let outbound = outbound.clone();
                    let cancellation_token = cancellation_token.clone();
                    workers.push(async move {
                        let result = run_monitor_probe(
                            &outbound,
                            &target.upstream_base_url,
                            api_key.as_str(),
                            &request,
                            monitor.timeout_seconds,
                            cancellation_token,
                        )
                        .await;
                        CompletedProbeInput {
                            monitor,
                            target,
                            started_at,
                            model,
                            endpoint,
                            request,
                            result,
                        }
                    });
                }
                for completed in join_all(workers).await {
                    runs.push(probe_outcome_v2(completed, pricing.as_ref()).await);
                }
            }
            Ok(runs)
        })
    }
}

struct CompletedProbeInput {
    monitor: ChannelMonitor,
    target: RuntimeRoutingCandidate,
    started_at: String,
    model: String,
    endpoint: String,
    request: RenderedMonitorRequest,
    result: MonitorProbeResult,
}

async fn probe_outcome_v2(
    completed: CompletedProbeInput,
    pricing: &PricingService,
) -> CompletedMonitorProbe {
    let CompletedProbeInput {
        monitor,
        target,
        started_at,
        model,
        endpoint,
        request,
        result,
    } = completed;
    let finished_at = now_string_v2();
    let duration_ms = duration_between(&started_at, &finished_at);
    let usage = result.usage.map(|usage| MonitorProbeUsageEvidence {
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        cache_creation_tokens: usage.cache_creation_tokens,
        cache_read_tokens: usage.cache_read_tokens,
    });
    let pricing_evidence = pricing
        .estimate_monitor_request_cost(&target.station_key_id, &model, usage.as_ref())
        .await
        .unwrap_or_else(|_| usage_only_pricing(usage.as_ref()));
    let error_message = result.error_summary.map(|error| short_error(&error));
    CompletedMonitorProbe {
        run: CreateChannelMonitorRunInput {
            monitor_id: monitor.id,
            template_id: monitor.template_id,
            station_id: target.station_id.clone(),
            station_key_id: Some(target.station_key_id.clone()),
            status: if result.ok { "success" } else { "failed" }.to_string(),
            started_at: started_at.clone(),
            finished_at: Some(finished_at.clone()),
            duration_ms,
            http_status: result.status_code.map(i64::from),
            latency_ms: Some(result.latency_ms),
            response_model: Some(model.clone()),
            fallback_model: None,
            error_message,
        },
        request: Some(CompletedMonitorRequestEvidence {
            method: request.method,
            path: request.path,
            endpoint,
            model,
            stream: request.stream,
            reasoning_effort: request.reasoning_effort,
            station_key_id: target.station_key_id.clone(),
            station_id: target.station_id.clone(),
            upstream_base_url: target.upstream_base_url.clone(),
            first_token_ms: result.first_token_ms,
            usage,
            pricing: pricing_evidence,
        }),
    }
}

fn failed_probe_input_v2(
    monitor: &ChannelMonitor,
    target: &RuntimeRoutingCandidate,
    started_at: String,
    model: String,
    error_message: String,
) -> CompletedMonitorProbe {
    let finished_at = now_string_v2();
    CompletedMonitorProbe {
        run: CreateChannelMonitorRunInput {
            monitor_id: monitor.id.clone(),
            template_id: monitor.template_id.clone(),
            station_id: target.station_id.clone(),
            station_key_id: Some(target.station_key_id.clone()),
            status: "failed".to_string(),
            started_at: started_at.clone(),
            finished_at: Some(finished_at.clone()),
            duration_ms: duration_between(&started_at, &finished_at),
            http_status: None,
            latency_ms: None,
            response_model: Some(model),
            fallback_model: None,
            error_message: Some(error_message),
        },
        request: None,
    }
}

fn usage_only_pricing(usage: Option<&MonitorProbeUsageEvidence>) -> MonitorRequestPricingEvidence {
    let usage = RequestUsage {
        input_tokens: usage.and_then(|usage| usage.prompt_tokens),
        output_tokens: usage.and_then(|usage| usage.completion_tokens),
        total_tokens: usage.and_then(|usage| usage.total_tokens),
        request_count: Some(1),
        cache_creation_tokens: usage.and_then(|usage| usage.cache_creation_tokens),
        cache_read_tokens: usage.and_then(|usage| usage.cache_read_tokens),
        media_count: None,
        duration_seconds: None,
        size_tier: None,
    };
    MonitorRequestPricingEvidence {
        estimate: request_cost_from_pricing_parts_and_usage(None, &usage),
        group_binding_id: None,
        normalization_status: None,
    }
}

fn now_string_v2() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

pub(crate) trait ChannelMonitorRunnerPort: Send + Sync + 'static {
    fn due_monitor_ids(&self, limit: u32) -> BoxFuture<'static, Result<Vec<String>, String>>;

    fn run_monitor(
        &self,
        monitor_id: String,
        cancellation_token: CancellationToken,
    ) -> BoxFuture<'static, Result<Vec<ChannelMonitorRun>, String>>;
}

pub(crate) struct V2ChannelMonitorRunnerAdapter {
    persistence: Arc<dyn ChannelMonitorPersistencePort>,
    probes: Arc<dyn ChannelMonitorProbePort>,
}

impl V2ChannelMonitorRunnerAdapter {
    pub(crate) fn new(
        persistence: Arc<dyn ChannelMonitorPersistencePort>,
        probes: Arc<dyn ChannelMonitorProbePort>,
    ) -> Self {
        Self {
            persistence,
            probes,
        }
    }
}

impl ChannelMonitorRunnerPort for V2ChannelMonitorRunnerAdapter {
    fn due_monitor_ids(&self, limit: u32) -> BoxFuture<'static, Result<Vec<String>, String>> {
        let persistence = self.persistence.clone();
        Box::pin(async move {
            persistence
                .due_monitors(limit)
                .await
                .map(|monitors| monitors.into_iter().map(|monitor| monitor.id).collect())
                .map_err(|error| error.to_string())
        })
    }

    fn run_monitor(
        &self,
        monitor_id: String,
        cancellation_token: CancellationToken,
    ) -> BoxFuture<'static, Result<Vec<ChannelMonitorRun>, String>> {
        let persistence = self.persistence.clone();
        let probes = self.probes.clone();
        Box::pin(async move {
            let (monitor, template) = load_monitor_v2(persistence.as_ref(), monitor_id)
                .await
                .map_err(|error| error.to_string())?;
            let pending_runs = probes.probe(monitor, template, cancellation_token).await?;
            if pending_runs.is_empty() {
                return Err("Channel monitor probe returned no terminal run".to_string());
            }
            let mut runs = Vec::with_capacity(pending_runs.len());
            for pending_run in pending_runs {
                runs.push(
                    record_monitor_run_v2(persistence.as_ref(), pending_run)
                        .await
                        .map_err(|error| error.to_string())?,
                );
            }
            Ok(runs)
        })
    }
}

fn is_monitor_already_running_error(error: &str) -> bool {
    error == MONITOR_ALREADY_RUNNING_ERROR
}

struct MonitorRunGuard {
    monitor_id: String,
}

impl MonitorRunGuard {
    fn try_start(monitor_id: &str) -> Result<Self, String> {
        let active_runs = ACTIVE_MONITOR_RUNS.get_or_init(|| Mutex::new(HashSet::new()));
        let mut active_runs = active_runs
            .lock()
            .map_err(|_| "Channel monitor run guard is unavailable".to_string())?;
        if !active_runs.insert(monitor_id.to_string()) {
            return Err(MONITOR_ALREADY_RUNNING_ERROR.to_string());
        }
        Ok(Self {
            monitor_id: monitor_id.to_string(),
        })
    }
}

impl Drop for MonitorRunGuard {
    fn drop(&mut self) {
        if let Some(active_runs) = ACTIVE_MONITOR_RUNS.get() {
            if let Ok(mut active_runs) = active_runs.lock() {
                active_runs.remove(&self.monitor_id);
            }
        }
    }
}

fn monitor_model(monitor: &ChannelMonitor) -> String {
    monitor
        .fallback_models
        .first()
        .cloned()
        .unwrap_or_else(|| DEFAULT_MONITOR_MODEL.to_string())
}

fn duration_between(started_at: &str, finished_at: &str) -> Option<i64> {
    let started_at = started_at.parse::<i64>().ok()?;
    let finished_at = finished_at.parse::<i64>().ok()?;
    Some((finished_at - started_at).max(0))
}

fn short_error(error: &str) -> String {
    let redacted = redact_monitor_text(error);
    redacted.chars().take(240).collect()
}

pub struct ChannelMonitorRunnerState {
    supervisor: TaskSupervisor,
    task_id: TaskId,
}

impl ChannelMonitorRunnerState {
    pub(crate) fn start_v2(
        supervisor: TaskSupervisor,
        port: Arc<dyn ChannelMonitorRunnerPort>,
    ) -> Result<Self, String> {
        let task_id = TaskId::from(RUNNER_TASK_ID);
        let runner_port = Arc::clone(&port);
        supervisor
            .register(
                TaskSpec::new(task_id.clone(), RUNNER_TASK_KIND, move |context| {
                    let port = Arc::clone(&runner_port);
                    Box::pin(runner_loop_v2(port, context))
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

async fn runner_loop_v2(
    port: Arc<dyn ChannelMonitorRunnerPort>,
    context: TaskRunContext,
) -> Result<(), TaskFailure> {
    let mut interval = tokio::time::interval(RUNNER_POLL_INTERVAL);
    loop {
        tokio::select! {
            _ = context.cancellation_token.cancelled() => {
                return Err(TaskFailure::cancelled());
            }
            _ = interval.tick() => {
                run_due_channel_monitors_once_v2(port.as_ref(), &context).await;
            }
        }
    }
}

async fn run_due_channel_monitors_once_v2(
    port: &dyn ChannelMonitorRunnerPort,
    context: &TaskRunContext,
) {
    match port.due_monitor_ids(256).await {
        Ok(monitor_ids) => {
            for monitor_id in monitor_ids {
                if context.cancellation_token.is_cancelled() {
                    break;
                }
                if let Err(error) = run_channel_monitor_guarded_v2(port, monitor_id, context).await
                {
                    if !is_monitor_already_running_error(&error) {
                        eprintln!("Channel monitor runner failed: {error}");
                    }
                }
            }
        }
        Err(error) => eprintln!("Channel monitor runner could not query due monitors: {error}"),
    }
}

async fn run_channel_monitor_guarded_v2(
    port: &dyn ChannelMonitorRunnerPort,
    monitor_id: String,
    context: &TaskRunContext,
) -> Result<Vec<ChannelMonitorRun>, String> {
    let _guard = MonitorRunGuard::try_start(&monitor_id)?;
    port.run_monitor(monitor_id, context.cancellation_token.clone())
        .await
}

impl Drop for ChannelMonitorRunnerState {
    fn drop(&mut self) {
        let _ = self.supervisor.cancel(&self.task_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background_tasks::{TaskRunId, TaskState};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::{oneshot, Notify};
    use tokio_util::sync::CancellationToken;

    struct StubPersistence {
        monitor: ChannelMonitor,
        template: ChannelMonitorRequestTemplate,
        recorded: Arc<Mutex<Vec<CompletedMonitorProbe>>>,
    }

    impl StubPersistence {
        fn new() -> Self {
            Self {
                monitor: sample_monitor(),
                template: sample_template(),
                recorded: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl ChannelMonitorPersistencePort for StubPersistence {
        fn get_monitor(
            &self,
            monitor_id: String,
        ) -> BoxFuture<'static, Result<ChannelMonitor, ApplicationError>> {
            let result = (monitor_id == self.monitor.id)
                .then(|| self.monitor.clone())
                .ok_or(ApplicationError::NotFound);
            Box::pin(async move { result })
        }

        fn get_template(
            &self,
            template_id: String,
        ) -> BoxFuture<'static, Result<ChannelMonitorRequestTemplate, ApplicationError>> {
            let result = (template_id == self.template.id)
                .then(|| self.template.clone())
                .ok_or(ApplicationError::NotFound);
            Box::pin(async move { result })
        }

        fn due_monitors(
            &self,
            limit: u32,
        ) -> BoxFuture<'static, Result<Vec<ChannelMonitor>, ApplicationError>> {
            let monitors = if limit == 0 {
                Vec::new()
            } else {
                vec![self.monitor.clone()]
            };
            Box::pin(async move { Ok(monitors) })
        }

        fn record_probe_outcome(
            &self,
            outcome: CompletedMonitorProbe,
        ) -> BoxFuture<'static, Result<ChannelMonitorRun, ApplicationError>> {
            let recorded = Arc::clone(&self.recorded);
            Box::pin(async move {
                recorded
                    .lock()
                    .expect("recorded runs")
                    .push(outcome.clone());
                Ok(stored_run(outcome.run))
            })
        }
    }

    struct StubProbe {
        outputs: Vec<CompletedMonitorProbe>,
    }

    impl ChannelMonitorProbePort for StubProbe {
        fn probe(
            &self,
            _monitor: ChannelMonitor,
            _template: ChannelMonitorRequestTemplate,
            _cancellation_token: CancellationToken,
        ) -> BoxFuture<'static, Result<Vec<CompletedMonitorProbe>, String>> {
            let outputs = self.outputs.clone();
            Box::pin(async move { Ok(outputs) })
        }
    }

    #[test]
    fn v2_runner_records_probe_results_through_persistence_port() {
        let persistence = Arc::new(StubPersistence::new());
        let recorded = Arc::clone(&persistence.recorded);
        let runner = V2ChannelMonitorRunnerAdapter::new(
            persistence,
            Arc::new(StubProbe {
                outputs: vec![CompletedMonitorProbe {
                    run: sample_run_input(),
                    request: None,
                }],
            }),
        );

        let runs = tauri::async_runtime::block_on(
            runner.run_monitor("monitor-1".to_string(), CancellationToken::new()),
        )
        .expect("V2 monitor run");

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "success");
        assert_eq!(recorded.lock().expect("recorded runs").len(), 1);
    }

    #[test]
    fn v2_runner_rejects_probe_without_terminal_result() {
        let persistence = Arc::new(StubPersistence::new());
        let recorded = Arc::clone(&persistence.recorded);
        let runner = V2ChannelMonitorRunnerAdapter::new(
            persistence,
            Arc::new(StubProbe {
                outputs: Vec::new(),
            }),
        );

        let error = tauri::async_runtime::block_on(
            runner.run_monitor("monitor-1".to_string(), CancellationToken::new()),
        )
        .expect_err("empty probe result must fail closed");

        assert_eq!(error, "Channel monitor probe returned no terminal run");
        assert!(recorded.lock().expect("recorded runs").is_empty());
    }

    #[tokio::test]
    async fn due_runner_runs_each_due_monitor_once_in_order() {
        let context = test_run_context();
        let port = RecordingMonitorRunnerPort::new(
            vec!["monitor-1".to_string(), "monitor-2".to_string()],
            vec![Ok(Vec::new()), Ok(Vec::new())],
        );

        run_due_channel_monitors_once_v2(&port, &context).await;

        assert_eq!(
            port.calls(),
            vec!["monitor-1".to_string(), "monitor-2".to_string()]
        );
    }

    #[tokio::test]
    async fn due_runner_stops_between_monitors_when_cancel_is_requested() {
        let context = test_run_context();
        let port = StopAfterFirstMonitorRunnerPort::new(
            context.cancellation_token.clone(),
            vec!["monitor-stop-1".to_string(), "monitor-stop-2".to_string()],
        );

        run_due_channel_monitors_once_v2(&port, &context).await;

        assert_eq!(port.calls(), vec!["monitor-stop-1".to_string()]);
    }

    #[tokio::test]
    async fn guarded_monitor_run_rejects_same_monitor_reentry_until_guard_drops() {
        let notify_started = Arc::new(Notify::new());
        let (release_sender, release_receiver) = oneshot::channel();
        let port = BlockingFirstMonitorRunnerPort::new(
            Arc::clone(&notify_started),
            Mutex::new(Some(release_receiver)),
        );
        let running_context = test_run_context();
        let running = tokio::spawn(async move {
            run_channel_monitor_guarded_v2(&port, "monitor-guarded".to_string(), &running_context)
                .await
        });
        notify_started.notified().await;

        let duplicate = RecordingMonitorRunnerPort::new(Vec::new(), vec![Ok(Vec::new())]);
        let duplicate_context = test_run_context();
        let duplicate_result = run_channel_monitor_guarded_v2(
            &duplicate,
            "monitor-guarded".to_string(),
            &duplicate_context,
        )
        .await;
        assert_eq!(
            duplicate_result.expect_err("duplicate monitor run must be rejected"),
            MONITOR_ALREADY_RUNNING_ERROR
        );
        assert!(duplicate.calls().is_empty());

        release_sender.send(()).expect("release first run");
        running
            .await
            .expect("first run joins")
            .expect("first run succeeds");

        let after_release = RecordingMonitorRunnerPort::new(Vec::new(), vec![Ok(Vec::new())]);
        let after_release_context = test_run_context();
        run_channel_monitor_guarded_v2(
            &after_release,
            "monitor-guarded".to_string(),
            &after_release_context,
        )
        .await
        .expect("guard is released after first run");
        assert_eq!(after_release.calls(), vec!["monitor-guarded".to_string()]);
    }

    struct RecordingMonitorRunnerPort {
        due: Vec<String>,
        calls: Arc<Mutex<Vec<String>>>,
        results: Arc<Mutex<Vec<Result<Vec<ChannelMonitorRun>, String>>>>,
    }

    impl RecordingMonitorRunnerPort {
        fn new(due: Vec<String>, results: Vec<Result<Vec<ChannelMonitorRun>, String>>) -> Self {
            Self {
                due,
                calls: Arc::new(Mutex::new(Vec::new())),
                results: Arc::new(Mutex::new(results)),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls").clone()
        }
    }

    impl ChannelMonitorRunnerPort for RecordingMonitorRunnerPort {
        fn due_monitor_ids(&self, _limit: u32) -> BoxFuture<'static, Result<Vec<String>, String>> {
            let due = self.due.clone();
            Box::pin(async move { Ok(due) })
        }

        fn run_monitor(
            &self,
            monitor_id: String,
            _cancellation_token: CancellationToken,
        ) -> BoxFuture<'static, Result<Vec<ChannelMonitorRun>, String>> {
            self.calls.lock().expect("calls").push(monitor_id);
            let result = self
                .results
                .lock()
                .expect("results")
                .pop()
                .unwrap_or_else(|| Ok(Vec::new()));
            Box::pin(async move { result })
        }
    }

    struct StopAfterFirstMonitorRunnerPort {
        cancellation_token: CancellationToken,
        due: Vec<String>,
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl StopAfterFirstMonitorRunnerPort {
        fn new(cancellation_token: CancellationToken, due: Vec<String>) -> Self {
            Self {
                cancellation_token,
                due,
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls").clone()
        }
    }

    impl ChannelMonitorRunnerPort for StopAfterFirstMonitorRunnerPort {
        fn due_monitor_ids(&self, _limit: u32) -> BoxFuture<'static, Result<Vec<String>, String>> {
            let due = self.due.clone();
            Box::pin(async move { Ok(due) })
        }

        fn run_monitor(
            &self,
            monitor_id: String,
            _cancellation_token: CancellationToken,
        ) -> BoxFuture<'static, Result<Vec<ChannelMonitorRun>, String>> {
            self.calls.lock().expect("calls").push(monitor_id);
            self.cancellation_token.cancel();
            Box::pin(async move { Ok(Vec::new()) })
        }
    }

    struct BlockingFirstMonitorRunnerPort {
        notify_started: Arc<Notify>,
        release_receiver: Mutex<Option<oneshot::Receiver<()>>>,
        calls: AtomicUsize,
    }

    impl BlockingFirstMonitorRunnerPort {
        fn new(
            notify_started: Arc<Notify>,
            release_receiver: Mutex<Option<oneshot::Receiver<()>>>,
        ) -> Self {
            Self {
                notify_started,
                release_receiver,
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl ChannelMonitorRunnerPort for BlockingFirstMonitorRunnerPort {
        fn due_monitor_ids(&self, _limit: u32) -> BoxFuture<'static, Result<Vec<String>, String>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn run_monitor(
            &self,
            _monitor_id: String,
            _cancellation_token: CancellationToken,
        ) -> BoxFuture<'static, Result<Vec<ChannelMonitorRun>, String>> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                let notify_started = Arc::clone(&self.notify_started);
                let receiver = self
                    .release_receiver
                    .lock()
                    .expect("release receiver")
                    .take()
                    .expect("first call has release receiver");
                Box::pin(async move {
                    notify_started.notify_waiters();
                    receiver
                        .await
                        .map(|_| Vec::new())
                        .map_err(|_| "release dropped".to_string())
                })
            } else {
                Box::pin(async { Ok(Vec::new()) })
            }
        }
    }

    #[tokio::test]
    async fn start_v2_registers_runner_with_supervisor_and_stop_cancels_task() {
        let supervisor = TaskSupervisor::new();
        let task_id = TaskId::from(RUNNER_TASK_ID);
        let port = Arc::new(RecordingMonitorRunnerPort::new(Vec::new(), Vec::new()));

        let runner =
            ChannelMonitorRunnerState::start_v2(supervisor.clone(), port).expect("runner starts");

        assert_eq!(
            supervisor.status(&task_id).expect("runner status").state,
            TaskState::Running
        );

        runner.stop();
        assert_eq!(
            supervisor.status(&task_id).expect("runner status").state,
            TaskState::Stopping
        );
    }

    fn test_run_context() -> TaskRunContext {
        TaskRunContext {
            task_id: TaskId::from(RUNNER_TASK_ID),
            run_id: TaskRunId(1),
            cancellation_token: CancellationToken::new(),
        }
    }

    fn sample_monitor() -> ChannelMonitor {
        ChannelMonitor {
            id: "monitor-1".to_string(),
            name: "V2 monitor".to_string(),
            target_type: "station".to_string(),
            station_id: "station-1".to_string(),
            station_key_id: None,
            template_id: "template-1".to_string(),
            enabled: true,
            interval_seconds: 60,
            jitter_seconds: 0,
            timeout_seconds: 5,
            max_concurrency: 2,
            consecutive_failure_threshold: 3,
            fallback_models: Vec::new(),
            note: None,
            created_at: "1".to_string(),
            updated_at: "1".to_string(),
        }
    }

    fn sample_template() -> ChannelMonitorRequestTemplate {
        ChannelMonitorRequestTemplate {
            id: "template-1".to_string(),
            name: "V2 template".to_string(),
            endpoint_kind: "chat_completions".to_string(),
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            request_body_json: "{}".to_string(),
            enabled: true,
            built_in: false,
            note: None,
            created_at: "1".to_string(),
            updated_at: "1".to_string(),
        }
    }

    fn sample_run_input() -> CreateChannelMonitorRunInput {
        CreateChannelMonitorRunInput {
            monitor_id: "monitor-1".to_string(),
            template_id: "template-1".to_string(),
            station_id: "station-1".to_string(),
            station_key_id: Some("key-1".to_string()),
            status: "success".to_string(),
            started_at: "10".to_string(),
            finished_at: Some("20".to_string()),
            duration_ms: Some(10),
            http_status: Some(200),
            latency_ms: Some(10),
            response_model: Some("gpt-test".to_string()),
            fallback_model: None,
            error_message: None,
        }
    }

    fn stored_run(input: CreateChannelMonitorRunInput) -> ChannelMonitorRun {
        ChannelMonitorRun {
            id: "run-1".to_string(),
            monitor_id: input.monitor_id,
            template_id: input.template_id,
            station_id: input.station_id,
            station_key_id: input.station_key_id,
            status: input.status,
            started_at: input.started_at,
            finished_at: input.finished_at,
            duration_ms: input.duration_ms,
            http_status: input.http_status,
            latency_ms: input.latency_ms,
            response_model: input.response_model,
            fallback_model: input.fallback_model,
            error_message: input.error_message,
            created_at: "20".to_string(),
        }
    }
}
