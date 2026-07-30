use std::{
    collections::HashSet,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use futures_util::future::BoxFuture;

use crate::{
    application::{app_services::AppServices, collectors::CollectorService, pagination::PageLimit},
    background_tasks::{
        BlockingExecutor, BlockingExecutorError, TaskFailure, TaskId, TaskRunContext, TaskSpec,
        TaskSupervisor,
    },
    outbound::AsyncOutboundClient,
    services::collectors::{
        self,
        apply::{CollectorApplyPort, V2CollectorApplyAdapter},
        output::CollectorTask,
        CollectorSourcePort, V2CollectorSourceAdapter,
    },
};

const COLLECTOR_BACKGROUND_INTERVAL: Duration = Duration::from_secs(30);
const RUNNER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const RUNNER_TASK_ID: &str = "station-collector-runner";
const RUNNER_TASK_KIND: &str = "station_collector_runner";
const RUNNER_CONCURRENCY_KEY: &str = "station-collector-runner";
static ACTIVE_STATION_RUNS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(crate) fn v2_runner_port(
    services: &AppServices,
    blocking: BlockingExecutor,
    outbound: AsyncOutboundClient,
    providers: Arc<collectors::orchestration::ProviderRegistry>,
    data_key: [u8; 32],
) -> Arc<dyn StationCollectorRunnerPort> {
    let source: Arc<dyn CollectorSourcePort> = Arc::new(V2CollectorSourceAdapter::new(
        services.collectors.clone(),
        services.credentials.clone(),
        services.settings.clone(),
    ));
    let apply: Arc<dyn CollectorApplyPort> =
        Arc::new(V2CollectorApplyAdapter::new((*services.collectors).clone()));
    let tasks: Arc<dyn StationCollectorTaskPort> = Arc::new(V2StationCollectorTaskAdapter::new(
        source, apply, blocking, outbound, providers, data_key,
    ));
    Arc::new(V2StationCollectorRunnerAdapter::new(
        services.collectors.clone(),
        tasks,
    ))
}

pub(crate) trait StationCollectorTaskPort: Send + Sync + 'static {
    fn collect_task(
        &self,
        station_id: String,
        task: CollectorTask,
        context: StationCollectorTaskContext,
    ) -> BoxFuture<'static, Result<(), String>>;
}

#[derive(Clone)]
pub(crate) struct StationCollectorTaskContext {
    task_id: TaskId,
    run_id: u64,
    correlation_id: String,
    cancellation_token: tokio_util::sync::CancellationToken,
}

impl From<&TaskRunContext> for StationCollectorTaskContext {
    fn from(context: &TaskRunContext) -> Self {
        Self {
            task_id: context.task_id.clone(),
            run_id: context.run_id.0,
            correlation_id: context.correlation_id.clone(),
            cancellation_token: context.cancellation_token.clone(),
        }
    }
}

pub(crate) struct V2StationCollectorTaskAdapter {
    source: Arc<dyn CollectorSourcePort>,
    apply: Arc<dyn CollectorApplyPort>,
    blocking: BlockingExecutor,
    outbound: AsyncOutboundClient,
    providers: Arc<collectors::orchestration::ProviderRegistry>,
    data_key: [u8; 32],
}

impl V2StationCollectorTaskAdapter {
    pub(crate) fn new(
        source: Arc<dyn CollectorSourcePort>,
        apply: Arc<dyn CollectorApplyPort>,
        blocking: BlockingExecutor,
        outbound: AsyncOutboundClient,
        providers: Arc<collectors::orchestration::ProviderRegistry>,
        data_key: [u8; 32],
    ) -> Self {
        Self {
            source,
            apply,
            blocking,
            outbound,
            providers,
            data_key,
        }
    }
}

impl StationCollectorTaskPort for V2StationCollectorTaskAdapter {
    fn collect_task(
        &self,
        station_id: String,
        task: CollectorTask,
        context: StationCollectorTaskContext,
    ) -> BoxFuture<'static, Result<(), String>> {
        let source = self.source.clone();
        let apply = self.apply.clone();
        let blocking = self.blocking.clone();
        let outbound = self.outbound.clone();
        let providers = self.providers.clone();
        let data_key = self.data_key;
        Box::pin(async move {
            let operation_id = Some(format!("{}:{}", context.task_id, context.run_id));
            let prepare = blocking
                .submit(
                    "station_collector_prepare",
                    operation_id,
                    Some(context.correlation_id.clone()),
                    None,
                    move |_| {
                        Ok(collectors::prepare_station_task_route_v2(
                            source.as_ref(),
                            &data_key,
                            station_id,
                            task,
                        ))
                    },
                )
                .map_err(blocking_executor_error_message)?;
            let prepare_cancellation_token = prepare.cancellation_token();
            let prepared = tokio::select! {
                _ = context.cancellation_token.cancelled() => {
                    prepare_cancellation_token.cancel();
                    return Err("Station collector task was cancelled".to_string());
                }
                result = prepare.result() => {
                    result.map_err(blocking_executor_error_message)?
                }
            }
            .map_err(|error| error.to_string())?;
            let prepared = match prepared {
                collectors::PreparedStationTaskRoute::Sub2Api(prepared) => {
                    collectors::finish_sub2api_task_v2(
                        providers.as_ref(),
                        &outbound,
                        prepared,
                        context.cancellation_token.clone(),
                        Some(context.correlation_id.clone()),
                    )
                    .await
                    .map_err(|error| error.to_string())?
                }
                collectors::PreparedStationTaskRoute::OpenAiCompatible(prepared) => {
                    collectors::finish_openai_compatible_task_v2(
                        providers.as_ref(),
                        &outbound,
                        prepared,
                        context.cancellation_token.clone(),
                        Some(context.correlation_id.clone()),
                    )
                    .await
                    .map_err(|error| error.to_string())?
                }
                collectors::PreparedStationTaskRoute::NewApi(prepared) => {
                    collectors::finish_newapi_task_v2(
                        providers.as_ref(),
                        &outbound,
                        prepared,
                        context.cancellation_token.clone(),
                        Some(context.correlation_id.clone()),
                    )
                    .await
                    .map_err(|error| error.to_string())?
                }
            };
            collectors::apply_prepared_station_task_v2(
                apply.as_ref(),
                prepared.0,
                prepared.1,
                prepared.2,
            )
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
        })
    }
}

pub(crate) trait StationCollectorRunnerPort: Send + Sync + 'static {
    fn due_station_ids(&self, limit: u32) -> BoxFuture<'static, Result<Vec<String>, String>>;

    fn collect_task(
        &self,
        station_id: String,
        task: CollectorTask,
        context: StationCollectorTaskContext,
    ) -> BoxFuture<'static, Result<(), String>>;
}

pub(crate) struct V2StationCollectorRunnerAdapter {
    collectors: Arc<CollectorService>,
    tasks: Arc<dyn StationCollectorTaskPort>,
}

impl V2StationCollectorRunnerAdapter {
    pub(crate) fn new(
        collectors: Arc<CollectorService>,
        tasks: Arc<dyn StationCollectorTaskPort>,
    ) -> Self {
        Self { collectors, tasks }
    }
}

impl StationCollectorRunnerPort for V2StationCollectorRunnerAdapter {
    fn due_station_ids(&self, limit: u32) -> BoxFuture<'static, Result<Vec<String>, String>> {
        let collectors = self.collectors.clone();
        Box::pin(async move {
            let limit = PageLimit::new(limit).map_err(|error| error.to_string())?;
            collectors
                .due_stations(limit)
                .await
                .map(|stations| stations.into_iter().map(|station| station.id).collect())
                .map_err(|error| error.to_string())
        })
    }

    fn collect_task(
        &self,
        station_id: String,
        task: CollectorTask,
        context: StationCollectorTaskContext,
    ) -> BoxFuture<'static, Result<(), String>> {
        self.tasks.collect_task(station_id, task, context)
    }
}

pub struct StationCollectorRunnerState {
    supervisor: TaskSupervisor,
    task_id: TaskId,
}

impl StationCollectorRunnerState {
    #[allow(dead_code)]
    pub fn stop(&self) {
        let _ = self.supervisor.cancel(&self.task_id);
    }

    pub(crate) fn start_v2(
        supervisor: TaskSupervisor,
        port: Arc<dyn StationCollectorRunnerPort>,
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
}

async fn runner_loop_v2(
    port: Arc<dyn StationCollectorRunnerPort>,
    context: TaskRunContext,
) -> Result<(), TaskFailure> {
    let mut interval = tokio::time::interval(COLLECTOR_BACKGROUND_INTERVAL);
    loop {
        tokio::select! {
            _ = context.cancellation_token.cancelled() => {
                return Err(TaskFailure::cancelled());
            }
            _ = interval.tick() => {
                run_due_station_collections_once_v2(port.as_ref(), &context).await;
            }
        }
    }
}

async fn run_due_station_collections_once_v2(
    port: &dyn StationCollectorRunnerPort,
    context: &TaskRunContext,
) {
    match port.due_station_ids(256).await {
        Ok(station_ids) => {
            for station_id in station_ids {
                if context.cancellation_token.is_cancelled() {
                    break;
                }
                if let Err(error) =
                    run_station_collection_guarded_v2(port, &station_id, context).await
                {
                    eprintln!("Station collector runner failed for {station_id}: {error}");
                }
            }
        }
        Err(error) => {
            eprintln!("Station collector runner could not query due stations: {error}")
        }
    }
}

async fn run_station_collection_guarded_v2(
    port: &dyn StationCollectorRunnerPort,
    station_id: &str,
    context: &TaskRunContext,
) -> Result<(), String> {
    let _guard = StationCollectorRunGuard::try_start(station_id)?;
    let task_context = StationCollectorTaskContext::from(context);
    let balance_result = port
        .collect_task(
            station_id.to_string(),
            CollectorTask::Balance,
            task_context.clone(),
        )
        .await;
    let groups_result = port
        .collect_task(station_id.to_string(), CollectorTask::Groups, task_context)
        .await;
    combine_collection_results(balance_result, groups_result)
}

impl Drop for StationCollectorRunnerState {
    fn drop(&mut self) {
        let _ = self.supervisor.cancel(&self.task_id);
    }
}

fn combine_collection_results(
    balance_result: Result<(), String>,
    groups_result: Result<(), String>,
) -> Result<(), String> {
    match (balance_result, groups_result) {
        (Ok(_), Ok(_)) => Ok(()),
        (Err(balance_error), Ok(_)) => Err(balance_error),
        (Ok(_), Err(groups_error)) => Err(groups_error),
        (Err(balance_error), Err(groups_error)) => Err(format!(
            "balance collection failed: {balance_error}; group collection failed: {groups_error}"
        )),
    }
}

fn blocking_executor_error_message(error: BlockingExecutorError) -> String {
    match error {
        BlockingExecutorError::QueueFull => {
            "Station collector blocking capacity is full".to_string()
        }
        BlockingExecutorError::QueueTimeout => {
            "Station collector blocking capacity timed out".to_string()
        }
        BlockingExecutorError::ExecutionTimeout => {
            "Station collector blocking task timed out".to_string()
        }
        BlockingExecutorError::CancelledBeforeStart
        | BlockingExecutorError::CancelledLateResultDiscarded => {
            "Station collector task was cancelled".to_string()
        }
        BlockingExecutorError::Closed => {
            "Station collector blocking executor is closed".to_string()
        }
        BlockingExecutorError::Panicked => "Station collector blocking task panicked".to_string(),
        BlockingExecutorError::JobFailed { code } => {
            format!("Station collector blocking task failed: {code}")
        }
        BlockingExecutorError::ShutdownTimeout { .. } => {
            "Station collector blocking executor shutdown timed out".to_string()
        }
    }
}

struct StationCollectorRunGuard {
    station_id: String,
}

impl StationCollectorRunGuard {
    fn try_start(station_id: &str) -> Result<Self, String> {
        let active_runs = ACTIVE_STATION_RUNS.get_or_init(|| Mutex::new(HashSet::new()));
        let mut active_runs = active_runs
            .lock()
            .map_err(|_| "Station collector run guard is unavailable".to_string())?;
        if !active_runs.insert(station_id.to_string()) {
            return Err("Station collector is already running".to_string());
        }
        Ok(Self {
            station_id: station_id.to_string(),
        })
    }
}

impl Drop for StationCollectorRunGuard {
    fn drop(&mut self) {
        if let Some(active_runs) = ACTIVE_STATION_RUNS.get() {
            if let Ok(mut active_runs) = active_runs.lock() {
                active_runs.remove(&self.station_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background_tasks::{TaskRunId, TaskState};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    use tokio::sync::{oneshot, Notify};
    use tokio_util::sync::CancellationToken;

    #[test]
    fn combines_balance_and_group_collection_errors_without_losing_partial_failure() {
        assert_eq!(
            combine_collection_results(Ok(()), Err("groups failed".to_string())),
            Err("groups failed".to_string())
        );
        assert_eq!(
            combine_collection_results(Err("balance failed".to_string()), Ok(())),
            Err("balance failed".to_string())
        );
        assert_eq!(
            combine_collection_results(
                Err("balance failed".to_string()),
                Err("groups failed".to_string())
            ),
            Err(
                "balance collection failed: balance failed; group collection failed: groups failed"
                    .to_string()
            )
        );
    }

    #[tokio::test]
    async fn guarded_collection_runs_balance_then_groups_for_due_station() {
        let port = RecordingRunnerPort::new(vec![Ok(()), Ok(())]);
        let context = test_run_context();

        run_station_collection_guarded_v2(&port, "station-1", &context)
            .await
            .expect("guarded run succeeds");

        assert_eq!(
            port.calls(),
            vec![
                ("station-1".to_string(), CollectorTask::Balance),
                ("station-1".to_string(), CollectorTask::Groups),
            ]
        );
    }

    #[tokio::test]
    async fn guarded_collection_keeps_group_side_effect_after_balance_failure() {
        let port = RecordingRunnerPort::new(vec![Err("balance failed".to_string()), Ok(())]);
        let context = test_run_context();

        let result = run_station_collection_guarded_v2(&port, "station-2", &context).await;

        assert_eq!(result, Err("balance failed".to_string()));
        assert_eq!(
            port.calls(),
            vec![
                ("station-2".to_string(), CollectorTask::Balance),
                ("station-2".to_string(), CollectorTask::Groups),
            ]
        );
    }

    #[tokio::test]
    async fn guarded_collection_rejects_same_station_reentry_until_guard_drops() {
        let notify_started = Arc::new(Notify::new());
        let (release_sender, release_receiver) = oneshot::channel();
        let port = BlockingFirstTaskRunnerPort::new(
            Arc::clone(&notify_started),
            Mutex::new(Some(release_receiver)),
        );
        let running = tokio::spawn(async move {
            let context = test_run_context();
            run_station_collection_guarded_v2(&port, "station-guarded", &context).await
        });
        notify_started.notified().await;

        let duplicate = RecordingRunnerPort::new(vec![Ok(()), Ok(())]);
        let duplicate_context = test_run_context();
        let duplicate_result =
            run_station_collection_guarded_v2(&duplicate, "station-guarded", &duplicate_context)
                .await;
        assert_eq!(
            duplicate_result,
            Err("Station collector is already running".to_string())
        );
        assert!(duplicate.calls().is_empty());

        release_sender.send(()).expect("release first run");
        running
            .await
            .expect("first run joins")
            .expect("first run succeeds");

        let after_release = RecordingRunnerPort::new(vec![Ok(()), Ok(())]);
        let after_release_context = test_run_context();
        run_station_collection_guarded_v2(
            &after_release,
            "station-guarded",
            &after_release_context,
        )
        .await
        .expect("guard is released after first run");
        assert_eq!(after_release.calls().len(), 2);
    }

    struct RecordingRunnerPort {
        calls: Arc<Mutex<Vec<(String, CollectorTask)>>>,
        results: Arc<Mutex<Vec<Result<(), String>>>>,
    }

    impl RecordingRunnerPort {
        fn new(results: Vec<Result<(), String>>) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                results: Arc::new(Mutex::new(results)),
            }
        }

        fn calls(&self) -> Vec<(String, CollectorTask)> {
            self.calls.lock().expect("calls").clone()
        }
    }

    impl StationCollectorRunnerPort for RecordingRunnerPort {
        fn due_station_ids(&self, _limit: u32) -> BoxFuture<'static, Result<Vec<String>, String>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn collect_task(
            &self,
            station_id: String,
            task: CollectorTask,
            _context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<(), String>> {
            self.calls.lock().expect("calls").push((station_id, task));
            let result = self.results.lock().expect("results").remove(0);
            Box::pin(async move { result })
        }
    }

    struct BlockingFirstTaskRunnerPort {
        notify_started: Arc<Notify>,
        release_receiver: Mutex<Option<oneshot::Receiver<()>>>,
        calls: AtomicUsize,
    }

    impl BlockingFirstTaskRunnerPort {
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

    impl StationCollectorRunnerPort for BlockingFirstTaskRunnerPort {
        fn due_station_ids(&self, _limit: u32) -> BoxFuture<'static, Result<Vec<String>, String>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn collect_task(
            &self,
            _station_id: String,
            _task: CollectorTask,
            _context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<(), String>> {
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
                    receiver.await.map_err(|_| "release dropped".to_string())
                })
            } else {
                Box::pin(async { Ok(()) })
            }
        }
    }

    #[tokio::test]
    async fn start_v2_registers_runner_with_supervisor_and_stop_cancels_task() {
        let supervisor = TaskSupervisor::new();
        let task_id = TaskId::from(RUNNER_TASK_ID);
        let port = Arc::new(RecordingRunnerPort::new(Vec::new()));

        let runner =
            StationCollectorRunnerState::start_v2(supervisor.clone(), port).expect("runner starts");

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

    #[test]
    fn station_collector_blocking_errors_are_public_safe_messages() {
        assert_eq!(
            blocking_executor_error_message(BlockingExecutorError::QueueFull),
            "Station collector blocking capacity is full"
        );
        assert_eq!(
            blocking_executor_error_message(BlockingExecutorError::ExecutionTimeout),
            "Station collector blocking task timed out"
        );
        assert_eq!(
            blocking_executor_error_message(BlockingExecutorError::CancelledBeforeStart),
            "Station collector task was cancelled"
        );
    }

    fn test_run_context() -> TaskRunContext {
        TaskRunContext {
            task_id: TaskId::from(RUNNER_TASK_ID),
            run_id: TaskRunId(1),
            correlation_id: "test-correlation".to_string(),
            cancellation_token: CancellationToken::new(),
        }
    }
}
