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

async function importStationDetailViewModels() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-station-detail-"));
  const detailPath = join(tempRoot, "stationDetailViewModels.mjs");
  const groupFactsPath = join(tempRoot, "groupFacts.mjs");
  const groupCategoriesPath = join(tempRoot, "groupCategories.mjs");
  const balanceFactsPath = join(tempRoot, "balanceFacts.mjs");
  const timePath = join(tempRoot, "time.mjs");
  const formattersPath = join(tempRoot, "formatters.mjs");
  const groupTypesPath = join(tempRoot, "groupTypes.mjs");

  await writeFile(
    timePath,
    "export function toTimestampMillis(value) { return Date.parse(value); }",
    "utf8",
  );
  await writeFile(
    formattersPath,
    "export function formatTrimmedDecimal(value, digits) { return Number(value).toFixed(digits).replace(/\\.0+$/, '').replace(/(\\.\\d*?)0+$/, '$1'); }\nexport function effectiveRateMultiplierForCredit(value, creditPerCny) { return value == null || !Number.isFinite(value) ? null : value / (Number.isFinite(creditPerCny) && creditPerCny > 0 ? creditPerCny : 1); }",
    "utf8",
  );
  await writeFile(
    groupTypesPath,
    `export function isCollectedStationGroupBinding(binding) {
      return (
        binding.bindingKind === "station_group" &&
        binding.bindingStatus !== "disabled" &&
        binding.bindingStatus !== "missing" &&
        binding.bindingStatus !== "manual_legacy" &&
        binding.rateSource !== "legacy_key_group"
      );
    }`,
    "utf8",
  );

  await transpileTsFile("src/lib/groupCategories.ts", groupCategoriesPath);
  await transpileTsFile("src/lib/projections/groupFacts.ts", groupFactsPath, [
    ['@/lib/groupCategories', "./groupCategories.mjs"],
  ]);
  await transpileTsFile("src/lib/projections/balanceFacts.ts", balanceFactsPath, [
    ['@/lib/time', "./time.mjs"],
  ]);
  await transpileTsFile("src/features/stations/stationDetailViewModels.ts", detailPath, [
    ['@/lib/formatters', "./formatters.mjs"],
    ['@/lib/time', "./time.mjs"],
    ['@/lib/types/groupFacts', "./groupTypes.mjs"],
    ['@/lib/projections/groupFacts', "./groupFacts.mjs"],
    ['@/lib/projections/balanceFacts', "./balanceFacts.mjs"],
  ]);
  return import(`file://${detailPath.replaceAll("\\", "/")}`);
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
    groupBinding({
      id: "binding-tiny-rate",
      groupName: "tiny-rate",
      defaultRateMultiplier: 0.001,
      effectiveRateMultiplier: 0.001,
      rateSource: "sub2api_groups_rates",
      lastCheckedAt: "2026-07-07T12:00:00.000Z",
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
    groupRate({
      id: "rate-tiny-rate-latest",
      groupBindingId: "binding-tiny-rate",
      groupName: "tiny-rate",
      groupKeyHash: "remote:tiny-rate",
      defaultRateMultiplier: 0.001,
      effectiveRateMultiplier: 0.001,
      source: "sub2api_groups_rates",
      checkedAt: "2026-07-07T12:35:00.000Z",
    }),
  ],
  10,
);

assert.equal(rows.length, 2, "station detail should show the collected station groups");
assert.ok(
  rows.every((row) => row.bindingStatus !== "缺失" && row.groupName !== "claude-retired"),
  "station detail should remove missing groups from the current group table",
);
const claudeRow = rows.find((row) => row.groupName === "claude-aws");
const tinyRateRow = rows.find((row) => row.groupName === "tiny-rate");
assert.ok(claudeRow, "station detail should include the collected claude group");
assert.ok(tinyRateRow, "station detail should include the collected tiny-rate group");
assert.equal(
  claudeRow.sourceLabel,
  "Sub2API 分组倍率接口",
  "station detail should show the latest collected rate source instead of stale remote_scan",
);
assert.equal(
  claudeRow.effectiveRate,
  "0.08x",
  "station detail should show the exchange-ratio-adjusted current fact effective rate",
);
assert.equal(
  claudeRow.defaultRate,
  "0.022x",
  "station detail should show the exchange-ratio-adjusted latest collected default rate",
);
assert.equal(
  tinyRateRow.effectiveRate,
  "0.0001x",
  "station detail should preserve tiny exchange-ratio-adjusted group rates",
);
assert.match(claudeRow.lastChecked, /07\/07/, "station detail should use the latest collected check time");

const detailSource = await readFile("src/features/stations/stationDetailViewModels.ts", "utf8");
assert.ok(
  detailSource.includes("buildCurrentStationGroupFacts") &&
    detailSource.includes("isDisplayableStationGroupCurrentFact"),
  "station detail group rows should consume shared current group projection facts",
);
assert.ok(
  !detailSource.includes("function dedupeStationGroupBindings(") &&
    !detailSource.includes("function preferStationGroupBinding(") &&
    !detailSource.includes("function stationGroupBindingScore("),
  "station detail should not keep page-local station group de-duplication after Stage 5",
);

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
