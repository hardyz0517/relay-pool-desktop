use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use futures_util::future::BoxFuture;

use crate::{
    application::{app_services::AppServices, collectors::CollectorService, pagination::PageLimit},
    services::collectors::{
        self,
        adapters::CollectorTask,
        apply::{CollectorApplyPort, V2CollectorApplyAdapter},
        CollectorSourcePort, V2CollectorSourceAdapter,
    },
};

const RUNNER_POLL_INTERVAL: Duration = Duration::from_secs(30);
const RUNNER_STOP_SLICE: Duration = Duration::from_millis(250);
static ACTIVE_STATION_RUNS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(crate) fn v2_runner_port(
    services: &AppServices,
    data_key: [u8; 32],
) -> Arc<dyn StationCollectorRunnerPort> {
    let source: Arc<dyn CollectorSourcePort> = Arc::new(V2CollectorSourceAdapter::new(
        services.collectors.clone(),
        services.credentials.clone(),
        services.settings.clone(),
    ));
    let apply: Arc<dyn CollectorApplyPort> =
        Arc::new(V2CollectorApplyAdapter::new((*services.collectors).clone()));
    let tasks: Arc<dyn StationCollectorTaskPort> =
        Arc::new(V2StationCollectorTaskAdapter::new(source, apply, data_key));
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
    ) -> BoxFuture<'static, Result<(), String>>;
}

pub(crate) struct V2StationCollectorTaskAdapter {
    source: Arc<dyn CollectorSourcePort>,
    apply: Arc<dyn CollectorApplyPort>,
    data_key: [u8; 32],
}

impl V2StationCollectorTaskAdapter {
    pub(crate) fn new(
        source: Arc<dyn CollectorSourcePort>,
        apply: Arc<dyn CollectorApplyPort>,
        data_key: [u8; 32],
    ) -> Self {
        Self {
            source,
            apply,
            data_key,
        }
    }
}

impl StationCollectorTaskPort for V2StationCollectorTaskAdapter {
    fn collect_task(
        &self,
        station_id: String,
        task: CollectorTask,
    ) -> BoxFuture<'static, Result<(), String>> {
        let source = self.source.clone();
        let apply = self.apply.clone();
        let data_key = self.data_key;
        Box::pin(async move {
            let prepared = tauri::async_runtime::spawn_blocking(move || {
                collectors::prepare_station_task_v2(source.as_ref(), &data_key, station_id, task)
            })
            .await
            .map_err(|error| format!("collector worker failed to join: {error}"))?
            .map_err(|error| error.to_string())?;
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
    ) -> BoxFuture<'static, Result<(), String>> {
        self.tasks.collect_task(station_id, task)
    }
}

pub struct StationCollectorRunnerState {
    stop_requested: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl StationCollectorRunnerState {
    #[allow(dead_code)]
    pub fn stop(&self) {
        self.stop_requested.store(true, Ordering::Relaxed);
        if let Ok(mut handle) = self.handle.lock() {
            if let Some(handle) = handle.take() {
                let _ = handle.join();
            }
        }
    }

    pub(crate) fn start_v2(port: Arc<dyn StationCollectorRunnerPort>) -> Self {
        let stop_requested = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop_requested);
        let handle = thread::spawn(move || {
            tauri::async_runtime::block_on(runner_loop_v2(port, thread_stop))
        });
        Self {
            stop_requested,
            handle: Mutex::new(Some(handle)),
        }
    }
}

async fn runner_loop_v2(
    port: Arc<dyn StationCollectorRunnerPort>,
    stop_requested: Arc<AtomicBool>,
) {
    while !stop_requested.load(Ordering::Relaxed) {
        match port.due_station_ids(256).await {
            Ok(station_ids) => {
                for station_id in station_ids {
                    if stop_requested.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Err(error) =
                        run_station_collection_guarded_v2(port.as_ref(), &station_id).await
                    {
                        eprintln!("Station collector runner failed for {station_id}: {error}");
                    }
                }
            }
            Err(error) => {
                eprintln!("Station collector runner could not query due stations: {error}")
            }
        }
        sleep_until_next_poll(&stop_requested);
    }
}

async fn run_station_collection_guarded_v2(
    port: &dyn StationCollectorRunnerPort,
    station_id: &str,
) -> Result<(), String> {
    let _guard = StationCollectorRunGuard::try_start(station_id)?;
    let balance_result = port
        .collect_task(station_id.to_string(), CollectorTask::Balance)
        .await;
    let groups_result = port
        .collect_task(station_id.to_string(), CollectorTask::Groups)
        .await;
    combine_collection_results(balance_result, groups_result)
}

impl Drop for StationCollectorRunnerState {
    fn drop(&mut self) {
        self.stop_requested.store(true, Ordering::Relaxed);
        if let Ok(mut handle) = self.handle.lock() {
            if let Some(handle) = handle.take() {
                let _ = handle.join();
            }
        }
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

fn sleep_until_next_poll(stop_requested: &AtomicBool) {
    let mut slept = Duration::ZERO;
    while slept < RUNNER_POLL_INTERVAL && !stop_requested.load(Ordering::Relaxed) {
        thread::sleep(RUNNER_STOP_SLICE);
        slept += RUNNER_STOP_SLICE;
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
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    use tokio::sync::{oneshot, Notify};

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

        run_station_collection_guarded_v2(&port, "station-1")
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

        let result = run_station_collection_guarded_v2(&port, "station-2").await;

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
            run_station_collection_guarded_v2(&port, "station-guarded").await
        });
        notify_started.notified().await;

        let duplicate = RecordingRunnerPort::new(vec![Ok(()), Ok(())]);
        let duplicate_result =
            run_station_collection_guarded_v2(&duplicate, "station-guarded").await;
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
        run_station_collection_guarded_v2(&after_release, "station-guarded")
            .await
            .expect("guard is released after first run");
        assert_eq!(after_release.calls().len(), 2);
    }

    #[test]
    fn stop_requested_sleep_returns_without_poll_interval_delay() {
        let stop_requested = AtomicBool::new(true);
        let started = std::time::Instant::now();

        sleep_until_next_poll(&stop_requested);

        assert!(started.elapsed() < RUNNER_STOP_SLICE);
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
}
