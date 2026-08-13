import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const [
  libSource,
  servicesModSource,
  coordinatorSource,
  stationCollectorSource,
  collectionFacadeSource,
  settingsFacadeSource,
  appCompositionSource,
  stationCatalogSource,
  sub2apiDriverSource,
  newapiDriverSource,
] = await Promise.all([
  readFile("src-tauri/src/lib.rs", "utf8"),
  readFile("src-tauri/src/services/mod.rs", "utf8"),
  readFile("src-tauri/src/services/station_collection_coordinator.rs", "utf8"),
  readFile("src-tauri/src/services/station_collectors.rs", "utf8"),
  readFile("src-tauri/src/application/command_facades/station_collection.rs", "utf8"),
  readFile("src-tauri/src/application/command_facades/settings_stations.rs", "utf8"),
  readFile("src-tauri/src/app_composition.rs", "utf8"),
  readFile("src-tauri/src/persistence/stores/station_catalog.rs", "utf8"),
  readFile("src-tauri/src/services/collectors/drivers/sub2api/mod.rs", "utf8"),
  readFile("src-tauri/src/services/collectors/drivers/newapi/mod.rs", "utf8"),
]);

const stationCollectorProductionSource = stationCollectorSource.split("#[cfg(test)]")[0];

assert.ok(
  servicesModSource.includes("pub mod station_collectors;") &&
    servicesModSource.includes("mod station_collection_coordinator;"),
  "services should expose the runner and crate-private collection coordinator",
);
assert.ok(
  coordinatorSource.includes("pub(crate) struct StationCollectionCoordinator {") &&
    coordinatorSource.includes("pub(crate) async fn acquire") &&
    coordinatorSource.includes("pub(crate) fn try_acquire"),
  "the coordinator should own both waiting background admission and immediate manual admission",
);
assert.ok(
  !stationCollectorProductionSource.includes("ACTIVE_STATION_RUNS") &&
    !stationCollectorProductionSource.includes("StationCollectorRunGuard"),
  "the runner must not retain a separate static station-run guard",
);
assert.ok(
  stationCollectorProductionSource.includes("StationCollectionCoordinator"),
  "the runner should use the shared coordinator for station admission",
);
assert.ok(
  collectionFacadeSource.includes("run_with_station_collection_lease") &&
    collectionFacadeSource.includes("StationCollectionCommandError::Admission"),
  "manual collection and saved-station login should share explicit admission",
);
assert.ok(
  settingsFacadeSource.includes("persist_and_apply_collection_runtime_settings") &&
    settingsFacadeSource.includes("set_max_concurrency"),
  "persisted collection concurrency should update the shared runtime coordinator",
);
assert.ok(
  libSource.includes("let station_collection_coordinator =") &&
    libSource.includes("StationCollectionCoordinator::new") &&
    libSource.includes("StationCollectorRunnerState::start_v2"),
  "app setup should construct one coordinator and start the runner",
);
assert.ok(
  !appCompositionSource.includes("StationCollectionCoordinator::new") &&
    !stationCollectorProductionSource.includes("StationCollectionCoordinator::new"),
  "composition and runner must receive the startup coordinator rather than construct another one",
);

assert.ok(
  stationCatalogSource.includes("pub(crate) async fn due_collector_task") &&
    stationCatalogSource.includes("collector_task_state.updated_at") &&
    stationCatalogSource.includes("(?2 * 60000) <= ?3"),
  "due query should keep each task interval and persisted task state semantics",
);
assert.ok(
  stationCollectorSource.includes("CollectorTask::Balance") &&
    stationCollectorSource.includes("CollectorTask::Groups"),
  "scheduled collection should retain balance and groups tasks",
);
assert.ok(
  sub2apiDriverSource.includes("context.budget") &&
    newapiDriverSource.includes("context.budget"),
  "provider drivers should keep using the collection request budget",
);
