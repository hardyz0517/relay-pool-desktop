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

async function importGroupOptionViewModels() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-group-option-view-models-"));
  const formattersPath = join(tempRoot, "formatters.mjs");
  const groupFactsPath = join(tempRoot, "groupFacts.mjs");
  const viewModelsPath = join(tempRoot, "groupOptionViewModels.mjs");
  await writeFile(
    formattersPath,
    "export function formatCompactMultiplier(value, fallback) { return value == null ? fallback : String(value); }\nexport function effectiveRateMultiplierForCredit(value, creditPerCny) { return value == null ? null : value / (creditPerCny || 1); }\n",
    "utf8",
  );
  await writeFile(
    groupFactsPath,
    "export function buildStationGroupOptionsFromCurrentFacts(facts) { return facts; }\nexport function isDisplayableStationGroupCurrentFact() { return true; }\n",
    "utf8",
  );
  await transpileTsFile("src/features/stations/groupOptionViewModels.ts", viewModelsPath, [
    ['@/lib/formatters', "./formatters.mjs"],
    ['@/lib/projections/groupFacts', "./groupFacts.mjs"],
  ]);
  return import(`file://${viewModelsPath.replaceAll("\\", "/")}`);
}

const { findMatchingGroupOption } = await importGroupOptionViewModels();

const canonicalGroupOption = {
  value: "binding:station-group-005",
  groupBindingId: "station-group-005",
  groupIdHash: "2",
  groupName: "倍率动态调整，分组上限0.05倍率",
  rateMultiplier: 0.05,
};

assert.equal(
  findMatchingGroupOption(
    {
      groupBindingId: "legacy-key-binding-005",
      groupIdHash: "legacy-local-hash",
      groupName: "倍率动态调整，分组上限0.05倍率",
    },
    [canonicalGroupOption],
  )?.groupBindingId,
  "station-group-005",
  "legacy key-level bindings should resolve to the selectable station group with the same group name instead of forcing a current-only option",
);
