use std::{sync::Arc, time::Duration};

pub(crate) mod runtime_events;

use futures_util::{future::BoxFuture, stream, StreamExt};

use crate::{
    application::{
        app_services::AppServices, collectors::CollectorService, pagination::PageLimit,
        settings::SettingsService,
    },
    background_tasks::{
        BlockingExecutor, BlockingExecutorError, TaskFailure, TaskId, TaskRunContext, TaskSpec,
        TaskSupervisor, TaskSupervisorError,
    },
    observability::correlation,
    outbound::AsyncOutboundClient,
    services::{
        collectors::{
            self,
            apply::{CollectorApplyPort, V2CollectorApplyAdapter},
            output::CollectorTask,
            CollectorSourcePort, V2CollectorSourceAdapter,
        },
        station_collection_coordinator::{
            StationCollectionAdmissionError, StationCollectionCoordinator,
        },
    },
};

const COLLECTOR_BACKGROUND_INTERVAL: Duration = Duration::from_secs(30);
const RUNNER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const RUNNER_TASK_ID: &str = "station-collector-runner";
const RUNNER_TASK_KIND: &str = "station_collector_runner";
const RUNNER_CONCURRENCY_KEY: &str = "station-collector-runner";

pub(crate) fn v2_runner_port(
    services: &AppServices,
    blocking: BlockingExecutor,
    outbound: AsyncOutboundClient,
    providers: Arc<collectors::orchestration::ProviderRegistry>,
    remote_keys: Arc<dyn StationCollectorRemoteKeyRefreshPort>,
) -> Arc<dyn StationCollectorRunnerPort> {
    let source: Arc<dyn CollectorSourcePort> = Arc::new(V2CollectorSourceAdapter::new(
        services.collectors.clone(),
        services.credentials.clone(),
        services.settings.clone(),
    ));
    let apply: Arc<dyn CollectorApplyPort> =
        Arc::new(V2CollectorApplyAdapter::new((*services.collectors).clone()));
    let tasks: Arc<dyn StationCollectorTaskPort> = Arc::new(V2StationCollectorTaskAdapter::new(
        source, apply, blocking, outbound, providers,
    ));
    Arc::new(V2StationCollectorRunnerAdapter::new(
        services.collectors.clone(),
        services.settings.clone(),
        tasks,
        remote_keys,
    ))
}

pub(crate) trait StationCollectorRemoteKeyRefreshPort: Send + Sync + 'static {
    fn refresh_remote_keys(
        &self,
        station_id: String,
        cancellation_token: tokio_util::sync::CancellationToken,
        correlation_id: Option<String>,
    ) -> BoxFuture<'static, Result<(), String>>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StationCollectorTaskOutcome {
    refresh_remote_keys: bool,
}

pub(crate) trait StationCollectorTaskPort: Send + Sync + 'static {
    fn collect_task(
        &self,
        station_id: String,
        task: CollectorTask,
        context: StationCollectorTaskContext,
    ) -> BoxFuture<'static, Result<StationCollectorTaskOutcome, String>>;
}

#[derive(Clone)]
pub(crate) struct StationCollectorTaskContext {
    task_id: TaskId,
    run_id: u64,
    correlation_id: String,
    cancellation_token: tokio_util::sync::CancellationToken,
}

pub(crate) struct V2StationCollectorTaskAdapter {
    source: Arc<dyn CollectorSourcePort>,
    apply: Arc<dyn CollectorApplyPort>,
    blocking: BlockingExecutor,
    outbound: AsyncOutboundClient,
    providers: Arc<collectors::orchestration::ProviderRegistry>,
}

impl V2StationCollectorTaskAdapter {
    pub(crate) fn new(
        source: Arc<dyn CollectorSourcePort>,
        apply: Arc<dyn CollectorApplyPort>,
        blocking: BlockingExecutor,
        outbound: AsyncOutboundClient,
        providers: Arc<collectors::orchestration::ProviderRegistry>,
    ) -> Self {
        Self {
            source,
            apply,
            blocking,
            outbound,
            providers,
        }
    }
}

impl StationCollectorTaskPort for V2StationCollectorTaskAdapter {
    fn collect_task(
        &self,
        station_id: String,
        task: CollectorTask,
        context: StationCollectorTaskContext,
    ) -> BoxFuture<'static, Result<StationCollectorTaskOutcome, String>> {
        let source = self.source.clone();
        let finish_source = self.source.clone();
        let apply = self.apply.clone();
        let blocking = self.blocking.clone();
        let outbound = self.outbound.clone();
        let providers = self.providers.clone();
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
                collectors::PreparedStationTaskRoute::NewApi(prepared) => {
                    collectors::finish_newapi_task_v2(
                        finish_source.as_ref(),
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
            let refresh_remote_keys = collectors::should_refresh_remote_keys_after_collection(
                task,
                prepared.2.status.as_str(),
            );
            collectors::apply_prepared_station_task_v2(
                apply.as_ref(),
                prepared.0,
                prepared.1,
                prepared.2,
            )
            .await
            .map(|_| StationCollectorTaskOutcome {
                refresh_remote_keys,
            })
            .map_err(|error| error.to_string())
        })
    }
}

pub(crate) trait StationCollectorRunnerPort: Send + Sync + 'static {
    fn due_station_collections(
        &self,
        limit: u32,
    ) -> BoxFuture<'static, Result<Vec<ScheduledStationCollection>, String>>;

    fn collect_task(
        &self,
        station_id: String,
        task: CollectorTask,
        context: StationCollectorTaskContext,
    ) -> BoxFuture<'static, Result<StationCollectorTaskOutcome, String>>;

    fn refresh_remote_keys(
        &self,
        station_id: String,
        context: StationCollectorTaskContext,
    ) -> BoxFuture<'static, Result<(), String>>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScheduledStationCollection {
    station_id: String,
    tasks: Vec<CollectorTask>,
}

pub(crate) struct V2StationCollectorRunnerAdapter {
    collectors: Arc<CollectorService>,
    settings: Arc<SettingsService>,
    tasks: Arc<dyn StationCollectorTaskPort>,
    remote_keys: Arc<dyn StationCollectorRemoteKeyRefreshPort>,
}

impl V2StationCollectorRunnerAdapter {
    pub(crate) fn new(
        collectors: Arc<CollectorService>,
        settings: Arc<SettingsService>,
        tasks: Arc<dyn StationCollectorTaskPort>,
        remote_keys: Arc<dyn StationCollectorRemoteKeyRefreshPort>,
    ) -> Self {
        Self {
            collectors,
            settings,
            tasks,
            remote_keys,
        }
    }
}

impl StationCollectorTaskContext {
    fn for_scheduled_task(context: &TaskRunContext, correlation_id: String) -> Self {
        Self {
            task_id: context.task_id.clone(),
            run_id: context.run_id.0,
            correlation_id,
            cancellation_token: context.cancellation_token.clone(),
        }
    }
}

impl StationCollectorRunnerPort for V2StationCollectorRunnerAdapter {
    fn due_station_collections(
        &self,
        limit: u32,
    ) -> BoxFuture<'static, Result<Vec<ScheduledStationCollection>, String>> {
        let collectors = self.collectors.clone();
        let settings = self.settings.clone();
        Box::pin(async move {
            let limit = PageLimit::new(limit).map_err(|error| error.to_string())?;
            let settings = settings.load().await.map_err(|error| error.to_string())?;
            let balance_stations = collectors
                .due_stations_for_task("balance", settings.balance_interval_minutes, limit)
                .await
                .map_err(|error| error.to_string())?;
            let group_stations = collectors
                .due_stations_for_task("groups", settings.group_rate_interval_minutes, limit)
                .await
                .map_err(|error| error.to_string())?;
            Ok(merge_due_station_collections(
                balance_stations.into_iter().map(|station| station.id),
                group_stations.into_iter().map(|station| station.id),
                limit.get() as usize,
            ))
        })
    }

    fn collect_task(
        &self,
        station_id: String,
        task: CollectorTask,
        context: StationCollectorTaskContext,
    ) -> BoxFuture<'static, Result<StationCollectorTaskOutcome, String>> {
        self.tasks.collect_task(station_id, task, context)
    }

    fn refresh_remote_keys(
        &self,
        station_id: String,
        context: StationCollectorTaskContext,
    ) -> BoxFuture<'static, Result<(), String>> {
        self.remote_keys.refresh_remote_keys(
            station_id,
            context.cancellation_token,
            Some(context.correlation_id),
        )
    }
}

pub struct StationCollectorRunnerState {
    supervisor: TaskSupervisor,
    task_id: TaskId,
}

impl StationCollectorRunnerState {
    pub fn stop(&self) {
        let _ = self.supervisor.cancel(&self.task_id);
    }

    pub async fn stop_and_join(&self, timeout: Duration) -> Result<(), String> {
        match self.supervisor.cancel(&self.task_id) {
            Ok(()) => {}
            Err(TaskSupervisorError::NotRunning(_)) => return Ok(()),
            Err(error) => return Err(error.to_string()),
        }
        tokio::time::timeout(timeout, self.supervisor.join_finished(&self.task_id))
            .await
            .map_err(|_| "Station collector runner shutdown timed out".to_string())?
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn start_v2(
        supervisor: TaskSupervisor,
        port: Arc<dyn StationCollectorRunnerPort>,
        coordinator: StationCollectionCoordinator,
    ) -> Result<Self, String> {
        let task_id = TaskId::from(RUNNER_TASK_ID);
        let runner_port = Arc::clone(&port);
        supervisor
            .register(
                TaskSpec::new(task_id.clone(), RUNNER_TASK_KIND, move |context| {
                    let port = Arc::clone(&runner_port);
                    let coordinator = coordinator.clone();
                    Box::pin(runner_loop_v2(port, coordinator, context))
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
    coordinator: StationCollectionCoordinator,
    context: TaskRunContext,
) -> Result<(), TaskFailure> {
    let mut interval = tokio::time::interval(COLLECTOR_BACKGROUND_INTERVAL);
    loop {
        tokio::select! {
            _ = context.cancellation_token.cancelled() => {
                crate::observability::runtime::bootstrap::emit(runtime_events::cancelled());
                return Err(TaskFailure::cancelled());
            }
            _ = interval.tick() => {
                run_due_station_collections_once_v2(port.as_ref(), &coordinator, &context).await;
            }
        }
    }
}

async fn run_due_station_collections_once_v2(
    port: &dyn StationCollectorRunnerPort,
    coordinator: &StationCollectionCoordinator,
    context: &TaskRunContext,
) {
    match port.due_station_collections(256).await {
        Ok(collections) => {
            let max_concurrency = coordinator.max_concurrency().get();
            stream::iter(collections)
                .for_each_concurrent(Some(max_concurrency), |collection| async move {
                    match run_station_collection_guarded_v2(port, coordinator, &collection, context)
                        .await
                    {
                        Ok(ScheduledStationCollectionOutcome::Completed)
                        | Ok(ScheduledStationCollectionOutcome::SkippedAlreadyRunning)
                        | Ok(ScheduledStationCollectionOutcome::Cancelled) => {}
                        Err(_error) => {
                            crate::observability::runtime::bootstrap::emit(runtime_events::failed())
                        }
                    }
                })
                .await;
        }
        Err(_error) => {
            crate::observability::runtime::bootstrap::emit(runtime_events::query_failed())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScheduledStationCollectionOutcome {
    Completed,
    SkippedAlreadyRunning,
    Cancelled,
}

async fn run_station_collection_guarded_v2(
    port: &dyn StationCollectorRunnerPort,
    coordinator: &StationCollectionCoordinator,
    collection: &ScheduledStationCollection,
    context: &TaskRunContext,
) -> Result<ScheduledStationCollectionOutcome, String> {
    let _lease = match coordinator
        .acquire(&collection.station_id, &context.cancellation_token)
        .await
    {
        Ok(lease) => lease,
        Err(StationCollectionAdmissionError::AlreadyRunning) => {
            return Ok(ScheduledStationCollectionOutcome::SkippedAlreadyRunning);
        }
        Err(StationCollectionAdmissionError::Cancelled) => {
            return Ok(ScheduledStationCollectionOutcome::Cancelled);
        }
        Err(StationCollectionAdmissionError::InvalidStationId) => {
            return Err("station collection invariant violation".to_string());
        }
        Err(StationCollectionAdmissionError::AtCapacity) => {
            return Err("station collection coordinator invariant violation".to_string());
        }
    };
    if context.cancellation_token.is_cancelled() {
        return Ok(ScheduledStationCollectionOutcome::Cancelled);
    }

    let mut failures = Vec::new();
    for task in &collection.tasks {
        if context.cancellation_token.is_cancelled() {
            return Ok(ScheduledStationCollectionOutcome::Cancelled);
        }
        let task_correlation_id = correlation::CorrelationId::new();
        let task_context = StationCollectorTaskContext::for_scheduled_task(
            context,
            task_correlation_id.as_str().to_string(),
        );
        let result: Result<(), String> =
            correlation::in_scope("station.collector.task", task_correlation_id, async {
                let outcome = port
                    .collect_task(collection.station_id.clone(), *task, task_context.clone())
                    .await?;
                if outcome.refresh_remote_keys {
                    if task_context.cancellation_token.is_cancelled() {
                        return Ok(());
                    }
                    port.refresh_remote_keys(collection.station_id.clone(), task_context)
                        .await
                        .map_err(|error| format!("remote key refresh failed: {error}"))?;
                }
                Ok(())
            })
            .await;
        if context.cancellation_token.is_cancelled() {
            return Ok(ScheduledStationCollectionOutcome::Cancelled);
        }
        if let Err(error) = result {
            if context.cancellation_token.is_cancelled() {
                return Ok(ScheduledStationCollectionOutcome::Cancelled);
            }
            failures.push(format!("{} collection failed: {error}", task.as_str()));
        }
    }
    if failures.is_empty() {
        Ok(ScheduledStationCollectionOutcome::Completed)
    } else {
        Err(failures.join("; "))
    }
}

impl Drop for StationCollectorRunnerState {
    fn drop(&mut self) {
        let _ = self.supervisor.cancel(&self.task_id);
    }
}

fn merge_due_station_collections(
    balance_station_ids: impl IntoIterator<Item = String>,
    group_station_ids: impl IntoIterator<Item = String>,
    limit: usize,
) -> Vec<ScheduledStationCollection> {
    let mut collections = Vec::<ScheduledStationCollection>::new();
    for (task, station_ids) in [
        (
            CollectorTask::Balance,
            balance_station_ids.into_iter().collect::<Vec<_>>(),
        ),
        (
            CollectorTask::Groups,
            group_station_ids.into_iter().collect::<Vec<_>>(),
        ),
    ] {
        for station_id in station_ids {
            if let Some(collection) = collections
                .iter_mut()
                .find(|collection| collection.station_id == station_id)
            {
                collection.tasks.push(task);
            } else if collections.len() < limit {
                collections.push(ScheduledStationCollection {
                    station_id,
                    tasks: vec![task],
                });
            }
        }
    }
    collections
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

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::*;
    use crate::background_tasks::{TaskRunId, TaskState};
    use crate::observability::runtime::bootstrap;
    use crate::observability::runtime::{RuntimeEvent, RuntimeLogReader, RuntimeLogService};
    use crate::services::station_collection_coordinator::StationCollectionCoordinator;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    use tokio::sync::{oneshot, Notify};
    use tokio_util::sync::CancellationToken;

    #[test]
    fn merges_due_balance_and_group_tasks_without_exceeding_station_limit() {
        assert_eq!(
            merge_due_station_collections(
                ["station-1".to_string(), "station-2".to_string()],
                ["station-1".to_string(), "station-3".to_string()],
                2,
            ),
            vec![
                ScheduledStationCollection {
                    station_id: "station-1".to_string(),
                    tasks: vec![CollectorTask::Balance, CollectorTask::Groups],
                },
                ScheduledStationCollection {
                    station_id: "station-2".to_string(),
                    tasks: vec![CollectorTask::Balance],
                },
            ]
        );
    }

    #[tokio::test]
    async fn guarded_collection_runs_balance_then_groups_for_due_station() {
        let port = RecordingRunnerPort::new(vec![Ok(task_outcome(false)), Ok(task_outcome(true))]);
        let context = test_run_context();
        let coordinator = coordinator(1);

        run_station_collection_guarded_v2(
            &port,
            &coordinator,
            &scheduled_collection(
                "station-1",
                &[CollectorTask::Balance, CollectorTask::Groups],
            ),
            &context,
        )
        .await
        .expect("guarded run succeeds");

        assert_eq!(
            port.calls(),
            vec![
                ("station-1".to_string(), CollectorTask::Balance),
                ("station-1".to_string(), CollectorTask::Groups),
            ]
        );
        let correlation_ids = port.correlation_ids();
        assert_eq!(correlation_ids.len(), 2);
        assert_ne!(correlation_ids[0], correlation_ids[1]);
        assert!(correlation_ids
            .iter()
            .all(|correlation_id| correlation_id != "test-correlation"));
        assert_eq!(
            port.remote_key_refreshes(),
            vec![("station-1".to_string(), correlation_ids[1].clone())]
        );
    }

    #[tokio::test]
    async fn guarded_collection_keeps_group_side_effect_after_balance_failure() {
        let port = RecordingRunnerPort::new(vec![
            Err("balance failed".to_string()),
            Ok(task_outcome(true)),
        ]);
        let context = test_run_context();
        let coordinator = coordinator(1);

        let result = run_station_collection_guarded_v2(
            &port,
            &coordinator,
            &scheduled_collection(
                "station-2",
                &[CollectorTask::Balance, CollectorTask::Groups],
            ),
            &context,
        )
        .await;

        assert_eq!(
            result,
            Err("balance collection failed: balance failed".to_string())
        );
        assert_eq!(
            port.calls(),
            vec![
                ("station-2".to_string(), CollectorTask::Balance),
                ("station-2".to_string(), CollectorTask::Groups),
            ]
        );
        assert_eq!(port.remote_key_refreshes().len(), 1);
    }

    #[tokio::test]
    async fn guarded_collection_does_not_refresh_remote_keys_after_group_failure() {
        let port = RecordingRunnerPort::new(vec![Err("groups failed".to_string())]);
        let coordinator = coordinator(1);

        let result = run_station_collection_guarded_v2(
            &port,
            &coordinator,
            &scheduled_collection("station-3", &[CollectorTask::Groups]),
            &test_run_context(),
        )
        .await;

        assert_eq!(
            result,
            Err("groups collection failed: groups failed".to_string())
        );
        assert!(port.remote_key_refreshes().is_empty());
    }

    #[tokio::test]
    async fn guarded_collection_reports_remote_key_refresh_failure() {
        let port = RecordingRunnerPort::with_refresh_results(
            vec![Ok(task_outcome(true))],
            vec![Err("scan unavailable".to_string())],
        );
        let coordinator = coordinator(1);

        let result = run_station_collection_guarded_v2(
            &port,
            &coordinator,
            &scheduled_collection("station-4", &[CollectorTask::Groups]),
            &test_run_context(),
        )
        .await;

        assert_eq!(
            result,
            Err(
                "groups collection failed: remote key refresh failed: scan unavailable".to_string()
            )
        );
        assert_eq!(port.remote_key_refreshes().len(), 1);
    }

    #[tokio::test]
    async fn guarded_collection_skips_same_station_until_lease_drops() {
        let notify_started = Arc::new(Notify::new());
        let (release_sender, release_receiver) = oneshot::channel();
        let port = BlockingFirstTaskRunnerPort::new(
            Arc::clone(&notify_started),
            Mutex::new(Some(release_receiver)),
        );
        let coordinator = coordinator(1);
        let running_coordinator = coordinator.clone();
        let running = tokio::spawn(async move {
            let context = test_run_context();
            run_station_collection_guarded_v2(
                &port,
                &running_coordinator,
                &scheduled_collection("station-guarded", &[CollectorTask::Balance]),
                &context,
            )
            .await
        });
        notify_started.notified().await;

        let duplicate =
            RecordingRunnerPort::new(vec![Ok(task_outcome(false)), Ok(task_outcome(false))]);
        let duplicate_context = test_run_context();
        let duplicate_result = run_station_collection_guarded_v2(
            &duplicate,
            &coordinator,
            &scheduled_collection("station-guarded", &[CollectorTask::Balance]),
            &duplicate_context,
        )
        .await;
        assert_eq!(
            duplicate_result,
            Ok(ScheduledStationCollectionOutcome::SkippedAlreadyRunning)
        );
        assert!(duplicate.calls().is_empty());

        release_sender.send(()).expect("release first run");
        running
            .await
            .expect("first run joins")
            .expect("first run succeeds");

        let after_release =
            RecordingRunnerPort::new(vec![Ok(task_outcome(false)), Ok(task_outcome(false))]);
        let after_release_context = test_run_context();
        run_station_collection_guarded_v2(
            &after_release,
            &coordinator,
            &scheduled_collection("station-guarded", &[CollectorTask::Balance]),
            &after_release_context,
        )
        .await
        .expect("lease is released after first run");
        assert_eq!(after_release.calls().len(), 1);
    }

    #[tokio::test]
    async fn due_batch_runs_different_stations_up_to_global_limit() {
        let port = ConcurrentRunnerPort::with_due_collections(vec![
            scheduled_collection("station-a", &[CollectorTask::Balance]),
            scheduled_collection("station-b", &[CollectorTask::Balance]),
            scheduled_collection("station-c", &[CollectorTask::Balance]),
        ]);
        let coordinator = coordinator(2);
        let context = test_run_context();
        let port_for_run = port.clone();
        let coordinator_for_run = coordinator.clone();
        let running = tokio::spawn(async move {
            run_due_station_collections_once_v2(&port_for_run, &coordinator_for_run, &context)
                .await;
        });

        port.wait_until_started(2).await;
        assert_eq!(port.peak_active(), 2);
        assert_eq!(port.started(), 2);
        port.release();
        running.await.expect("batch joins");
        assert_eq!(port.completed(), 3);
        assert_eq!(port.peak_active(), 2);
    }

    #[tokio::test]
    async fn due_batch_skips_preheld_station_without_blocking_other_station() {
        let port = DueRecordingRunnerPort::new(
            vec![
                scheduled_collection("station-a", &[CollectorTask::Balance]),
                scheduled_collection("station-b", &[CollectorTask::Balance]),
            ],
            vec![Ok(task_outcome(false))],
        );
        let coordinator = coordinator(2);
        let _held = coordinator.try_acquire("station-a").expect("a is held");
        let context = test_run_context();

        run_due_station_collections_once_v2(&port, &coordinator, &context).await;

        assert_eq!(
            port.calls(),
            vec![("station-b".to_string(), CollectorTask::Balance)]
        );
    }

    #[tokio::test]
    async fn due_batch_isolates_one_station_failure_from_other_stations() {
        let port = FailureIsolatingRunnerPort::new(vec![
            scheduled_collection("station-a", &[CollectorTask::Balance]),
            scheduled_collection("station-b", &[CollectorTask::Balance]),
            scheduled_collection("station-c", &[CollectorTask::Balance]),
        ]);
        let coordinator = coordinator(3);

        run_due_station_collections_once_v2(&port, &coordinator, &test_run_context()).await;

        assert_eq!(
            port.completed_station_ids(),
            vec!["station-a", "station-b", "station-c"]
        );
    }

    #[tokio::test]
    async fn provider_fault_from_real_collector_runner_publishes_final_jsonl_event() {
        let root = tempfile::tempdir().expect("runtime root");
        let service = Arc::new(RuntimeLogService::open(root.path()));
        let port = FailureIsolatingRunnerPort::new(vec![scheduled_collection(
            "station-a",
            &[CollectorTask::Balance],
        )]);
        let coordinator = coordinator(1);

        bootstrap::with_test_service(Arc::clone(&service), || async {
            run_due_station_collections_once_v2(&port, &coordinator, &test_run_context()).await;
        })
        .await;
        service.flush();

        let page = RuntimeLogReader::new(root.path()).read_page(0, 200, 1024 * 1024);
        assert!(page.lines.iter().any(|line| {
            serde_json::from_slice::<RuntimeEvent>(line.as_bytes())
                .ok()
                .is_some_and(|event| event.event_code.as_str() == "collector.station.failed")
        }));
    }

    #[tokio::test]
    async fn due_batch_cancellation_stops_capacity_waiter_before_provider_call() {
        let port = DueRecordingRunnerPort::new(
            vec![scheduled_collection("station-b", &[CollectorTask::Balance])],
            vec![Ok(task_outcome(false))],
        );
        let coordinator = coordinator(1);
        let held = coordinator.try_acquire("station-a").expect("a is held");
        let context = test_run_context();
        let cancellation = context.cancellation_token.clone();
        let running_coordinator = coordinator.clone();

        let running = tokio::spawn(async move {
            run_due_station_collections_once_v2(&port, &running_coordinator, &context).await;
            port.calls()
        });
        tokio::task::yield_now().await;
        cancellation.cancel();

        let calls = tokio::time::timeout(Duration::from_secs(1), running)
            .await
            .expect("cancelled batch joins")
            .expect("batch task joins");
        assert!(calls.is_empty());
        assert_eq!(coordinator.snapshot().active, 1);
        drop(held);
        assert_eq!(coordinator.snapshot().active, 0);
    }

    #[tokio::test]
    async fn stop_and_join_cancels_running_station_and_releases_lease() {
        let supervisor = TaskSupervisor::new();
        let coordinator = coordinator(1);
        let port = Arc::new(CancellationAwareRunnerPort::new());
        let runner = StationCollectorRunnerState::start_v2(
            supervisor.clone(),
            Arc::clone(&port) as Arc<dyn StationCollectorRunnerPort>,
            coordinator.clone(),
        )
        .expect("runner starts");

        port.wait_until_started().await;
        assert_eq!(coordinator.snapshot().active, 1);
        runner
            .stop_and_join(Duration::from_secs(1))
            .await
            .expect("cancellation-aware collector joins");

        assert_eq!(coordinator.snapshot().active, 0);
        assert_eq!(
            supervisor
                .status(&TaskId::from(RUNNER_TASK_ID))
                .expect("runner status")
                .state,
            TaskState::Cancelled
        );
    }

    fn scheduled_collection(
        station_id: &str,
        tasks: &[CollectorTask],
    ) -> ScheduledStationCollection {
        ScheduledStationCollection {
            station_id: station_id.to_string(),
            tasks: tasks.to_vec(),
        }
    }

    fn coordinator(limit: usize) -> StationCollectionCoordinator {
        StationCollectionCoordinator::new(NonZeroUsize::new(limit).expect("non-zero limit"))
    }

    fn task_outcome(refresh_remote_keys: bool) -> StationCollectorTaskOutcome {
        StationCollectorTaskOutcome {
            refresh_remote_keys,
        }
    }

    struct RecordingRunnerPort {
        calls: Arc<Mutex<Vec<(String, CollectorTask)>>>,
        correlation_ids: Arc<Mutex<Vec<String>>>,
        results: Arc<Mutex<Vec<Result<StationCollectorTaskOutcome, String>>>>,
        remote_key_refreshes: Arc<Mutex<Vec<(String, String)>>>,
        refresh_results: Arc<Mutex<Vec<Result<(), String>>>>,
    }

    impl RecordingRunnerPort {
        fn new(results: Vec<Result<StationCollectorTaskOutcome, String>>) -> Self {
            Self::with_refresh_results(results, Vec::new())
        }

        fn with_refresh_results(
            results: Vec<Result<StationCollectorTaskOutcome, String>>,
            refresh_results: Vec<Result<(), String>>,
        ) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                correlation_ids: Arc::new(Mutex::new(Vec::new())),
                results: Arc::new(Mutex::new(results)),
                remote_key_refreshes: Arc::new(Mutex::new(Vec::new())),
                refresh_results: Arc::new(Mutex::new(refresh_results)),
            }
        }

        fn calls(&self) -> Vec<(String, CollectorTask)> {
            self.calls.lock().expect("calls").clone()
        }

        fn correlation_ids(&self) -> Vec<String> {
            self.correlation_ids
                .lock()
                .expect("correlation ids")
                .clone()
        }

        fn remote_key_refreshes(&self) -> Vec<(String, String)> {
            self.remote_key_refreshes
                .lock()
                .expect("remote key refreshes")
                .clone()
        }
    }

    impl StationCollectorRunnerPort for RecordingRunnerPort {
        fn due_station_collections(
            &self,
            _limit: u32,
        ) -> BoxFuture<'static, Result<Vec<ScheduledStationCollection>, String>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn collect_task(
            &self,
            station_id: String,
            task: CollectorTask,
            context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<StationCollectorTaskOutcome, String>> {
            self.calls.lock().expect("calls").push((station_id, task));
            self.correlation_ids
                .lock()
                .expect("correlation ids")
                .push(context.correlation_id);
            let result = self.results.lock().expect("results").remove(0);
            Box::pin(async move { result })
        }

        fn refresh_remote_keys(
            &self,
            station_id: String,
            context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<(), String>> {
            self.remote_key_refreshes
                .lock()
                .expect("remote key refreshes")
                .push((station_id, context.correlation_id));
            let result = {
                let mut results = self.refresh_results.lock().expect("refresh results");
                if results.is_empty() {
                    Ok(())
                } else {
                    results.remove(0)
                }
            };
            Box::pin(async move { result })
        }
    }

    struct BlockingFirstTaskRunnerPort {
        notify_started: Arc<Notify>,
        release_receiver: Mutex<Option<oneshot::Receiver<()>>>,
        calls: AtomicUsize,
    }

    struct DueRecordingRunnerPort {
        due_collections: Vec<ScheduledStationCollection>,
        inner: RecordingRunnerPort,
    }

    struct FailureIsolatingRunnerPort {
        due_collections: Vec<ScheduledStationCollection>,
        completed_station_ids: Arc<Mutex<Vec<String>>>,
    }

    impl FailureIsolatingRunnerPort {
        fn new(due_collections: Vec<ScheduledStationCollection>) -> Self {
            Self {
                due_collections,
                completed_station_ids: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn completed_station_ids(&self) -> Vec<String> {
            let mut station_ids = self
                .completed_station_ids
                .lock()
                .expect("completed station IDs")
                .clone();
            station_ids.sort();
            station_ids
        }
    }

    impl StationCollectorRunnerPort for FailureIsolatingRunnerPort {
        fn due_station_collections(
            &self,
            _limit: u32,
        ) -> BoxFuture<'static, Result<Vec<ScheduledStationCollection>, String>> {
            let due_collections = self.due_collections.clone();
            Box::pin(async move { Ok(due_collections) })
        }

        fn collect_task(
            &self,
            station_id: String,
            _task: CollectorTask,
            _context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<StationCollectorTaskOutcome, String>> {
            self.completed_station_ids
                .lock()
                .expect("completed station IDs")
                .push(station_id.clone());
            Box::pin(async move {
                if station_id == "station-a" {
                    Err("station-a collection failed".to_string())
                } else {
                    Ok(task_outcome(false))
                }
            })
        }

        fn refresh_remote_keys(
            &self,
            _station_id: String,
            _context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[derive(Clone)]
    struct CancellationAwareRunnerPort {
        started: tokio::sync::watch::Sender<bool>,
    }

    impl CancellationAwareRunnerPort {
        fn new() -> Self {
            let (started, _) = tokio::sync::watch::channel(false);
            Self { started }
        }

        async fn wait_until_started(&self) {
            let mut started = self.started.subscribe();
            while !*started.borrow() {
                started.changed().await.expect("started sender stays alive");
            }
        }
    }

    impl StationCollectorRunnerPort for CancellationAwareRunnerPort {
        fn due_station_collections(
            &self,
            _limit: u32,
        ) -> BoxFuture<'static, Result<Vec<ScheduledStationCollection>, String>> {
            Box::pin(async {
                Ok(vec![scheduled_collection(
                    "station-cancellation-aware",
                    &[CollectorTask::Balance],
                )])
            })
        }

        fn collect_task(
            &self,
            _station_id: String,
            _task: CollectorTask,
            context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<StationCollectorTaskOutcome, String>> {
            let started = self.started.clone();
            Box::pin(async move {
                started.send_replace(true);
                context.cancellation_token.cancelled().await;
                Err("collector cancelled".to_string())
            })
        }

        fn refresh_remote_keys(
            &self,
            _station_id: String,
            _context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
    }

    impl DueRecordingRunnerPort {
        fn new(
            due_collections: Vec<ScheduledStationCollection>,
            results: Vec<Result<StationCollectorTaskOutcome, String>>,
        ) -> Self {
            Self {
                due_collections,
                inner: RecordingRunnerPort::new(results),
            }
        }

        fn calls(&self) -> Vec<(String, CollectorTask)> {
            self.inner.calls()
        }
    }

    impl StationCollectorRunnerPort for DueRecordingRunnerPort {
        fn due_station_collections(
            &self,
            _limit: u32,
        ) -> BoxFuture<'static, Result<Vec<ScheduledStationCollection>, String>> {
            let due_collections = self.due_collections.clone();
            Box::pin(async move { Ok(due_collections) })
        }

        fn collect_task(
            &self,
            station_id: String,
            task: CollectorTask,
            context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<StationCollectorTaskOutcome, String>> {
            self.inner.collect_task(station_id, task, context)
        }

        fn refresh_remote_keys(
            &self,
            station_id: String,
            context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<(), String>> {
            self.inner.refresh_remote_keys(station_id, context)
        }
    }

    #[derive(Clone)]
    struct ConcurrentRunnerPort {
        state: Arc<ConcurrentRunnerState>,
    }

    struct ConcurrentRunnerState {
        due_collections: Vec<ScheduledStationCollection>,
        active: AtomicUsize,
        peak: AtomicUsize,
        started: AtomicUsize,
        completed: AtomicUsize,
        started_watch: tokio::sync::watch::Sender<usize>,
        release: tokio::sync::watch::Sender<bool>,
    }

    impl ConcurrentRunnerPort {
        fn with_due_collections(due_collections: Vec<ScheduledStationCollection>) -> Self {
            let (release, _) = tokio::sync::watch::channel(false);
            let (started_watch, _) = tokio::sync::watch::channel(0usize);
            Self {
                state: Arc::new(ConcurrentRunnerState {
                    due_collections,
                    active: AtomicUsize::new(0),
                    peak: AtomicUsize::new(0),
                    started: AtomicUsize::new(0),
                    completed: AtomicUsize::new(0),
                    started_watch,
                    release,
                }),
            }
        }

        async fn wait_until_started(&self, expected: usize) {
            let mut started = self.state.started_watch.subscribe();
            while *started.borrow() < expected {
                started.changed().await.expect("started sender stays alive");
            }
        }

        fn started(&self) -> usize {
            self.state.started.load(Ordering::SeqCst)
        }

        fn completed(&self) -> usize {
            self.state.completed.load(Ordering::SeqCst)
        }

        fn peak_active(&self) -> usize {
            self.state.peak.load(Ordering::SeqCst)
        }

        fn release(&self) {
            self.state.release.send_replace(true);
        }
    }

    impl StationCollectorRunnerPort for ConcurrentRunnerPort {
        fn due_station_collections(
            &self,
            _limit: u32,
        ) -> BoxFuture<'static, Result<Vec<ScheduledStationCollection>, String>> {
            let due_collections = self.state.due_collections.clone();
            Box::pin(async move { Ok(due_collections) })
        }

        fn collect_task(
            &self,
            _station_id: String,
            _task: CollectorTask,
            _context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<StationCollectorTaskOutcome, String>> {
            let state = Arc::clone(&self.state);
            let mut release = state.release.subscribe();
            Box::pin(async move {
                let active = state.active.fetch_add(1, Ordering::SeqCst) + 1;
                state.peak.fetch_max(active, Ordering::SeqCst);
                let started = state.started.fetch_add(1, Ordering::SeqCst) + 1;
                state.started_watch.send_replace(started);
                while !*release.borrow() {
                    release
                        .changed()
                        .await
                        .map_err(|_| "release dropped".to_string())?;
                }
                state.active.fetch_sub(1, Ordering::SeqCst);
                state.completed.fetch_add(1, Ordering::SeqCst);
                Ok(task_outcome(false))
            })
        }

        fn refresh_remote_keys(
            &self,
            _station_id: String,
            _context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
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
        fn due_station_collections(
            &self,
            _limit: u32,
        ) -> BoxFuture<'static, Result<Vec<ScheduledStationCollection>, String>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn collect_task(
            &self,
            _station_id: String,
            _task: CollectorTask,
            _context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<StationCollectorTaskOutcome, String>> {
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
                        .map(|_| task_outcome(false))
                        .map_err(|_| "release dropped".to_string())
                })
            } else {
                Box::pin(async { Ok(task_outcome(false)) })
            }
        }

        fn refresh_remote_keys(
            &self,
            _station_id: String,
            _context: StationCollectorTaskContext,
        ) -> BoxFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn start_v2_registers_runner_with_supervisor_and_stop_cancels_task() {
        let supervisor = TaskSupervisor::new();
        let task_id = TaskId::from(RUNNER_TASK_ID);
        let port = Arc::new(RecordingRunnerPort::new(Vec::new()));

        let runner =
            StationCollectorRunnerState::start_v2(supervisor.clone(), port, coordinator(1))
                .expect("runner starts");

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
