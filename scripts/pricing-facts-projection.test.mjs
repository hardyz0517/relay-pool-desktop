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

async function importPricingProjection() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-pricing-projection-"));
  const groupCategoriesPath = join(tempRoot, "groupCategories.mjs");
  const groupFactsPath = join(tempRoot, "groupFacts.mjs");
  const groupCategoriesPath = join(tempRoot, "groupCategories.mjs");
  const pricingFactsPath = join(tempRoot, "pricingFacts.mjs");
  await transpileTsFile("src/lib/groupCategories.ts", groupCategoriesPath);
  await transpileTsFile("src/lib/projections/groupFacts.ts", groupFactsPath, [
    ['@/lib/groupCategories', "./groupCategories.mjs"],
  ]);
  await transpileTsFile("src/lib/projections/pricingFacts.ts", pricingFactsPath, [
    ['@/lib/projections/groupFacts', "./groupFacts.mjs"],
  ]);
  return import(`file://${pricingFactsPath.replaceAll("\\", "/")}`);
}

const { buildPricingGroupCandidates } = await importPricingProjection();

const candidates = buildPricingGroupCandidates({
  stations: [station("station-a", "Station A", 10)],
  stationKeys: [stationKey("station-a", "key-a", "Key A")],
  groupBindings: [
    binding({
      id: "binding-current",
      stationId: "station-a",
      stationKeyId: "key-a",
      groupKeyHash: "local-current",
      groupIdHash: "remote-current",
      groupName: "default",
      userRateMultiplier: null,
      effectiveRateMultiplier: 0.8,
      defaultRateMultiplier: 1,
      bindingStatus: "available",
    }),
    binding({
      id: "binding-rule-fallback",
      stationId: "station-a",
      groupKeyHash: "local-rule",
      groupIdHash: "remote-rule",
      groupName: "rule only",
      userRateMultiplier: null,
      effectiveRateMultiplier: null,
      defaultRateMultiplier: null,
      bindingStatus: "available",
    }),
    binding({
      id: "binding-missing",
      stationId: "station-a",
      groupKeyHash: "local-missing",
      groupIdHash: "remote-missing",
      groupName: "missing group",
      userRateMultiplier: null,
      effectiveRateMultiplier: null,
      defaultRateMultiplier: 1,
      bindingStatus: "missing",
    }),
  ],
  groupRates: [
    rate({
      id: "rate-current",
      stationId: "station-a",
      groupBindingId: "binding-current",
      groupKeyHash: "local-current",
      groupName: "default",
      effectiveRateMultiplier: 0.7,
      checkedAt: "2026-07-08T01:00:00.000Z",
    }),
    rate({
      id: "rate-shadow",
      stationId: "station-a",
      groupBindingId: null,
      groupKeyHash: "local-current",
      groupName: "default",
      effectiveRateMultiplier: 0.7,
      checkedAt: "2026-07-08T02:00:00.000Z",
    }),
    rate({
      id: "rate-missing",
      stationId: "station-a",
      groupBindingId: "binding-missing",
      groupKeyHash: "local-missing",
      groupName: "missing group",
      effectiveRateMultiplier: 0.01,
      checkedAt: "2026-07-08T03:00:00.000Z",
    }),
  ],
  pricingRules: [
    pricingRule({
      id: "rule-fallback",
      stationId: "station-a",
      groupBindingId: "binding-rule-fallback",
      groupName: "rule only",
      model: "gpt-5-mini",
      rateMultiplier: 0.42,
      enabled: true,
    }),
  ],
});

assert.deepEqual(
  candidates.map((candidate) => ({
    identityKey: candidate.identityKey,
    stationName: candidate.station.name,
    stationKeyName: candidate.stationKeyName,
    groupBindingId: candidate.groupBindingId,
    groupRateRecordId: candidate.groupRateRecordId,
    groupName: candidate.groupName,
    groupMultiplier: candidate.groupMultiplier,
    pricingRuleId: candidate.pricingRuleId,
  })),
  [
    {
      identityKey: "binding:binding-current",
      stationName: "Station A",
      stationKeyName: "Key A",
      groupBindingId: "binding-current",
      groupRateRecordId: "rate-current",
      groupName: "default",
      groupMultiplier: 0.8,
      pricingRuleId: null,
    },
    {
      identityKey: "binding:binding-rule-fallback",
      stationName: "Station A",
      stationKeyName: null,
      groupBindingId: "binding-rule-fallback",
      groupRateRecordId: null,
      groupName: "rule only",
      groupMultiplier: 0.42,
      pricingRuleId: "rule-fallback",
    },
  ],
  "pricing candidates should reuse current group facts, suppress shadow rates, hide missing groups, and use pricingRules only as multiplier fallback",
);

const projectionSource = await readFile("src/lib/projections/pricingFacts.ts", "utf8");
assert.ok(
  projectionSource.includes("buildCurrentStationGroupFacts"),
  "pricing projection should consume the shared current group projection",
);
assert.ok(
  !projectionSource.includes('from "@/features/') &&
    !projectionSource.includes("invoke<") &&
    !projectionSource.includes("getLocalAccessKey") &&
    !projectionSource.includes("upsertPricingRule"),
  "pricing projection should stay pure and must not import feature modules, call Tauri, read secrets, or write pricing rules",
);

function station(id, name, creditPerCny) {
  return {
    id,
    name,
    stationType: "sub2api",
    websiteUrl: `https://${id}.example.test`,
    apiBaseUrl: `https://${id}.example.test/v1`,
    endpointRevision: 1,
    apiKeyMasked: "sk-...",
    apiKeyPresent: true,
    keyCount: 1,
    enabled: true,
    priority: 0,
    creditPerCny,
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
  };
}

function stationKey(stationId, id, name) {
  return {
    id,
    stationId,
    name,
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
    source: "test",
    confidence: 1,
    rawJsonRedacted: null,
    checkedAt: "2026-07-08T00:00:00.000Z",
    createdAt: "2026-07-08T00:00:00.000Z",
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
    model: "gpt-5-mini",
    inputPrice: null,
    outputPrice: null,
    fixedPrice: null,
    rateMultiplier: null,
    currency: "CNY",
    unit: "multiplier",
    priceType: "rate_multiplier",
    basePriceSource: null,
    normalizationStatus: "normalized",
    source: "test",
    confidence: 1,
    enabled: true,
    note: null,
    collectedAt: "2026-07-08T00:00:00.000Z",
    validFrom: null,
    validUntil: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}
