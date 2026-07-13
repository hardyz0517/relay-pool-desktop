import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const stationsSource = await readFile("src/features/stations/StationsPage.tsx", "utf8");
const querySource = await readFile("src/lib/query/resourceQueries.ts", "utf8");
const economicsSource = await readFile("src/lib/api/economics.ts", "utf8");
const databaseSource = await readFile("src-tauri/src/services/database.rs", "utf8");
const commandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const tauriLibSource = await readFile("src-tauri/src/lib.rs", "utf8");
const currentBalanceQuerySource = databaseSource.match(
  /fn list_current_station_balance_snapshots_from_connection[\s\S]*?fn list_balance_snapshots_for_station_from_connection/,
)?.[0] ?? "";

assert.ok(
  dashboardSource.includes("currentStationBalanceSnapshotsQueryOptions") &&
    stationsSource.includes("currentStationBalanceSnapshotsQueryOptions") &&
    !dashboardSource.includes("balanceSnapshotsQueryOptions") &&
    !stationsSource.includes("balanceSnapshotsQueryOptions"),
  "dashboard and stations should read only current station balance facts",
);

assert.ok(
  querySource.includes("currentStationBalanceSnapshotsQueryOptions") &&
    querySource.includes("queryFn: listCurrentStationBalanceSnapshots"),
  "the shared active query should call the bounded current-balance API",
);

assert.ok(
  economicsSource.includes("export function listCurrentStationBalanceSnapshots()") &&
    economicsSource.includes('invoke<BalanceSnapshot[]>("list_current_station_balance_snapshots")'),
  "the frontend API should expose the bounded Tauri command",
);

assert.ok(
  databaseSource.includes("list_current_station_balance_snapshots") &&
    currentBalanceQuerySource.includes(
      "FROM balance_snapshots latest INDEXED BY idx_balance_snapshots_station_scope_updated",
    ) &&
    currentBalanceQuerySource.includes("SELECT latest.id") &&
    commandsSource.includes("pub fn list_current_station_balance_snapshots") &&
    tauriLibSource.includes("commands::list_current_station_balance_snapshots"),
  "the backend should project one indexed latest station-scope row per station",
);

assert.ok(
  !dashboardSource.includes("window.setInterval"),
  "dashboard should not own a page-local balance polling interval",
);
