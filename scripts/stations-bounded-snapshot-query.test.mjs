import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const stationsPageSource = await readFile("src/features/stations/StationsPage.tsx", "utf8");
const querySource = await readFile("src/lib/query/resourceQueries.ts", "utf8");
const apiSource = await readFile("src/lib/api/collector.ts", "utf8");
const backendClientSource = await readFile("src/lib/bridge/BackendClient.ts", "utf8");
const desktopBackendSource = await readFile("src/lib/bridge/DesktopBackend.ts", "utf8");
const commandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const registrySource = await readFile("src-tauri/src/ipc/registry.rs", "utf8");
const mainWindowPermissionSource = await readFile("src-tauri/permissions/main-window.toml", "utf8");
const facadeSource = await readFile("src-tauri/src/application/command_facades/collector_metadata.rs", "utf8");
const serviceSource = await readFile("src-tauri/src/application/collectors.rs", "utf8");

assert.ok(
  stationsPageSource.includes("stationAssetsQueryOptions(stationIds)") &&
    stationsPageSource.includes("const assetSnapshotsByStation = useMemo(") &&
    !stationsPageSource.includes("useQueries") &&
    !stationsPageSource.includes("stationAssetQueryOptions(station.id)") &&
    !stationsPageSource.includes("usePageRefreshEnabled"),
  "StationsPage should use one activity-bound aggregate station asset query instead of per-row queries",
);

assert.ok(
  querySource.includes("export const stationAssetsQueryOptions = (stationIds: readonly string[]) =>") &&
    querySource.includes("queryKeys.stationAssetsForStations(stationIds)") &&
    querySource.includes("enabled: stationIds.length > 0") &&
    querySource.includes("listLatestCollectorSnapshots([...stationIds])") &&
    querySource.includes('"station asset snapshots"') &&
    querySource.includes("6_000"),
  "station asset aggregate query option should be keyed by the visible station id set and timeout bounded",
);

assert.ok(
  apiSource.includes("listLatestCollectorSnapshots(stationIds: string[])") &&
    apiSource.includes("getActiveBackendClient().collectors.listLatestCollectorSnapshots(stationIds)") &&
    backendClientSource.includes("listLatestCollectorSnapshots(stationIds: string[]): Promise<CollectorSnapshot[]>") &&
    desktopBackendSource.includes("listLatestCollectorSnapshotsBinding({ stationIds })"),
  "frontend collector API should route the aggregate read through the active backend client",
);

assert.ok(
  commandsSource.includes("list_latest_collector_snapshots") &&
    commandsSource.includes("CollectorStationIdsInputDto::parse(input)") &&
    registrySource.includes("list_latest_collector_snapshots => $crate::commands::list_latest_collector_snapshots") &&
    registrySource.includes('migrated_read("CollectorStationIdsInputDto", "Vec<CollectorSnapshotDto>")') &&
    mainWindowPermissionSource.includes('"list_latest_collector_snapshots"'),
  "IPC registry should expose list_latest_collector_snapshots as a migrated read with a stationIds DTO",
);

assert.ok(
  facadeSource.includes("list_latest_collector_snapshots(") &&
    facadeSource.includes("list_latest_station_snapshots(station_ids)") &&
    serviceSource.includes("pub(crate) async fn list_latest_station_snapshots(") &&
    serviceSource.includes("ROW_NUMBER() OVER") &&
    serviceSource.includes("PARTITION BY station_id") &&
    serviceSource.includes("WHERE station_id IN") &&
    serviceSource.includes("WHERE station_snapshot_rank = 1"),
  "backend aggregate read should use one bounded latest-snapshot query for the requested station ids",
);

assert.ok(
  !/for station_id in &station_ids[\s\S]{0,400}latest_station_snapshot/.test(serviceSource),
  "backend aggregate read must not hide N+1 by looping over latest_station_snapshot",
);

console.log("stations bounded snapshot query contract passed");
