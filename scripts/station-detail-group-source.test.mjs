import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

async function importStationDetailViewModels() {
  const source = await readFile("src/features/stations/stationDetailViewModels.ts", "utf8");
  const testableSource = source.replace(
    /import \{ isCollectedStationGroupBinding,[^\n]+from "@\/lib\/types\/groupFacts";/,
    `function isCollectedStationGroupBinding(binding) {
      return (
        binding.bindingKind === "station_group" &&
        binding.bindingStatus !== "disabled" &&
        binding.bindingStatus !== "manual_legacy" &&
        binding.rateSource !== "legacy_key_group"
      );
    }`,
  );
  const output = ts.transpileModule(testableSource, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      verbatimModuleSyntax: true,
    },
  }).outputText;
  return import(`data:text/javascript;base64,${Buffer.from(output, "utf8").toString("base64")}`);
}

const { buildGroupRows } = await importStationDetailViewModels();

const rows = buildGroupRows(
  [
    groupBinding({
      id: "binding-claude-aws",
      groupName: "claude-aws",
      defaultRateMultiplier: 0.8,
      effectiveRateMultiplier: 0.8,
      rateSource: "remote_scan",
      lastCheckedAt: "2026-07-05T15:29:00.000Z",
    }),
    groupBinding({
      id: "binding-missing-claude-aws",
      groupName: "claude-retired",
      bindingStatus: "missing",
      defaultRateMultiplier: 0.22,
      effectiveRateMultiplier: 0.22,
      rateSource: "sub2api_groups_rates",
      lastCheckedAt: "2026-07-04T16:59:00.000Z",
    }),
  ],
  [
    groupRate({
      id: "rate-claude-aws-latest",
      groupBindingId: "binding-claude-aws",
      groupName: "claude-aws",
      defaultRateMultiplier: 0.22,
      effectiveRateMultiplier: 0.22,
      source: "sub2api_groups_rates",
      checkedAt: "2026-07-07T12:34:00.000Z",
    }),
  ],
);

assert.equal(rows.length, 1, "station detail should show the collected station group");
assert.ok(
  rows.every((row) => row.bindingStatus !== "缺失" && row.groupName !== "claude-retired"),
  "station detail should remove missing groups from the current group table",
);
assert.equal(
  rows[0].sourceLabel,
  "Sub2API 分组倍率接口",
  "station detail should show the latest collected rate source instead of stale remote_scan",
);
assert.equal(
  rows[0].effectiveRate,
  "0.22x",
  "station detail should show the latest collected effective rate instead of stale remote_scan rate",
);
assert.equal(
  rows[0].defaultRate,
  "0.22x",
  "station detail should show the latest collected default rate instead of stale remote_scan rate",
);
assert.match(rows[0].lastChecked, /07\/07/, "station detail should use the latest collected check time");

function groupBinding(overrides = {}) {
  const timestamp = "2026-07-05T15:29:00.000Z";
  return {
    id: "binding",
    stationId: "station-a",
    stationKeyId: null,
    bindingKind: "station_group",
    parentGroupBindingId: null,
    groupKeyHash: "remote:claude-aws",
    groupIdHash: "claude-aws",
    groupName: "claude-aws",
    bindingStatus: "available",
    defaultRateMultiplier: null,
    userRateMultiplier: null,
    effectiveRateMultiplier: null,
    rateSource: null,
    confidence: 0.95,
    lastSeenAt: timestamp,
    lastCheckedAt: timestamp,
    lastRateChangedAt: timestamp,
    rawJsonRedacted: null,
    createdAt: timestamp,
    updatedAt: timestamp,
    ...overrides,
  };
}

function groupRate(overrides = {}) {
  const timestamp = "2026-07-07T12:34:00.000Z";
  return {
    id: "rate",
    stationId: "station-a",
    stationKeyId: null,
    groupBindingId: "binding",
    bindingKind: "station_group",
    groupKeyHash: "remote:claude-aws",
    groupName: "claude-aws",
    defaultRateMultiplier: null,
    userRateMultiplier: null,
    effectiveRateMultiplier: null,
    source: "sub2api_groups_rates",
    confidence: 0.9,
    rawJsonRedacted: null,
    checkedAt: timestamp,
    createdAt: timestamp,
    ...overrides,
  };
}
