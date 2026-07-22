import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const libSource = await readFile("src-tauri/src/lib.rs", "utf8");
const servicesModSource = await readFile("src-tauri/src/services/mod.rs", "utf8");
const stationCollectorSource = await readFile("src-tauri/src/services/station_collectors.rs", "utf8").catch(
  () => "",
);
const stationCatalogSource = await readFile(
  "src-tauri/src/persistence/stores/station_catalog.rs",
  "utf8",
);
const sub2apiLoginSource = await readFile("src-tauri/src/services/collectors/sub2api.rs", "utf8");
const sub2apiAdapterSource = await readFile("src-tauri/src/services/collectors/adapters/sub2api.rs", "utf8");
const newapiAdapterSource = [
  await readFile("src-tauri/src/services/collectors/adapters/newapi/mod.rs", "utf8"),
  await readFile("src-tauri/src/services/collectors/adapters/newapi/client.rs", "utf8"),
  await readFile("src-tauri/src/services/collectors/adapters/newapi/auth.rs", "utf8"),
  await readFile("src-tauri/src/services/collectors/adapters/newapi/parsers.rs", "utf8"),
].join("\n");
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
  stationCatalogSource.includes("pub(crate) async fn due_collectors") &&
    stationCatalogSource.includes("collection_interval_minutes") &&
    stationCatalogSource.includes("* 60000) <= ?1"),
  "station collector due query should use each station's collection interval",
);

assert.ok(
  stationCollectorSource.includes("CollectorTask::Balance") &&
    stationCollectorSource.includes("CollectorTask::Groups"),
  "station collector runner should collect balance and groups on each scheduled station run",
);

assert.ok(
  stationsPageSource.includes("useActivityQuery(refreshEnabled, stationsQueryOptions())") &&
    stationsPageSource.includes("currentStationBalanceSnapshotsQueryOptions()"),
  "station asset page should refresh automatic collector results through shared activity queries",
);

assert.ok(
  stationsPageSource.includes("queryClient.invalidateQueries({ queryKey: queryKeys.balanceSnapshots })") &&
    stationsPageSource.includes("queryClient.cancelQueries({ queryKey: queryKeys.balanceSnapshots })"),
  "station asset refresh paths should update the station balance snapshots cache",
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

const accountProfileBalanceSource = sub2apiAdapterSource.match(
  /fn collect_account_profile_balance[\s\S]*?\r?\n}\r?\n\r?\nfn merge_account_profile_balance/,
)?.[0];
assert.ok(accountProfileBalanceSource, "Sub2API account profile balance collector should exist");
assert.ok(
  accountProfileBalanceSource.includes("login_and_store_access_token_with_budget") &&
    !accountProfileBalanceSource.includes("login_and_store_access_token(database"),
  "Sub2API account profile balance login should use the shared collection task budget",
);

assert.ok(
  sub2apiAdapterSource.indexOf("let account_profile_balance = collect_account_profile_balance") > -1 &&
    sub2apiAdapterSource.indexOf("let account_profile_balance = collect_account_profile_balance") <
      sub2apiAdapterSource.indexOf("if facts.balances.is_empty()"),
  "Sub2API balance collection should read the account profile before deciding whether usage fallback is needed",
);

assert.ok(
  sub2apiAdapterSource.includes("fn merge_account_profile_balance") &&
    sub2apiAdapterSource.includes("account_concurrency_limit") &&
    sub2apiAdapterSource.includes("parse_account_concurrency_limit"),
  "Sub2API account profile collection should parse and merge account concurrency limit",
);

assert.ok(
  stationsPageSource.includes("row.latestBalance?.updatedAt ?? row.latestBalance?.collectedAt") &&
    !stationsPageSource.includes("const lastCollectText = formatRelativeTime(station.lastPricingFetchedAt ?? station.updatedAt);"),
  "station asset balance timestamp should use the latest balance snapshot time, not the pricing collection timestamp",
);
