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
