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
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-group-category-view-model-"));
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

async function importGroupVisualMeta() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-group-visual-meta-"));
  const groupCategoriesPath = join(tempRoot, "groupCategories.mjs");
  const visualMetaPath = join(tempRoot, "groupVisualMeta.mjs");
  await transpileTsFile("src/lib/groupCategories.ts", groupCategoriesPath);
  await transpileTsFile("src/features/stations/groupVisualMeta.ts", visualMetaPath, [
    ['@/lib/groupCategories', "./groupCategories.mjs"],
  ]);
  return import(`file://${visualMetaPath.replaceAll("\\", "/")}`);
}

async function importGroupCategories() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-group-categories-"));
  const groupCategoriesPath = join(tempRoot, "groupCategories.mjs");
  await transpileTsFile("src/lib/groupCategories.ts", groupCategoriesPath);
  return import(`file://${groupCategoriesPath.replaceAll("\\", "/")}`);
}

const { buildPricingComparisonViewModel } = await importPricingComparisonViewModel();
const { groupVisualMetaFor } = await importGroupVisualMeta();
const { inferGroupCategoryFromEvidence } = await importGroupCategories();

const view = buildPricingComparisonViewModel({
  stations: [station("station-a", "Alpha Relay", 10)],
  groupBindings: [
    group("station-a", "manual-code", "coding", 0.2, {
      inferredGroupCategory: "gpt",
      groupCategoryOverride: "claude",
    }),
    group("station-a", "unknown-newapi", "78 plus", 0.1, {
      inferredGroupCategory: "unknown",
      groupCategoryOverride: null,
    }),
    group("station-a", "embedding", "向量分组", 0.01, {
      inferredGroupCategory: "embedding",
      groupCategoryOverride: null,
    }),
    group("station-a", "rerank", "重排分组", 0.03, {
      inferredGroupCategory: "rerank",
      groupCategoryOverride: null,
    }),
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
  view.sections.map((section) => section.groupType),
  ["claude", "unknown"],
  "pricing comparison should preserve manually categorized and unknown NewAPI groups while hiding developer-only groups by default",
);

const developerView = buildPricingComparisonViewModel({
  stations: [station("station-a", "Alpha Relay", 10)],
  groupBindings: [
    group("station-a", "embedding", "向量分组", 0.01, {
      inferredGroupCategory: "embedding",
      groupCategoryOverride: null,
    }),
    group("station-a", "rerank", "重排分组", 0.03, {
      inferredGroupCategory: "rerank",
      groupCategoryOverride: null,
    }),
  ],
  groupRates: [],
  pricingRules: [],
  developerModeEnabled: true,
  filters: {
    groupType: "all",
    query: "",
    stationId: "all",
  },
});

assert.deepEqual(
  developerView.sections.map((section) => section.groupType),
  ["embedding", "rerank"],
  "Embedding and Rerank sections should render only when developer mode is enabled",
);

const claudeRows = view.sections.find((section) => section.groupType === "claude")?.rows ?? [];
assert.deepEqual(
  claudeRows.map((row) => row.groupName),
  ["coding"],
  "manual group category override should win over inferred group category",
);

const unknownRows = view.sections.find((section) => section.groupType === "unknown")?.rows ?? [];
assert.deepEqual(
  unknownRows.map((row) => row.groupName),
  ["78 plus"],
  "groups that cannot be classified should appear in the unknown section",
);

const legacyStructuredMappingView = buildPricingComparisonViewModel({
  stations: [station("station-1783311325734-4639", "Legacy Relay", 10)],
  groupBindings: [
    group("station-1783311325734-4639", "13", "13", 0.5, {
      inferredGroupCategory: null,
      groupCategoryOverride: null,
    }),
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
  legacyStructuredMappingView.sections.map((section) => section.groupType),
  ["claude"],
  "legacy structured station/group-id mappings should still classify old numeric groups when stored category fields are missing",
);

const categorizedBindingWithStaleRateView = buildPricingComparisonViewModel({
  stations: [station("station-ghost", "Xueyu Relay", 1)],
  groupBindings: [
    group("station-ghost", "current-plus", "plus", 0.0001, {
      inferredGroupCategory: "unknown",
      groupCategoryOverride: "gpt",
    }),
  ],
  groupRates: [
    rate("station-ghost", "stale-plus-rate", "plus", {
      groupKeyHash: "station-ghost-stale-plus",
      inferredGroupCategory: "unknown",
      effectiveRateMultiplier: null,
    }),
  ],
  pricingRules: [],
  filters: {
    groupType: "all",
    query: "",
    stationId: "all",
  },
});

assert.deepEqual(
  categorizedBindingWithStaleRateView.sections.map((section) => ({
    groupType: section.groupType,
    groupNames: section.rows.map((row) => row.groupName),
  })),
  [{ groupType: "gpt", groupNames: ["plus"] }],
  "same-station same-name stale rate-only facts should not create an unknown ghost row after the binding is categorized",
);

assert.equal(
  groupVisualMetaFor("coding", null, "claude").platform,
  "anthropic",
  "visual metadata should accept effective group category instead of relying only on raw platform fields",
);

assert.equal(
  inferGroupCategoryFromEvidence({
    groupName: "Claude Kiro",
    rawJsonRedacted: { platform: "anthropic", image_ratio: 2 },
  }),
  "claude",
  "Sub2API structured platform should not be misclassified as image generation because raw JSON contains image-related fields",
);

assert.equal(
  inferGroupCategoryFromEvidence({
    groupName: "GPT画图分组",
    rawJsonRedacted: { platform: "openai" },
  }),
  "image_generation",
  "image-like group names should still take precedence over OpenAI platform",
);

const editorSource = await readFile("src/features/stations/components/StationGroupRowsEditor.tsx", "utf8");
const selectControlSource = await readFile("src/components/ui/SelectControl.tsx", "utf8");
const addProviderSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");
assert.ok(
  editorSource.includes("groupCategoryOverride") && editorSource.includes("groupCategoryOptions"),
  "station group editor should expose a type dropdown backed by manual group category override",
);
assert.ok(
  editorSource.includes("SelectControl") && !editorSource.includes("<select"),
  "station group category selector should reuse the shared SelectControl instead of the browser-native select",
);
assert.ok(
  !editorSource.includes("自动："),
  "auto-detected group category labels should show the category directly without an 自动 prefix",
);
assert.ok(
  !editorSource.includes('source: event.target.value === autoGroupCategoryValue ? row.source : "manual"'),
  "manual category selection should not rewrite the row rate source; category override and multiplier source are separate concerns",
);
assert.ok(
  editorSource.includes('label: "跟随识别结果"') &&
    editorSource.includes("triggerLabel: groupCategoryLabel(inferredGroupCategory)") &&
    editorSource.includes('sectionLabel: index === 0 ? "手动指定" : undefined'),
  "the automatic category choice should be visually distinct while the closed trigger keeps showing the inferred category",
);
assert.ok(
  editorSource.includes("developerModeEnabled") &&
    editorSource.includes('definition.value !== "embedding"') &&
    editorSource.includes('definition.value !== "rerank"'),
  "Embedding and Rerank category choices should be hidden outside developer mode",
);
assert.ok(
  selectControlSource.includes("triggerLabel?: ReactNode") &&
    selectControlSource.includes("sectionLabel?: ReactNode") &&
    selectControlSource.includes("Math.min(320"),
  "the shared SelectControl should support reusable trigger labels and option section labels",
);
assert.ok(
  addProviderSource.includes("getSettings") &&
    addProviderSource.includes("developerModeEnabled={developerModeEnabled}"),
  "the station editor should receive the current developer-mode setting",
);

const detailViewModelSource = await readFile("src/features/stations/stationDetailViewModels.ts", "utf8");
const detailContentSource = await readFile("src/features/stations/components/StationDetailContent.tsx", "utf8");
assert.ok(
  detailViewModelSource.includes("effectiveGroupCategory") &&
    detailContentSource.includes("row.effectiveGroupCategory"),
  "station detail group badges should use the effective group category, including manual overrides",
);

function station(id, name, creditPerCny) {
  return {
    id,
    name,
    stationType: "newapi",
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

function group(stationId, id, groupName, multiplier, categoryFields) {
  return {
    id,
    stationId,
    stationKeyId: null,
    bindingKind: "station_group",
    parentGroupBindingId: null,
    groupKeyHash: `${stationId}-${id}`,
    groupIdHash: id,
    groupName,
    bindingStatus: "available",
    defaultRateMultiplier: multiplier,
    userRateMultiplier: null,
    effectiveRateMultiplier: multiplier,
    rateSource: "newapi_user_groups",
    confidence: 0.9,
    lastSeenAt: "2026-07-06T00:00:00.000Z",
    lastCheckedAt: "2026-07-06T00:00:00.000Z",
    lastRateChangedAt: "2026-07-06T00:00:00.000Z",
    rawJsonRedacted: null,
    createdAt: "2026-07-06T00:00:00.000Z",
    updatedAt: "2026-07-06T00:00:00.000Z",
    ...categoryFields,
  };
}

function rate(stationId, id, groupName, fields = {}) {
  return {
    id,
    stationId,
    stationKeyId: null,
    groupBindingId: null,
    bindingKind: "station_group",
    groupKeyHash: `${stationId}-${id}`,
    groupName,
    defaultRateMultiplier: null,
    userRateMultiplier: null,
    effectiveRateMultiplier: null,
    inferredGroupCategory: null,
    source: "sub2api_groups_rates",
    confidence: 0.9,
    rawJsonRedacted: null,
    checkedAt: "2026-07-06T00:00:00.000Z",
    createdAt: "2026-07-06T00:00:00.000Z",
    ...fields,
  };
}
