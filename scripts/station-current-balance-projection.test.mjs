import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import ts from "typescript";

async function importBalanceProjection() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-balance-projection-"));
  const outputPath = join(tempRoot, "balanceFacts.mjs");
  const timePath = join(tempRoot, "time.mjs");
  let source = await readFile("src/lib/projections/balanceFacts.ts", "utf8");
  source = source.replaceAll("@/lib/time", "./time.mjs");
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      verbatimModuleSyntax: true,
    },
  }).outputText;
  await writeFile(outputPath, output, "utf8");
  await writeFile(
    timePath,
    "export function toTimestampMillis(value) { return Date.parse(value); }",
    "utf8",
  );
  return import(`file://${outputPath.replaceAll("\\", "/")}`);
}

const {
  buildCurrentStationBalanceFacts,
  currentStationBalanceFor,
  latestStationBalanceSnapshots,
} = await importBalanceProjection();

const stations = [
  station({ id: "station-a", balanceCny: 99, lowBalanceThresholdCny: 10, lastCheckedAt: "2026-07-08T01:00:00.000Z" }),
  station({ id: "station-b", balanceCny: 6, lowBalanceThresholdCny: 8, lastCheckedAt: "2026-07-08T02:00:00.000Z" }),
  station({ id: "station-c", balanceCny: null, lowBalanceThresholdCny: null, lastCheckedAt: null }),
];

const facts = buildCurrentStationBalanceFacts({
  stations,
  balances: [
    balance({ id: "key-newer", stationId: "station-a", stationKeyId: "key-a", scope: "station_key", value: 100, updatedAt: "2026-07-08T04:00:00.000Z" }),
    balance({ id: "station-old", stationId: "station-a", scope: "station", value: 12, status: "normal", updatedAt: "2026-07-08T03:00:00.000Z" }),
    balance({ id: "station-new", stationId: "station-a", scope: "station", value: 13, status: "low", lowBalanceThreshold: 20, source: "station_balance", updatedAt: "2026-07-08T05:00:00.000Z", collectedAt: "2026-07-08T05:00:01.000Z" }),
  ],
});

assert.equal(
  typeof latestStationBalanceSnapshots,
  "function",
  "balance facts should export the reusable latest station-scope projection",
);
assert.deepEqual(
  latestStationBalanceSnapshots([
    balance({ id: "station-old", stationId: "station-a", value: 12, updatedAt: "2026-07-08T03:00:00.000Z" }),
    balance({ id: "key-newer", stationId: "station-a", stationKeyId: "key-a", scope: "station_key", value: 100, updatedAt: "2026-07-08T06:00:00.000Z" }),
    balance({ id: "station-new", stationId: "station-a", value: 13, updatedAt: "2026-07-08T05:00:00.000Z" }),
    balance({ id: "station-b", stationId: "station-b", value: 8, updatedAt: "2026-07-08T04:00:00.000Z" }),
  ]).map((snapshot) => snapshot.id),
  ["station-new", "station-b"],
  "current station balance projection should return one latest station-scope row per station",
);

assert.deepEqual(
  latestStationBalanceSnapshots([
    balance({
      id: "same-updated-older-created",
      stationId: "station-a",
      updatedAt: "2026-07-08T05:00:00.000Z",
      createdAt: "2026-07-08T04:00:00.000Z",
    }),
    balance({
      id: "same-updated-newer-created",
      stationId: "station-a",
      updatedAt: "2026-07-08T05:00:00.000Z",
      createdAt: "2026-07-08T04:30:00.000Z",
    }),
    balance({
      id: "tie-a",
      stationId: "station-b",
      updatedAt: "2026-07-08T05:00:00.000Z",
      createdAt: "2026-07-08T04:30:00.000Z",
    }),
    balance({
      id: "tie-b",
      stationId: "station-b",
      updatedAt: "2026-07-08T05:00:00.000Z",
      createdAt: "2026-07-08T04:30:00.000Z",
    }),
  ]).map((snapshot) => snapshot.id),
  ["same-updated-newer-created", "tie-b"],
  "fallback projection should match SQL ordering by updatedAt, createdAt, then id",
);

assert.deepEqual(
  Array.from(facts.values()).map((fact) => ({
    stationId: fact.stationId,
    snapshotId: fact.snapshotId,
    value: fact.value,
    lowBalanceThreshold: fact.lowBalanceThreshold,
    status: fact.status,
    source: fact.source,
    updatedAt: fact.updatedAt,
    collectedAt: fact.collectedAt,
  })),
  [
    {
      stationId: "station-a",
      snapshotId: "station-new",
      value: 13,
      lowBalanceThreshold: 20,
      status: "low",
      source: "balance_snapshot",
      updatedAt: "2026-07-08T05:00:00.000Z",
      collectedAt: "2026-07-08T05:00:01.000Z",
    },
    {
      stationId: "station-b",
      snapshotId: null,
      value: 6,
      lowBalanceThreshold: 8,
      status: "low",
      source: "station_cache",
      updatedAt: "2026-07-08T02:00:00.000Z",
      collectedAt: null,
    },
    {
      stationId: "station-c",
      snapshotId: null,
      value: null,
      lowBalanceThreshold: null,
      status: null,
      source: "missing",
      updatedAt: null,
      collectedAt: null,
    },
  ],
  "current balance facts should prefer latest station-scope snapshots, ignore station-key snapshots, and fallback to station cache",
);

assert.equal(
  currentStationBalanceFor({ station: stations[1], balances: [] }).source,
  "station_cache",
  "single-station helper should use the same fallback rule",
);

const projectionSource = await readFile("src/lib/projections/balanceFacts.ts", "utf8");
assert.ok(
  !projectionSource.includes("invoke<") && !projectionSource.includes("listBalanceSnapshots"),
  "balance projection should stay pure and must not call Tauri or query services",
);

function station(overrides) {
  return {
    id: "station",
    name: "Station",
    stationType: "sub2api",
    baseUrl: "https://station.example.test",
    apiKeyMasked: "sk-...",
    apiKeyPresent: true,
    keyCount: 1,
    enabled: true,
    priority: 0,
    creditPerCny: 10,
    balanceRaw: null,
    balanceCny: null,
    lowBalanceThresholdCny: null,
    collectionIntervalMinutes: 5,
    status: "healthy",
    latencyMs: null,
    lastCheckedAt: null,
    lastPricingFetchedAt: null,
    note: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}

function balance(overrides) {
  return {
    id: "balance",
    stationId: "station",
    stationKeyId: null,
    scope: "station",
    value: 1,
    currency: "CNY",
    creditUnit: null,
    usedValue: null,
    totalValue: null,
    lowBalanceThreshold: null,
    status: "normal",
    source: "station_balance",
    confidence: 1,
    collectedAt: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}
