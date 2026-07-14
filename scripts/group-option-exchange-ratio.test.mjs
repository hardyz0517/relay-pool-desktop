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
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-group-option-exchange-"));
  const groupFactsPath = join(tempRoot, "groupFacts.mjs");
  const groupCategoriesPath = join(tempRoot, "groupCategories.mjs");
  const groupOptionsPath = join(tempRoot, "groupOptionViewModels.mjs");
  const formattersPath = join(tempRoot, "formatters.mjs");
  await writeFile(
    formattersPath,
    "export function formatCompactMultiplier(value, fallback = '未采集倍率') { return value == null ? fallback : String(Number(value.toFixed(3))); }\nexport function effectiveRateMultiplierForCredit(value, creditPerCny) { return value == null || !Number.isFinite(value) ? null : value / (Number.isFinite(creditPerCny) && creditPerCny > 0 ? creditPerCny : 1); }",
    "utf8",
  );
  await transpileTsFile("src/lib/groupCategories.ts", groupCategoriesPath);
  await transpileTsFile("src/lib/projections/groupFacts.ts", groupFactsPath, [
    ['@/lib/groupCategories', "./groupCategories.mjs"],
  ]);
  await transpileTsFile("src/features/stations/groupOptionViewModels.ts", groupOptionsPath, [
    ['@/lib/formatters', "./formatters.mjs"],
    ['@/lib/projections/groupFacts', "./groupFacts.mjs"],
  ]);
  return import(`file://${groupOptionsPath.replaceAll("\\", "/")}`);
}

const { buildStationGroupOptionFromRawMultiplierForSelect } = await importGroupOptionViewModels();

assert.equal(
  typeof buildStationGroupOptionFromRawMultiplierForSelect,
  "function",
  "group option view model should expose a raw-to-display multiplier helper",
);

assert.deepEqual(
  buildStationGroupOptionFromRawMultiplierForSelect(
    {
      id: "binding-ai-maok-pro",
      groupIdHash: "30",
      groupName: "ChatGPT-Pro20【超稳-VIP至尊通道】",
      defaultRateMultiplier: 2,
      userRateMultiplier: null,
      effectiveRateMultiplier: 2,
      inferredGroupCategory: "gpt",
      groupCategoryOverride: null,
      rateSource: "sub2api_groups_rates",
    },
    10,
  ),
  {
    value: "binding:binding-ai-maok-pro",
    groupBindingId: "binding-ai-maok-pro",
    groupIdHash: "30",
    groupName: "ChatGPT-Pro20【超稳-VIP至尊通道】",
    rateMultiplier: 0.2,
    inferredGroupCategory: "gpt",
    groupCategoryOverride: null,
    effectiveGroupCategory: "gpt",
    rateSource: "sub2api_groups_rates",
    selectableForRemoteKey: true,
  },
  "AI Maok raw group multipliers should be exchange-ratio adjusted for editable group options",
);

const addProviderSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");
assert.ok(
  addProviderSource.includes("buildStationGroupOptionFromRawMultiplierForSelect(saved, creditPerCny)") &&
    addProviderSource.includes("saveGroupRows(station.id, groupRows, currentCreditPerCny)") &&
    addProviderSource.includes("saveGroupRows(activeStationId, groupRows, currentCreditPerCny)"),
  "saved station groups should be converted through the same raw-to-display helper before UI merge",
);
assert.ok(
  !addProviderSource.includes("formatMultiplier(effectiveRateMultiplierForCredit(group.rateMultiplier, creditPerCny))"),
  "saved display multipliers should not be divided by creditPerCny a second time when merging key rows",
);
assert.ok(
  addProviderSource.includes("rateMultiplier: row.rateMultiplier,"),
  "saved display multipliers must not be written back into raw group draft rows",
);
assert.ok(
  !addProviderSource.includes("rateMultiplier: group.rateMultiplier === null ? row.rateMultiplier : formatMultiplier(group.rateMultiplier)"),
  "group draft rows should preserve raw multipliers so later option derivation and saves do not double-convert",
);
