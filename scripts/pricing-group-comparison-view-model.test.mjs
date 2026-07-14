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

async function importPricingComparisonViewModel() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-pricing-group-view-model-"));
  const groupCategoriesPath = join(tempRoot, "groupCategories.mjs");
  const groupFactsPath = join(tempRoot, "groupFacts.mjs");
  const pricingFactsPath = join(tempRoot, "pricingFacts.mjs");
  const viewModelPath = join(tempRoot, "pricingComparisonViewModel.mjs");
  await transpileTsFile("src/lib/groupCategories.ts", groupCategoriesPath);
  await transpileTsFile("src/lib/projections/groupFacts.ts", groupFactsPath, [
    ['@/lib/groupCategories', "./groupCategories.mjs"],
  ]);
  await transpileTsFile("src/lib/projections/pricingFacts.ts", pricingFactsPath, [
    ['@/lib/projections/groupFacts', "./groupFacts.mjs"],
  ]);
  await transpileTsFile("src/features/pricing/pricingComparisonViewModel.ts", viewModelPath, [
    ["../../lib/projections/pricingFacts", "./pricingFacts.mjs"],
    ["../../lib/groupCategories", "./groupCategories.mjs"],
  ]);
  return import(`file://${viewModelPath.replaceAll("\\", "/")}`);
}

const { buildPricingComparisonViewModel } = await importPricingComparisonViewModel();

const view = buildPricingComparisonViewModel({
  stations: [
    station("station-a", "Alpha Relay", 10),
    station("station-b", "Beta Relay", 10),
  ],
  stationKeys: [stationKey("station-a", "key-a", "生产 Key")],
  groupBindings: [
    group("station-a", "gpt", "gpt普通分组", 0.8, { platform: "openai" }),
    group("station-a", "gpt-image", "GPT画图分组", 2, { platform: "openai" }),
    group("station-b", "claude", "claude_sonnet", 0.4, { platform: "anthropic" }),
    group("station-b", "gemini", "gemini_flash", 0.3, { platform: "gemini" }),
    group("station-b", "grok", "grok_fast", 0.7, { platform: "grok" }),
  ],
  groupRates: [],
  pricingRules: [],
  filters: {
    groupType: "all",
    query: "",
    stationId: "all",
  },
});

assert.deepEqual(
  view.sections.map((section) => ({ type: section.groupType, title: section.title })),
  [
    { type: "gpt", title: "GPT" },
    { type: "claude", title: "Claude" },
    { type: "gemini", title: "Gemini" },
    { type: "grok", title: "Grok" },
    { type: "image_generation", title: "生成图片" },
  ],
  "pricing comparison should expose group-type sections instead of concrete model sections",
);

const gpt = view.sections.find((section) => section.groupType === "gpt");
const image = view.sections.find((section) => section.groupType === "image_generation");
assert.ok(gpt, "GPT group section should exist");
assert.ok(image, "image-generation group section should exist");
assert.deepEqual(
  gpt.rows.map((row) => row.groupName),
  ["gpt普通分组"],
  "group names containing 图 must not be classified into the GPT group",
);
assert.deepEqual(
  image.rows.map((row) => row.groupName),
  ["GPT画图分组"],
  "group names containing 图 should be classified into the image-generation group",
);

assert.equal(view.metrics.comparableGroupCount, 5);
assert.equal(view.metrics.lowestEffectiveMultiplier, 0.03);
assert.equal(view.metrics.lowestEffectiveMultiplierLabel, "Gemini / Beta Relay / gemini_flash");

const gptOnly = buildPricingComparisonViewModel({
  stations: viewInputStations(),
  groupBindings: [
    group("station-a", "gpt", "gpt普通分组", 0.8, { platform: "openai" }),
    group("station-a", "gpt-image", "GPT画图分组", 2, { platform: "openai" }),
  ],
  groupRates: [],
  pricingRules: [],
  filters: {
    groupType: "gpt",
    query: "",
    stationId: "all",
  },
});
assert.deepEqual(
  gptOnly.sections.flatMap((section) => section.rows.map((row) => row.groupName)),
  ["gpt普通分组"],
  "the GPT filter should not leak image-generation groups",
);

const search = buildPricingComparisonViewModel({
  stations: viewInputStations(),
  stationKeys: [stationKey("station-a", "key-a", "生产 Key")],
  groupBindings: [group("station-a", "gpt", "gpt普通分组", 0.8, { platform: "openai" }, "key-a")],
  groupRates: [],
  pricingRules: [],
  filters: {
    groupType: "all",
    query: "生产",
    stationId: "all",
  },
});
assert.deepEqual(
  search.sections.flatMap((section) =>
    section.rows.map((row) => ({ groupName: row.groupName, stationKeyName: row.stationKeyName })),
  ),
  [{ groupName: "gpt普通分组", stationKeyName: "生产 Key" }],
  "search should still match station key names in group comparison mode",
);

const pageSource = await readFile("src/features/pricing/PricingPage.tsx", "utf8");
assert.ok(!pageSource.includes("官方输入"), "PricingPage should not display official model input prices");
assert.ok(!pageSource.includes("官方输出"), "PricingPage should not display official model output prices");
assert.ok(!pageSource.includes("输入价"), "PricingPage should not render model-derived input price columns");
assert.ok(!pageSource.includes("输出价"), "PricingPage should not render model-derived output price columns");
assert.ok(!pageSource.includes(">Key<"), "PricingPage should not render a standalone key column");
assert.ok(!pageSource.includes("全站分组"), "PricingPage should not render the all-station key placeholder");
assert.ok(pageSource.includes("分组倍率比较"), "PricingPage should describe the page as group multiplier comparison");
assert.ok(!pageSource.includes("覆盖分组类型"), "PricingPage should not show the covered group-type metric card");
assert.ok(!pageSource.includes("已有可比较倍率的分组类型"), "PricingPage should not show covered group-type metric detail copy");

function viewInputStations() {
  return [station("station-a", "Alpha Relay", 10), station("station-b", "Beta Relay", 1)];
}

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
    createdAt: "2026-07-06T00:00:00.000Z",
    updatedAt: "2026-07-06T00:00:00.000Z",
  };
}

function group(stationId, id, groupName, multiplier, rawJsonRedacted, stationKeyId = null) {
  return {
    id,
    stationId,
    stationKeyId,
    bindingKind: "station_group",
    parentGroupBindingId: null,
    groupKeyHash: `${stationId}-${id}`,
    groupIdHash: id,
    groupName,
    bindingStatus: "available",
    defaultRateMultiplier: multiplier,
    userRateMultiplier: null,
    effectiveRateMultiplier: multiplier,
    rateSource: "sub2api_groups_rates",
    confidence: 0.9,
    lastSeenAt: "2026-07-06T00:00:00.000Z",
    lastCheckedAt: "2026-07-06T00:00:00.000Z",
    lastRateChangedAt: "2026-07-06T00:00:00.000Z",
    rawJsonRedacted,
    createdAt: "2026-07-06T00:00:00.000Z",
    updatedAt: "2026-07-06T00:00:00.000Z",
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
    createdAt: "2026-07-06T00:00:00.000Z",
    updatedAt: "2026-07-06T00:00:00.000Z",
  };
}
