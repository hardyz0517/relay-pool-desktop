import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

async function importTsModule(path) {
  const source = await readFile(path, "utf8");
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      verbatimModuleSyntax: true,
    },
  }).outputText;
  return import(`data:text/javascript;base64,${Buffer.from(output, "utf8").toString("base64")}`);
}

const {
  buildCurrentStationGroupFacts,
  buildStationGroupOptionsFromCurrentFacts,
  latestGroupRatesByBindingOrHash,
} = await importTsModule("src/lib/projections/groupFacts.ts");

const bindings = [
  binding({
    id: "binding-current",
    stationId: "station-a",
    groupKeyHash: "local-current",
    groupIdHash: "remote-current",
    groupName: "default",
    userRateMultiplier: null,
    effectiveRateMultiplier: 0.8,
    defaultRateMultiplier: 1,
    bindingStatus: "available",
  }),
  binding({
    id: "binding-same-name-a",
    stationId: "station-a",
    groupKeyHash: "local-a",
    groupIdHash: "same-remote",
    groupName: "shared name",
    userRateMultiplier: 0.5,
    effectiveRateMultiplier: 0.7,
    defaultRateMultiplier: 1,
    bindingStatus: "available",
  }),
  binding({
    id: "binding-same-name-b",
    stationId: "station-a",
    groupKeyHash: "local-b",
    groupIdHash: "same-remote",
    groupName: "shared name",
    userRateMultiplier: null,
    effectiveRateMultiplier: 0.6,
    defaultRateMultiplier: 1,
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
];

const rates = [
  rate({
    id: "rate-current-newer",
    stationId: "station-a",
    groupBindingId: "binding-current",
    groupKeyHash: "local-current",
    groupName: "default",
    userRateMultiplier: 0.75,
    effectiveRateMultiplier: 0.75,
    defaultRateMultiplier: 1,
    checkedAt: "2026-07-07T02:00:00.000Z",
  }),
  rate({
    id: "rate-current-shadow",
    stationId: "station-a",
    groupBindingId: null,
    groupKeyHash: "local-current",
    groupName: "default",
    userRateMultiplier: 0.7,
    effectiveRateMultiplier: 0.7,
    defaultRateMultiplier: 1,
    checkedAt: "2026-07-07T03:00:00.000Z",
  }),
  rate({
    id: "rate-missing-history",
    stationId: "station-a",
    groupBindingId: "binding-missing",
    groupKeyHash: "local-missing",
    groupName: "missing group",
    userRateMultiplier: 0.01,
    effectiveRateMultiplier: 0.01,
    defaultRateMultiplier: 1,
    checkedAt: "2026-07-07T04:00:00.000Z",
  }),
];

const latestRates = latestGroupRatesByBindingOrHash(rates);
assert.equal(latestRates.get("binding:binding-current")?.id, "rate-current-newer");
assert.equal(latestRates.get("group-key:local-current")?.id, "rate-current-shadow");

const facts = buildCurrentStationGroupFacts({ bindings, rates });

assert.deepEqual(
  facts.map((fact) => ({
    identityKey: fact.identityKey,
    groupBindingId: fact.groupBindingId,
    groupName: fact.groupName,
    rateMultiplier: fact.rateMultiplier,
    available: fact.available,
    rateEvidenceId: fact.rateEvidenceId,
  })),
  [
    {
      identityKey: "binding:binding-current",
      groupBindingId: "binding-current",
      groupName: "default",
      rateMultiplier: 0.8,
      available: true,
      rateEvidenceId: "rate-current-newer",
    },
    {
      identityKey: "binding:binding-same-name-a",
      groupBindingId: "binding-same-name-a",
      groupName: "shared name",
      rateMultiplier: 0.5,
      available: true,
      rateEvidenceId: null,
    },
    {
      identityKey: "binding:binding-same-name-b",
      groupBindingId: "binding-same-name-b",
      groupName: "shared name",
      rateMultiplier: 0.6,
      available: true,
      rateEvidenceId: null,
    },
    {
      identityKey: "binding:binding-missing",
      groupBindingId: "binding-missing",
      groupName: "missing group",
      rateMultiplier: 0.01,
      available: false,
      rateEvidenceId: "rate-missing-history",
    },
  ],
  "current facts should preserve durable identities, use rate fallback, and not revive missing groups",
);

const options = buildStationGroupOptionsFromCurrentFacts(facts);
assert.deepEqual(
  options.map((option) => ({
    value: option.value,
    groupBindingId: option.groupBindingId,
    groupName: option.groupName,
    rateMultiplier: option.rateMultiplier,
    selectableForRemoteKey: option.selectableForRemoteKey,
  })),
  [
    {
      value: "binding:binding-current",
      groupBindingId: "binding-current",
      groupName: "default",
      rateMultiplier: 0.8,
      selectableForRemoteKey: true,
    },
    {
      value: "binding:binding-same-name-a",
      groupBindingId: "binding-same-name-a",
      groupName: "shared name",
      rateMultiplier: 0.5,
      selectableForRemoteKey: true,
    },
    {
      value: "binding:binding-same-name-b",
      groupBindingId: "binding-same-name-b",
      groupName: "shared name",
      rateMultiplier: 0.6,
      selectableForRemoteKey: true,
    },
  ],
  "group options should include only available current facts and keep duplicate display names distinct",
);

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
    createdAt: "2026-07-07T00:00:00.000Z",
    updatedAt: "2026-07-07T00:00:00.000Z",
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
    checkedAt: "2026-07-07T00:00:00.000Z",
    createdAt: "2026-07-07T00:00:00.000Z",
    ...overrides,
  };
}

const projectionSource = await readFile("src/lib/projections/groupFacts.ts", "utf8");
assert.ok(
  !projectionSource.includes("from \"@/features/") &&
    !projectionSource.includes("invoke<") &&
    !projectionSource.includes("getLocalAccessKey") &&
    !projectionSource.includes("upsertStationGroupBinding"),
  "group projection should stay pure and must not import feature modules, call Tauri, read secrets, or write bindings",
);
assert.ok(
  projectionSource.includes("binding.userRateMultiplier") &&
    projectionSource.includes("binding.effectiveRateMultiplier") &&
    projectionSource.includes("latestRate?.userRateMultiplier") &&
    projectionSource.includes("latestRate?.effectiveRateMultiplier") &&
    projectionSource.includes("binding.defaultRateMultiplier") &&
    projectionSource.includes("latestRate?.defaultRateMultiplier"),
  "group projection should encode the documented rate fallback order",
);
