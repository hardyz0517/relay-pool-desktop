use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::services::{
    collectors::{self, adapters::CollectorTask},
    database::AppDatabase,
};

const RUNNER_POLL_INTERVAL: Duration = Duration::from_secs(30);
const RUNNER_STOP_SLICE: Duration = Duration::from_millis(250);
static ACTIVE_STATION_RUNS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub struct StationCollectorRunnerState {
    stop_requested: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl StationCollectorRunnerState {
    pub fn start(database: AppDatabase, data_key: [u8; 32]) -> Self {
        let stop_requested = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop_requested);
        let handle = thread::spawn(move || runner_loop(database, data_key, thread_stop));
        Self {
            stop_requested,
            handle: Mutex::new(Some(handle)),
        }
    }

    #[allow(dead_code)]
    pub fn stop(&self) {
        self.stop_requested.store(true, Ordering::Relaxed);
        if let Ok(mut handle) = self.handle.lock() {
            if let Some(handle) = handle.take() {
                let _ = handle.join();
            }
        }
    }
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

fn runner_loop(database: AppDatabase, data_key: [u8; 32], stop_requested: Arc<AtomicBool>) {
    while !stop_requested.load(Ordering::Relaxed) {
        let now = crate::services::database::now_millis_for_services().to_string();
        match database.due_station_collectors(&now) {
            Ok(stations) => {
                for station in stations {
                    if stop_requested.load(Ordering::Relaxed) {
                        break;
                    }
                    let station_id = station.id.clone();
                    if let Err(error) =
                        run_station_collection_guarded(&database, &data_key, &station_id)
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

fn run_station_collection_guarded(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
) -> Result<(), String> {
    let _guard = StationCollectorRunGuard::try_start(station_id)?;
    let balance_result = collectors::collect_station_task(
        database,
        data_key,
        station_id.to_string(),
        CollectorTask::Balance,
    );
    let groups_result = collectors::collect_station_task(
        database,
        data_key,
        station_id.to_string(),
        CollectorTask::Groups,
    );
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
