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

async function importModules() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-group-options-"));
  const groupFactsPath = join(tempRoot, "groupFacts.mjs");
  const groupOptionsPath = join(tempRoot, "groupOptionViewModels.mjs");
  const formattersPath = join(tempRoot, "formatters.mjs");
  await writeFile(
    formattersPath,
    "export function formatCompactMultiplier(value, fallback = '未采集倍率') { return value == null ? fallback : String(Number(value.toFixed(3))); }",
    "utf8",
  );
  await transpileTsFile("src/lib/projections/groupFacts.ts", groupFactsPath);
  await transpileTsFile("src/features/stations/groupOptionViewModels.ts", groupOptionsPath, [
    ['@/lib/formatters', "./formatters.mjs"],
    ['@/lib/projections/groupFacts', "./groupFacts.mjs"],
  ]);
  return {
    groupFacts: await import(`file://${groupFactsPath.replaceAll("\\", "/")}`),
    groupOptions: await import(`file://${groupOptionsPath.replaceAll("\\", "/")}`),
  };
}

const { groupFacts, groupOptions } = await importModules();
const { buildCurrentStationGroupFacts } = groupFacts;
const { buildStationGroupOptionsFromCurrentFactsForSelect } = groupOptions;

const currentFacts = buildCurrentStationGroupFacts({
  bindings: [
    binding({ id: "binding-current", groupName: "current", groupIdHash: "remote-current", effectiveRateMultiplier: 0.8 }),
    binding({ id: "binding-missing", groupName: "missing", bindingStatus: "missing", groupIdHash: "remote-missing", effectiveRateMultiplier: 0.1 }),
    binding({ id: "binding-legacy", groupName: "legacy", rateSource: "legacy_key_group", groupIdHash: "remote-legacy" }),
  ],
  rates: [],
});

const options = buildStationGroupOptionsFromCurrentFactsForSelect(currentFacts);
assert.deepEqual(
  options,
  [
    {
      value: "binding:binding-current",
      groupBindingId: "binding-current",
      groupIdHash: "remote-current",
      groupName: "current",
      rateMultiplier: 0.8,
      rateSource: "test",
      selectableForRemoteKey: true,
    },
  ],
  "selectable group options should come from displayable current group facts",
);

const groupOptionSource = await readFile("src/features/stations/groupOptionViewModels.ts", "utf8");
assert.ok(
  groupOptionSource.includes("buildStationGroupOptionsFromCurrentFacts") &&
    groupOptionSource.includes("isDisplayableStationGroupCurrentFact"),
  "group option view model should delegate current fact option construction to groupFacts projection helpers",
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
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}
