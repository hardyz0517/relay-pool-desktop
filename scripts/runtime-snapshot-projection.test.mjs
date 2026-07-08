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

async function importRuntimeSnapshot() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-runtime-snapshot-"));
  const groupFactsPath = join(tempRoot, "groupFacts.mjs");
  const pricingFactsPath = join(tempRoot, "pricingFacts.mjs");
  const balanceFactsPath = join(tempRoot, "balanceFacts.mjs");
  const runtimeSnapshotPath = join(tempRoot, "runtimeSnapshot.mjs");
  const timePath = join(tempRoot, "time.mjs");

  await writeFile(
    timePath,
    "export function toTimestampMillis(value) { return value ? Date.parse(value) : Number.NaN; }",
    "utf8",
  );
  await transpileTsFile("src/lib/projections/groupFacts.ts", groupFactsPath);
  await transpileTsFile("src/lib/projections/pricingFacts.ts", pricingFactsPath, [
    ['@/lib/projections/groupFacts', "./groupFacts.mjs"],
  ]);
  await transpileTsFile("src/lib/projections/balanceFacts.ts", balanceFactsPath, [
    ['@/lib/time', "./time.mjs"],
  ]);
  await transpileTsFile("src/lib/projections/runtimeSnapshot.ts", runtimeSnapshotPath, [
    ['@/lib/projections/groupFacts', "./groupFacts.mjs"],
    ['@/lib/projections/pricingFacts', "./pricingFacts.mjs"],
    ['@/lib/projections/balanceFacts', "./balanceFacts.mjs"],
  ]);

  return import(`file://${runtimeSnapshotPath.replaceAll("\\", "/")}`);
}

const { buildRuntimeRouteSnapshot } = await importRuntimeSnapshot();

const snapshot = buildRuntimeRouteSnapshot({
  generatedAt: "2026-07-08T00:00:00.000Z",
  stations: [station({ id: "station-a", name: "Alpha", baseUrl: "https://alpha.example/v1" })],
  stationKeys: [
    key({
      id: "key-a",
      stationId: "station-a",
      name: "primary",
      apiKeyMasked: "sk-...masked",
      apiKeyPresent: true,
      enabled: true,
      priority: 5,
      groupBindingId: "binding-a",
      apiKey: "sk-live-plaintext",
    }),
    key({
      id: "key-disabled",
      stationId: "station-a",
      name: "disabled",
      apiKeyMasked: "sk-disabled",
      apiKeyPresent: true,
      enabled: false,
      priority: 1,
    }),
  ],
  capabilities: [
    capability({
      stationKeyId: "key-a",
      modelAllowlist: ["gpt-4.1"],
      preferredModels: ["gpt-4.1-mini"],
      onlyUseAsBackup: false,
      updatedAt: "2026-07-08T00:01:00.000Z",
    }),
  ],
  health: [
    health({
      stationKeyId: "key-a",
      consecutiveFailures: 2,
      cooldownUntil: "2026-07-08T00:10:00.000Z",
      updatedAt: "2026-07-08T00:02:00.000Z",
    }),
  ],
  groupBindings: [
    binding({
      id: "binding-a",
      stationId: "station-a",
      groupName: "vip",
      effectiveRateMultiplier: 0.75,
      rateSource: "collector",
    }),
  ],
  groupRates: [],
  pricingRules: [
    pricingRule({
      id: "rule-a",
      stationId: "station-a",
      groupBindingId: "binding-a",
      rateMultiplier: 0.8,
      confidence: 0.9,
      source: "manual",
    }),
  ],
  balances: [
    balance({
      id: "balance-a",
      stationId: "station-a",
      scope: "station",
      value: 42,
      currency: "CNY",
      status: "normal",
      collectedAt: "2026-07-08T00:03:00.000Z",
    }),
  ],
});

assert.equal(snapshot.version, 1);
assert.equal(snapshot.snapshotId, "runtime-route-2026-07-08T00:00:00.000Z");
assert.deepEqual(snapshot.candidates.map((candidate) => candidate.stationKeyId), ["key-a"]);
assert.equal(snapshot.candidates[0].secretRef.kind, "station_key_secret");
assert.equal(snapshot.candidates[0].secretRef.present, true);
assert.equal(snapshot.candidates[0].secretRef.masked, "sk-...masked");
assert.equal(JSON.stringify(snapshot).includes("sk-live-plaintext"), false);
assert.equal(snapshot.candidates[0].groupBindingId, "binding-a");
assert.equal(snapshot.candidates[0].rateMultiplier, 0.75);
assert.equal(snapshot.candidates[0].rateSource, "collector");
assert.equal(snapshot.candidates[0].modelPolicy.allowlist[0], "gpt-4.1");
assert.equal(snapshot.candidates[0].modelPolicy.preferredModels[0], "gpt-4.1-mini");
assert.equal(snapshot.candidates[0].pricingStatus.pricingRuleId, null);
assert.equal(snapshot.candidates[0].pricingStatus.priceConfidence, null);
assert.equal(snapshot.candidates[0].balanceStatus.value, 42);
assert.equal(snapshot.candidates[0].balanceStatus.scope, "station");
assert.equal(snapshot.candidates[0].healthStatus.consecutiveFailures, 2);
assert.equal(snapshot.candidates[0].healthStatus.cooldownUntil, "2026-07-08T00:10:00.000Z");
assert.equal(snapshot.candidates[0].evidence.groupFactIdentity, "binding:binding-a");
assert.equal(snapshot.candidates[0].evidence.balanceSnapshotId, "balance-a");
assert.equal(snapshot.candidates[0].evidence.capabilityUpdatedAt, "2026-07-08T00:01:00.000Z");
assert.equal(snapshot.candidates[0].evidence.healthUpdatedAt, "2026-07-08T00:02:00.000Z");

function station(overrides) {
  return {
    id: "station",
    name: "Station",
    stationType: "sub2api",
    baseUrl: "https://station.example/v1",
    apiKeyMasked: "sk-station",
    apiKeyPresent: true,
    keyCount: 1,
    enabled: true,
    priority: 0,
    creditPerCny: 1,
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

function key(overrides) {
  return {
    id: "key",
    stationId: "station",
    name: "key",
    apiKeyMasked: "sk-key",
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
    balanceScope: "station_key",
    status: "healthy",
    lastCheckedAt: null,
    lastUsedAt: null,
    note: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}

function capability(overrides) {
  return {
    stationKeyId: "key",
    supportsChatCompletions: true,
    supportsResponses: true,
    supportsEmbeddings: true,
    supportsStream: true,
    supportsTools: true,
    supportsVision: true,
    supportsReasoning: true,
    modelAllowlist: [],
    modelBlocklist: [],
    preferredModels: [],
    onlyUseAsBackup: false,
    routingTags: [],
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}

function health(overrides) {
  return {
    stationKeyId: "key",
    lastSuccessAt: null,
    lastFailureAt: null,
    consecutiveFailures: 0,
    successCount: 0,
    failureCount: 0,
    avgLatencyMs: null,
    lastErrorSummary: null,
    cooldownUntil: null,
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
    rateSource: "test",
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

function pricingRule(overrides) {
  return {
    id: "rule",
    stationId: "station",
    stationKeyId: null,
    groupBindingId: null,
    groupName: null,
    tierLabel: null,
    model: "*",
    inputPrice: null,
    outputPrice: null,
    fixedPrice: null,
    rateMultiplier: null,
    currency: "CNY",
    unit: "1M tokens",
    priceType: "multiplier",
    basePriceSource: null,
    normalizationStatus: "normalized",
    source: "test",
    confidence: 1,
    enabled: true,
    note: null,
    collectedAt: null,
    validFrom: null,
    validUntil: null,
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
    value: null,
    currency: "CNY",
    creditUnit: null,
    usedValue: null,
    totalValue: null,
    lowBalanceThreshold: null,
    status: "unknown",
    source: "test",
    confidence: 1,
    collectedAt: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}
