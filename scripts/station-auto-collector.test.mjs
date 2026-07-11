import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const libSource = await readFile("src-tauri/src/lib.rs", "utf8");
const servicesModSource = await readFile("src-tauri/src/services/mod.rs", "utf8");
const stationCollectorSource = await readFile("src-tauri/src/services/station_collectors.rs", "utf8").catch(
  () => "",
);
const collectorsSource = await readFile("src-tauri/src/services/collectors/mod.rs", "utf8");
const databaseSource = await readFile("src-tauri/src/services/database.rs", "utf8");
const sub2apiLoginSource = await readFile("src-tauri/src/services/collectors/sub2api.rs", "utf8");
const sub2apiAdapterSource = await readFile("src-tauri/src/services/collectors/adapters/sub2api.rs", "utf8");
const newapiAdapterSource = await readFile("src-tauri/src/services/collectors/adapters/newapi.rs", "utf8");
const openaiCompatibleAdapterSource = await readFile(
  "src-tauri/src/services/collectors/adapters/openai_compatible.rs",
  "utf8",
);
const stationsPageSource = await readFile("src/features/stations/StationsPage.tsx", "utf8");

assert.ok(
  servicesModSource.includes("pub mod station_collectors;"),
  "Tauri services should expose a station collector runner module",
);

assert.ok(
  libSource.includes("StationCollectorRunnerState::start"),
  "app setup should start the station collector runner",
);

assert.ok(
  libSource.includes("station_collector_runner"),
  "app setup should manage the station collector runner state",
);

assert.ok(
  databaseSource.includes("due_station_collectors") &&
    databaseSource.includes("collection_interval_minutes") &&
    databaseSource.includes("* 60000 <= ?1"),
  "station collector due query should use each station's collection interval",
);

assert.ok(
  stationCollectorSource.includes("CollectorTask::Balance") &&
    stationCollectorSource.includes("CollectorTask::Groups"),
  "station collector runner should collect balance and groups on each scheduled station run",
);

assert.ok(
  collectorsSource.includes("remote_keys::scan_remote_keys") &&
    collectorsSource.includes("append_remote_key_refresh_event"),
  "station group collection should refresh remote key discoveries during scheduled station runs",
);

assert.ok(
  stationsPageSource.includes("STATION_ASSET_REFRESH_INTERVAL_MS"),
  "station asset page should poll for automatic collector results",
);

assert.ok(
  stationsPageSource.includes("window.setInterval") &&
    stationsPageSource.includes("refreshStations"),
  "station asset polling should refresh the station list and balance snapshots",
);

assert.ok(
  sub2apiAdapterSource.includes("COLLECTOR_HTTP_TIMEOUT") &&
    sub2apiAdapterSource.includes(".timeout(COLLECTOR_HTTP_TIMEOUT)") &&
    newapiAdapterSource.includes("COLLECTOR_HTTP_TIMEOUT") &&
    newapiAdapterSource.includes(".timeout(COLLECTOR_HTTP_TIMEOUT)") &&
    openaiCompatibleAdapterSource.includes("COLLECTOR_HTTP_TIMEOUT") &&
    openaiCompatibleAdapterSource.includes(".timeout(COLLECTOR_HTTP_TIMEOUT)"),
  "collector HTTP requests should have a bounded timeout so one station cannot block the scheduled runner",
);

assert.ok(
  sub2apiAdapterSource.includes("CollectionAttemptBudget") &&
    sub2apiAdapterSource.includes("recoveryActions"),
  "Sub2API scheduled collection should use bounded adaptive recovery diagnostics",
);

assert.ok(
  sub2apiLoginSource.includes("login_access_token_with_budget"),
  "Sub2API auth recovery should share the collection task budget",
);

const accountBalanceFallbackSource = sub2apiAdapterSource.match(
  /fn collect_account_balance_fallback[\s\S]*?\r?\n}\r?\n\r?\nfn parse_account_balance/,
)?.[0];
assert.ok(accountBalanceFallbackSource, "Sub2API account balance fallback should exist");
assert.ok(
  accountBalanceFallbackSource.includes("login_and_store_access_token_with_budget") &&
    !accountBalanceFallbackSource.includes("login_and_store_access_token(database"),
  "Sub2API account balance fallback login should use the shared collection task budget",
);

assert.ok(
  stationsPageSource.includes("row.latestBalance?.updatedAt ?? row.latestBalance?.collectedAt") &&
    !stationsPageSource.includes("const lastCollectText = formatRelativeTime(station.lastPricingFetchedAt ?? station.updatedAt);"),
  "station asset balance timestamp should use the latest balance snapshot time, not the pricing collection timestamp",
);
