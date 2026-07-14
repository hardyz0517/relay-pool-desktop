import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import ts from "typescript";

async function transpileTsFile(sourcePath, outputPath, replacements = []) {
  let source = await readFile(sourcePath, "utf8");
  for (const [from, to] of replacements) {
    source = source.replaceAll(from, to);
  }
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      verbatimModuleSyntax: true,
    },
  }).outputText;
  await writeFile(outputPath, output, "utf8");
}

async function importStationAssetViewModels() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-station-assets-"));
  const groupCategoriesPath = join(tempRoot, "groupCategories.mjs");
  const groupFactsPath = join(tempRoot, "groupFacts.mjs");
  const balanceFactsPath = join(tempRoot, "balanceFacts.mjs");
  const assetPath = join(tempRoot, "stationAssetViewModels.mjs");
  const timePath = join(tempRoot, "time.mjs");
  await writeFile(
    timePath,
    "export function toTimestampMillis(value) { return Date.parse(value); }",
    "utf8",
  );
  await transpileTsFile("src/lib/groupCategories.ts", groupCategoriesPath);
  await transpileTsFile("src/lib/projections/groupFacts.ts", groupFactsPath, [
    ['@/lib/groupCategories', "./groupCategories.mjs"],
  ]);
  await transpileTsFile("src/lib/projections/balanceFacts.ts", balanceFactsPath, [
    ['@/lib/time', "./time.mjs"],
  ]);
  await transpileTsFile("src/features/stations/stationAssetViewModels.ts", assetPath, [
    ['@/lib/projections/groupFacts', "./groupFacts.mjs"],
    ['@/lib/projections/balanceFacts', "./balanceFacts.mjs"],
    ['@/lib/time', "./time.mjs"],
  ]);
  return import(`file://${assetPath.replaceAll("\\", "/")}`);
}

const { buildStationAssetRows, formatStationBalance } = await importStationAssetViewModels();

const rows = buildStationAssetRows({
  stations: [
    station({ id: "station-a", name: "Station A", balanceCny: 99, lowBalanceThresholdCny: 10 }),
    station({ id: "station-b", name: "Station B", balanceCny: 6, lowBalanceThresholdCny: 8 }),
  ],
  keysByStation: new Map([["station-a", [stationKey({ id: "key-a", stationId: "station-a" })]]]),
  balances: [
    balance({ id: "key-newer", stationId: "station-a", stationKeyId: "key-a", scope: "station_key", value: 100, updatedAt: "2026-07-08T05:00:00.000Z" }),
    balance({ id: "station-current", stationId: "station-a", scope: "station", value: 13, status: "low", updatedAt: "2026-07-08T04:00:00.000Z" }),
  ],
  snapshotsByStation: new Map(),
  groupBindingsByStation: new Map([
    [
      "station-a",
      [
        binding({ id: "binding-current", stationId: "station-a", groupName: "current", effectiveRateMultiplier: 0.8 }),
        binding({ id: "binding-missing", stationId: "station-a", groupName: "missing", bindingStatus: "missing", effectiveRateMultiplier: 0.1 }),
      ],
    ],
  ]),
  groupRatesByStation: new Map([
    [
      "station-a",
      [
        rate({ id: "rate-current", stationId: "station-a", groupBindingId: "binding-current", groupName: "current", effectiveRateMultiplier: 0.7, checkedAt: "2026-07-08T04:00:00.000Z" }),
      ],
    ],
  ]),
  changes: [],
});

assert.deepEqual(
  rows[0].rateChips.map((chip) => ({ label: chip.label, value: chip.value, tone: chip.tone })),
  [{ label: "current", value: "0.80x", tone: "good" }],
  "station assets should build rate chips from shared current group facts and hide missing groups",
);
assert.equal(formatStationBalance(rows[0]), "CNY 13.00", "station asset balance should prefer station-scope current balance");
assert.equal(formatStationBalance(rows[1]), "CNY 6.00", "station asset balance should fallback to station cache");

const assetSource = await readFile("src/features/stations/stationAssetViewModels.ts", "utf8");
assert.ok(
  assetSource.includes("buildCurrentStationGroupFacts") &&
    assetSource.includes("isDisplayableStationGroupCurrentFact") &&
    assetSource.includes("buildCurrentStationBalanceFacts"),
  "station asset rows should consume shared group and balance projections",
);

function station(overrides) {
  return {
    id: "station",
    name: "Station",
    stationType: "sub2api",
    websiteUrl: "https://station.example.test",
    apiBaseUrl: "https://station.example.test/v1",
    endpointRevision: 1,
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

function stationKey(overrides) {
  return {
    id: "key",
    stationId: "station",
    name: "Key",
    apiKeyMasked: "sk-...",
    apiKeyPresent: true,
    enabled: true,
    priority: 0,
    groupBindingId: null,
    groupIdHash: null,
    groupName: null,
    tierLabel: null,
    rateMultiplier: null,
    rateSource: null,
    rateCollectedAt: null,
    balanceScope: null,
    status: "healthy",
    lastCheckedAt: null,
    lastUsedAt: null,
    note: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}

function binding(overrides) {
  return {
    id: "binding",
    stationId: "station",
    stationKeyId: null,
    bindingKind: "station_group",
    parentGroupBindingId: null,
    groupKeyHash: "group-key",
    groupIdHash: null,
    groupName: "group",
    bindingStatus: "available",
    defaultRateMultiplier: null,
    userRateMultiplier: null,
    effectiveRateMultiplier: null,
    rateSource: "sub2api_groups_rates",
    confidence: 1,
    lastSeenAt: null,
    lastCheckedAt: null,
    lastRateChangedAt: null,
    rawJsonRedacted: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}

function rate(overrides) {
  return {
    id: "rate",
    stationId: "station",
    stationKeyId: null,
    groupBindingId: null,
    bindingKind: "station_group",
    groupKeyHash: "group-key",
    groupName: "group",
    defaultRateMultiplier: null,
    userRateMultiplier: null,
    effectiveRateMultiplier: null,
    source: "sub2api_groups_rates",
    confidence: 1,
    rawJsonRedacted: null,
    checkedAt: "2026-07-08T00:00:00.000Z",
    createdAt: "2026-07-08T00:00:00.000Z",
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
