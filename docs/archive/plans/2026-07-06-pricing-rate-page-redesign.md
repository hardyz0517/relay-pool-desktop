# Pricing Rate Page Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current matrix-style `价格 / 倍率` page with concrete model sections that compare station/group prices within one model at a time, using official model prices, group multipliers, and `Station.creditPerCny`.

**Architecture:** Add a curated frontend official model catalog, add a focused comparison view model that joins catalog entries with station group facts, then make `PricingPage.tsx` render model sections from that view model. Remove the interim wildcard model path so the page never renders `gpt-*`, `claude-*`, or `gemini-*`.

**Tech Stack:** React 18, TypeScript, Vite, Tauri IPC wrappers, existing Relay Pool UI primitives, Node script tests, `pnpm.cmd build`.

---

## Source Notes

Use current official pricing pages when reviewing or refreshing catalog constants:

- OpenAI API pricing and model pages: `https://developers.openai.com/api/docs/pricing`, `https://developers.openai.com/api/docs/models/gpt-5-mini`
- Anthropic pricing: `https://platform.claude.com/docs/en/about-claude/pricing`
- Google Gemini pricing: `https://ai.google.dev/gemini-api/docs/pricing`

The first implementation slice should keep the catalog small and obvious. Do not add a full provider catalog.

## File Structure

- Create `src/features/pricing/officialModelCatalog.ts`
  - Owns concrete model catalog entries and group family matchers.
  - Exports only static catalog data and small pure helpers.
- Create `src/features/pricing/pricingComparisonViewModel.ts`
  - Owns station/group/model join logic, pricing math, filters, sorting, and metrics.
  - Has no React imports.
- Create `scripts/pricing-comparison-view-model.test.mjs`
  - Node script test that imports TypeScript helpers through `typescript.transpileModule`.
  - Locks concrete model rows, multi-group rows, recharge multiplier math, sorting, and source delegation.
- Modify `src/features/pricing/PricingPage.tsx`
  - Load the same backend data as today.
  - Own toolbar state and rendering only.
  - Render model sections from `buildPricingComparisonViewModel`.
- Modify `src/features/pricing/pricingMatrix.ts`
  - Remove `buildGroupRateOnlyPricingRules` and wildcard fallback helpers.
  - Keep existing matrix helpers only if still referenced elsewhere; otherwise leave them unused but harmless until a later cleanup.
- Modify `src/features/pricing/rateSnapshotParser.ts`
  - Remove interim `modelProvider`, `modelFamilies`, and `modelPrefixes` fields if the final comparison view model reads group facts directly.

## Task 1: Add the Failing Comparison View Model Test

**Files:**
- Create: `scripts/pricing-comparison-view-model.test.mjs`
- Read: `src/features/pricing/PricingPage.tsx`

- [ ] **Step 1: Write the failing test**

Create `scripts/pricing-comparison-view-model.test.mjs` with this complete content:

```js
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

const { officialModelCatalog, enabledOfficialModelCatalog } = await importTsModule(
  "src/features/pricing/officialModelCatalog.ts",
);
const { buildPricingComparisonViewModel } = await importTsModule(
  "src/features/pricing/pricingComparisonViewModel.ts",
);

assert.ok(
  officialModelCatalog.length >= 4,
  "catalog should seed a small concrete model set",
);
assert.ok(
  officialModelCatalog.every((model) => !/\\*$/.test(model.modelId) && !/\\*$/.test(model.displayName)),
  "official catalog must not use wildcard model IDs or labels",
);
assert.ok(
  enabledOfficialModelCatalog().some((model) => model.provider === "openai"),
  "default catalog should include OpenAI models",
);
assert.ok(
  enabledOfficialModelCatalog().some((model) => model.provider === "anthropic"),
  "default catalog should include Anthropic models",
);
assert.ok(
  enabledOfficialModelCatalog().some((model) => model.provider === "google"),
  "default catalog should include Google models",
);

const stations = [
  station("station-a", "杂鱼丸中转站", 10),
  station("station-b", "FYLinkApi", 1),
  station("station-c", "WHITEXI", 2),
];

const groupBindings = [
  group("station-a", "group-a-default", "default", 0.8, "OpenAI green default", "2026-07-06T01:00:00.000Z"),
  group("station-a", "group-a-pro", "gpt_pro", 0.5, "OpenAI green pro", "2026-07-06T02:00:00.000Z"),
  group("station-b", "group-b-claude", "claude", 0.1, "Anthropic yellow", "2026-07-06T03:00:00.000Z"),
  group("station-b", "group-b-gemini", "gemini", 0.2, "Google Gemini", "2026-07-06T04:00:00.000Z"),
  group("station-c", "group-c-misc", "杂色", 1, "misc", "2026-07-06T05:00:00.000Z"),
];

const groupRates = [];
const pricingRules = [];
const modelEvidence = [
  { stationId: "station-a", modelId: "gpt-5-mini", status: "discovered" },
  { stationId: "station-b", modelId: "claude-sonnet-5", status: "discovered" },
];

const view = buildPricingComparisonViewModel({
  models: [
    {
      provider: "openai",
      modelId: "gpt-5-mini",
      displayName: "GPT-5 mini",
      officialInputPrice: 0.25,
      officialOutputPrice: 2,
      currency: "USD",
      unit: "per_1m_tokens",
      aliases: ["gpt-5-mini"],
      groupMatchers: ["openai", "gpt", "default", "green"],
      enabledByDefault: true,
    },
    {
      provider: "anthropic",
      modelId: "claude-sonnet-5",
      displayName: "Claude Sonnet 5",
      officialInputPrice: 2,
      officialOutputPrice: 10,
      currency: "USD",
      unit: "per_1m_tokens",
      aliases: ["claude-sonnet-5"],
      groupMatchers: ["anthropic", "claude", "yellow"],
      enabledByDefault: true,
    },
    {
      provider: "google",
      modelId: "gemini-3.1-flash-lite",
      displayName: "Gemini 3.1 Flash-Lite",
      officialInputPrice: 0.25,
      officialOutputPrice: 1.5,
      currency: "USD",
      unit: "per_1m_tokens",
      aliases: ["gemini-3.1-flash-lite"],
      groupMatchers: ["google", "gemini"],
      enabledByDefault: true,
    },
  ],
  stations,
  groupBindings,
  groupRates,
  pricingRules,
  modelEvidence,
  filters: {
    provider: "all",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: false,
  },
});

assert.deepEqual(
  view.sections.map((section) => section.modelId),
  ["claude-sonnet-5", "gemini-3.1-flash-lite", "gpt-5-mini"],
  "sections should be concrete models sorted by display name/model name",
);

const gpt = view.sections.find((section) => section.modelId === "gpt-5-mini");
assert.ok(gpt, "GPT section should exist");
assert.equal(gpt.rows.length, 2, "same station should show multiple matching GPT groups");
assert.deepEqual(
  gpt.rows.map((row) => row.groupName),
  ["gpt_pro", "default"],
  "GPT rows should sort by estimated output price ascending within the GPT section",
);
assert.equal(gpt.rows[0].stationName, "杂鱼丸中转站");
assert.equal(gpt.rows[0].groupMultiplier, 0.5);
assert.equal(gpt.rows[0].creditPerCny, 10);
assert.equal(gpt.rows[0].effectiveMultiplier, 0.05);
assert.equal(gpt.rows[0].estimatedInputCny, 0.0125);
assert.equal(gpt.rows[0].estimatedOutputCny, 0.1);
assert.equal(gpt.rows[0].evidenceStatus, "discovered");
assert.equal(gpt.rows[1].effectiveMultiplier, 0.08);
assert.equal(gpt.rows[1].estimatedOutputCny, 0.16);

const claude = view.sections.find((section) => section.modelId === "claude-sonnet-5");
assert.ok(claude, "Claude section should exist");
assert.equal(claude.rows.length, 1);
assert.equal(claude.rows[0].stationName, "FYLinkApi");
assert.equal(claude.rows[0].groupName, "claude");
assert.equal(claude.rows[0].effectiveMultiplier, 0.1);
assert.equal(claude.rows[0].estimatedOutputCny, 1);
assert.equal(claude.rows[0].evidenceStatus, "discovered");

const gemini = view.sections.find((section) => section.modelId === "gemini-3.1-flash-lite");
assert.ok(gemini, "Gemini section should exist");
assert.equal(gemini.rows.length, 1);
assert.equal(gemini.rows[0].groupName, "gemini");
assert.equal(gemini.rows[0].evidenceStatus, "unverified");

const openaiOnly = buildPricingComparisonViewModel({
  models: view.sections.map((section) => ({
    provider: section.provider,
    modelId: section.modelId,
    displayName: section.displayName,
    officialInputPrice: section.officialInputPrice,
    officialOutputPrice: section.officialOutputPrice,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: [section.modelId],
    groupMatchers:
      section.provider === "openai"
        ? ["openai", "gpt", "default", "green"]
        : section.provider === "anthropic"
          ? ["anthropic", "claude", "yellow"]
          : ["google", "gemini"],
    enabledByDefault: true,
  })),
  stations,
  groupBindings,
  groupRates,
  pricingRules,
  modelEvidence,
  filters: {
    provider: "openai",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: false,
  },
});
assert.deepEqual(openaiOnly.sections.map((section) => section.modelId), ["gpt-5-mini"]);

const verifiedOnly = buildPricingComparisonViewModel({
  models: view.sections.map((section) => ({
    provider: section.provider,
    modelId: section.modelId,
    displayName: section.displayName,
    officialInputPrice: section.officialInputPrice,
    officialOutputPrice: section.officialOutputPrice,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: [section.modelId],
    groupMatchers:
      section.provider === "openai"
        ? ["openai", "gpt", "default", "green"]
        : section.provider === "anthropic"
          ? ["anthropic", "claude", "yellow"]
          : ["google", "gemini"],
    enabledByDefault: true,
  })),
  stations,
  groupBindings,
  groupRates,
  pricingRules,
  modelEvidence,
  filters: {
    provider: "all",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: true,
  },
});
assert.ok(
  verifiedOnly.sections.every((section) =>
    section.rows.every((row) => row.evidenceStatus === "discovered"),
  ),
  "verified-only filter should hide unverified rows",
);

const pageSource = await readFile("src/features/pricing/PricingPage.tsx", "utf8");
assert.ok(
  pageSource.includes("buildPricingComparisonViewModel"),
  "PricingPage should delegate comparison construction to the helper",
);
assert.ok(
  !pageSource.includes("buildGroupRateOnlyPricingRules"),
  "PricingPage should not use wildcard fallback pricing rules",
);
assert.ok(
  !pageSource.includes("buildPriceMatrix("),
  "PricingPage should not render the old model-by-station matrix",
);

function station(id, name, creditPerCny) {
  return {
    id,
    name,
    stationType: "sub2api",
    baseUrl: `https://${id}.example.test`,
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

function group(stationId, id, groupName, multiplier, rawText, updatedAt) {
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
    rateSource: "sub2api_groups_rates",
    confidence: 0.9,
    lastSeenAt: updatedAt,
    lastCheckedAt: updatedAt,
    lastRateChangedAt: updatedAt,
    rawJsonRedacted: { label: rawText },
    createdAt: updatedAt,
    updatedAt,
  };
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```powershell
node scripts/pricing-comparison-view-model.test.mjs
```

Expected: FAIL with module-not-found for `src/features/pricing/officialModelCatalog.ts` or `src/features/pricing/pricingComparisonViewModel.ts`.

- [ ] **Step 3: Commit the failing test**

Run:

```powershell
git add -- scripts/pricing-comparison-view-model.test.mjs
git commit -m "test: cover pricing comparison model sections"
```

Expected: commit succeeds with only `scripts/pricing-comparison-view-model.test.mjs` staged.

## Task 2: Add Official Model Catalog

**Files:**
- Create: `src/features/pricing/officialModelCatalog.ts`
- Test: `scripts/pricing-comparison-view-model.test.mjs`

- [ ] **Step 1: Create the catalog module**

Create `src/features/pricing/officialModelCatalog.ts` with this complete content:

```ts
export type OfficialModelProvider = "openai" | "anthropic" | "google";

export type OfficialModelCatalogEntry = {
  provider: OfficialModelProvider;
  modelId: string;
  displayName: string;
  officialInputPrice: number;
  officialOutputPrice: number;
  currency: "USD";
  unit: "per_1m_tokens";
  aliases: string[];
  groupMatchers: string[];
  enabledByDefault: boolean;
  priceSourceUrl: string;
  priceSourceLabel: string;
};

const openAiPriceSource = "https://developers.openai.com/api/docs/pricing";
const anthropicPriceSource = "https://platform.claude.com/docs/en/about-claude/pricing";
const geminiPriceSource = "https://ai.google.dev/gemini-api/docs/pricing";

export const officialModelCatalog: OfficialModelCatalogEntry[] = [
  {
    provider: "openai",
    modelId: "gpt-5.3-codex",
    displayName: "GPT-5.3 Codex",
    officialInputPrice: 1.75,
    officialOutputPrice: 14,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: ["gpt-5.3-codex", "codex"],
    groupMatchers: ["openai", "gpt", "codex", "default", "green"],
    enabledByDefault: true,
    priceSourceUrl: openAiPriceSource,
    priceSourceLabel: "OpenAI API pricing, standard Codex rate",
  },
  {
    provider: "openai",
    modelId: "gpt-5-mini",
    displayName: "GPT-5 mini",
    officialInputPrice: 0.25,
    officialOutputPrice: 2,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: ["gpt-5-mini"],
    groupMatchers: ["openai", "gpt", "default", "green"],
    enabledByDefault: true,
    priceSourceUrl: "https://developers.openai.com/api/docs/models/gpt-5-mini",
    priceSourceLabel: "OpenAI GPT-5 mini model pricing",
  },
  {
    provider: "anthropic",
    modelId: "claude-sonnet-5",
    displayName: "Claude Sonnet 5",
    officialInputPrice: 2,
    officialOutputPrice: 10,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: ["claude-sonnet-5", "sonnet-5"],
    groupMatchers: ["anthropic", "claude", "sonnet", "yellow", "amber"],
    enabledByDefault: true,
    priceSourceUrl: anthropicPriceSource,
    priceSourceLabel: "Anthropic Sonnet introductory pricing through 2026-08-31",
  },
  {
    provider: "google",
    modelId: "gemini-3.5-flash",
    displayName: "Gemini 3.5 Flash",
    officialInputPrice: 1.5,
    officialOutputPrice: 9,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: ["gemini-3.5-flash"],
    groupMatchers: ["google", "gemini", "flash"],
    enabledByDefault: true,
    priceSourceUrl: geminiPriceSource,
    priceSourceLabel: "Gemini Developer API pricing, paid tier",
  },
  {
    provider: "google",
    modelId: "gemini-3.1-flash-lite",
    displayName: "Gemini 3.1 Flash-Lite",
    officialInputPrice: 0.25,
    officialOutputPrice: 1.5,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: ["gemini-3.1-flash-lite", "gemini-flash-lite"],
    groupMatchers: ["google", "gemini", "flash-lite", "flash_lite"],
    enabledByDefault: true,
    priceSourceUrl: geminiPriceSource,
    priceSourceLabel: "Gemini Developer API pricing, paid tier",
  },
];

export function enabledOfficialModelCatalog() {
  return officialModelCatalog.filter((model) => model.enabledByDefault);
}

export function normalizeCatalogText(value: string) {
  return value.trim().toLowerCase().replace(/[_\s]+/g, "-");
}
```

- [ ] **Step 2: Run test to verify it still fails for the view model**

Run:

```powershell
node scripts/pricing-comparison-view-model.test.mjs
```

Expected: FAIL with module-not-found for `src/features/pricing/pricingComparisonViewModel.ts`.

- [ ] **Step 3: Commit the catalog**

Run:

```powershell
git add -- src/features/pricing/officialModelCatalog.ts
git commit -m "feat: add official pricing model catalog"
```

Expected: commit succeeds with only `src/features/pricing/officialModelCatalog.ts` staged.

## Task 3: Add Comparison View Model

**Files:**
- Create: `src/features/pricing/pricingComparisonViewModel.ts`
- Test: `scripts/pricing-comparison-view-model.test.mjs`

- [ ] **Step 1: Create the comparison view model**

Create `src/features/pricing/pricingComparisonViewModel.ts` with this complete content:

```ts
import type { PricingRule } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { Station } from "@/lib/types/stations";
import type { OfficialModelCatalogEntry, OfficialModelProvider } from "./officialModelCatalog";

export type PricingEvidenceStatus = "discovered" | "unverified" | "unavailable";

export type PricingModelEvidence = {
  stationId: string;
  modelId: string;
  status: PricingEvidenceStatus;
};

export type PricingComparisonFilters = {
  provider: OfficialModelProvider | "all";
  modelQuery: string;
  stationId: string | "all";
  verifiedOnly: boolean;
};

export type PricingComparisonInput = {
  models: OfficialModelCatalogEntry[];
  stations: Station[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  pricingRules: PricingRule[];
  modelEvidence?: PricingModelEvidence[];
  filters: PricingComparisonFilters;
};

export type PricingComparisonRow = {
  stationId: string;
  stationName: string;
  groupBindingId: string | null;
  groupName: string;
  groupMultiplier: number | null;
  creditPerCny: number;
  effectiveMultiplier: number | null;
  estimatedInputCny: number | null;
  estimatedOutputCny: number | null;
  evidenceStatus: PricingEvidenceStatus;
  evidenceLabel: string;
  source: string;
  updatedAt: string;
  isCheapest: boolean;
};

export type PricingModelSection = {
  modelId: string;
  displayName: string;
  provider: OfficialModelProvider;
  officialInputPrice: number;
  officialOutputPrice: number;
  priceSourceUrl: string;
  priceSourceLabel: string;
  rows: PricingComparisonRow[];
};

export type PricingComparisonMetrics = {
  coveredModelCount: number;
  comparableGroupCount: number;
  lowestEffectiveMultiplier: number | null;
  lowestEffectiveMultiplierLabel: string | null;
};

export type PricingComparisonViewModel = {
  sections: PricingModelSection[];
  metrics: PricingComparisonMetrics;
  emptyReason: "no_catalog_models" | "no_group_rates" | "filtered_empty" | null;
};

export function buildPricingComparisonViewModel(input: PricingComparisonInput): PricingComparisonViewModel {
  const enabledModels = input.models
    .filter((model) => model.enabledByDefault)
    .filter((model) => input.filters.provider === "all" || model.provider === input.filters.provider)
    .filter((model) => modelMatchesQuery(model, input.filters.modelQuery))
    .sort((a, b) => a.displayName.localeCompare(b.displayName));
  const stationById = new Map(input.stations.map((station) => [station.id, station] as const));
  const latestRateByBindingId = latestGroupRatesByBinding(input.groupRates);
  const evidenceByStationModel = new Map(
    (input.modelEvidence ?? []).map((item) => [`${item.stationId}:${normalizeCatalogText(item.modelId)}`, item.status] as const),
  );

  const sections = enabledModels
    .map((model) => {
      const rows = input.groupBindings
        .filter((binding) => binding.bindingKind === "station_group")
        .filter((binding) => binding.bindingStatus !== "disabled" && binding.bindingStatus !== "manual_legacy")
        .filter((binding) => input.filters.stationId === "all" || binding.stationId === input.filters.stationId)
        .filter((binding) => groupMatchesModel(binding, model))
        .map((binding) => rowFromBinding(model, binding, stationById, latestRateByBindingId, evidenceByStationModel))
        .filter((row): row is PricingComparisonRow => row !== null)
        .filter((row) => !input.filters.verifiedOnly || row.evidenceStatus === "discovered")
        .sort(compareRowsByOutputPrice);
      markCheapest(rows);
      return {
        modelId: model.modelId,
        displayName: model.displayName,
        provider: model.provider,
        officialInputPrice: model.officialInputPrice,
        officialOutputPrice: model.officialOutputPrice,
        priceSourceUrl: model.priceSourceUrl,
        priceSourceLabel: model.priceSourceLabel,
        rows,
      };
    })
    .filter((section) => section.rows.length > 0);

  return {
    sections,
    metrics: metricsFromSections(sections),
    emptyReason: emptyReason(enabledModels, input.groupBindings, sections),
  };
}

function modelMatchesQuery(model: OfficialModelCatalogEntry, query: string) {
  const normalizedQuery = normalizeCatalogText(query);
  if (!normalizedQuery) {
    return true;
  }
  return [model.modelId, model.displayName, ...model.aliases]
    .map(normalizeCatalogText)
    .some((value) => value.includes(normalizedQuery));
}

function groupMatchesModel(binding: StationGroupBinding, model: OfficialModelCatalogEntry) {
  const haystack = normalizeCatalogText(
    [binding.groupName, binding.groupIdHash ?? "", rawText(binding.rawJsonRedacted)].join(" "),
  );
  return model.groupMatchers.some((matcher) => haystack.includes(normalizeCatalogText(matcher)));
}

function rowFromBinding(
  model: OfficialModelCatalogEntry,
  binding: StationGroupBinding,
  stationById: Map<string, Station>,
  latestRateByBindingId: Map<string, GroupRateRecord>,
  evidenceByStationModel: Map<string, PricingEvidenceStatus>,
): PricingComparisonRow | null {
  const station = stationById.get(binding.stationId);
  if (!station) {
    return null;
  }
  const rate = latestRateByBindingId.get(binding.id);
  const groupMultiplier = binding.effectiveRateMultiplier ?? rate?.effectiveRateMultiplier ?? null;
  if (groupMultiplier == null) {
    return null;
  }
  const creditPerCny = station.creditPerCny > 0 && Number.isFinite(station.creditPerCny) ? station.creditPerCny : 1;
  const effectiveMultiplier = roundPrice(groupMultiplier / creditPerCny);
  const evidenceStatus = evidenceByStationModel.get(`${station.id}:${normalizeCatalogText(model.modelId)}`) ?? "unverified";
  return {
    stationId: station.id,
    stationName: station.name,
    groupBindingId: binding.id,
    groupName: binding.groupName,
    groupMultiplier,
    creditPerCny,
    effectiveMultiplier,
    estimatedInputCny: roundPrice(model.officialInputPrice * effectiveMultiplier),
    estimatedOutputCny: roundPrice(model.officialOutputPrice * effectiveMultiplier),
    evidenceStatus,
    evidenceLabel: evidenceLabel(evidenceStatus),
    source: binding.rateSource ?? rate?.source ?? "station_group_binding",
    updatedAt: binding.lastCheckedAt ?? rate?.checkedAt ?? binding.updatedAt,
    isCheapest: false,
  };
}

function latestGroupRatesByBinding(rates: GroupRateRecord[]) {
  const latest = new Map<string, GroupRateRecord>();
  for (const rate of rates) {
    if (!rate.groupBindingId || rate.bindingKind !== "station_group") {
      continue;
    }
    const previous = latest.get(rate.groupBindingId);
    if (!previous || toTime(rate.checkedAt) > toTime(previous.checkedAt)) {
      latest.set(rate.groupBindingId, rate);
    }
  }
  return latest;
}

function compareRowsByOutputPrice(left: PricingComparisonRow, right: PricingComparisonRow) {
  const leftPrice = left.estimatedOutputCny ?? Number.POSITIVE_INFINITY;
  const rightPrice = right.estimatedOutputCny ?? Number.POSITIVE_INFINITY;
  if (leftPrice !== rightPrice) {
    return leftPrice - rightPrice;
  }
  return `${left.stationName}:${left.groupName}`.localeCompare(`${right.stationName}:${right.groupName}`);
}

function markCheapest(rows: PricingComparisonRow[]) {
  const firstPriced = rows.find((row) => row.estimatedOutputCny != null);
  if (firstPriced) {
    firstPriced.isCheapest = true;
  }
}

function metricsFromSections(sections: PricingModelSection[]): PricingComparisonMetrics {
  const rows = sections.flatMap((section) => section.rows.map((row) => ({ section, row })));
  const lowest = rows
    .filter((item) => item.row.effectiveMultiplier != null)
    .sort((a, b) => (a.row.effectiveMultiplier ?? 0) - (b.row.effectiveMultiplier ?? 0))[0];
  return {
    coveredModelCount: sections.length,
    comparableGroupCount: rows.length,
    lowestEffectiveMultiplier: lowest?.row.effectiveMultiplier ?? null,
    lowestEffectiveMultiplierLabel: lowest
      ? `${lowest.section.displayName} · ${lowest.row.stationName} · ${lowest.row.groupName}`
      : null,
  };
}

function emptyReason(
  enabledModels: OfficialModelCatalogEntry[],
  groupBindings: StationGroupBinding[],
  sections: PricingModelSection[],
): PricingComparisonViewModel["emptyReason"] {
  if (enabledModels.length === 0) {
    return "no_catalog_models";
  }
  if (!groupBindings.some((binding) => binding.bindingKind === "station_group")) {
    return "no_group_rates";
  }
  if (sections.length === 0) {
    return "filtered_empty";
  }
  return null;
}

function evidenceLabel(status: PricingEvidenceStatus) {
  if (status === "discovered") {
    return "已发现";
  }
  if (status === "unavailable") {
    return "不可用";
  }
  return "未验证";
}

function normalizeCatalogText(value: string) {
  return value.trim().toLowerCase().replace(/[_\s]+/g, "-");
}

function rawText(value: unknown): string {
  if (value == null) {
    return "";
  }
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  if (Array.isArray(value)) {
    return value.map(rawText).join(" ");
  }
  if (typeof value === "object") {
    return Object.entries(value as Record<string, unknown>)
      .flatMap(([key, item]) => [key, rawText(item)])
      .join(" ");
  }
  return "";
}

function roundPrice(value: number) {
  return Number(value.toFixed(6));
}

function toTime(value: string) {
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return Number.isNaN(date.getTime()) ? 0 : date.getTime();
}
```

- [ ] **Step 2: Run test to verify helper behavior passes except page wiring**

Run:

```powershell
node scripts/pricing-comparison-view-model.test.mjs
```

Expected: FAIL on the `PricingPage should delegate comparison construction to the helper` assertion because the page is still using `buildGroupRateOnlyPricingRules` and matrix rendering.

- [ ] **Step 3: Commit the view model**

Run:

```powershell
git add -- src/features/pricing/pricingComparisonViewModel.ts
git commit -m "feat: build pricing comparison view model"
```

Expected: commit succeeds with only `src/features/pricing/pricingComparisonViewModel.ts` staged.

## Task 4: Replace PricingPage Matrix With Model Sections

**Files:**
- Modify: `src/features/pricing/PricingPage.tsx`
- Test: `scripts/pricing-comparison-view-model.test.mjs`

- [ ] **Step 1: Update imports and state**

In `src/features/pricing/PricingPage.tsx`, replace the pricing imports at the top with these imports:

```ts
import { useEffect, useMemo, useState } from "react";
import { BadgeDollarSign, Layers3, RefreshCw, ShieldCheck, TrendingDown } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  EmptyState,
  MetricCard,
  SegmentedControl,
  SectionCard,
  SelectControl,
  StatusBadge,
  Toolbar,
  useToast,
  type StatusTone,
} from "@/components/ui";
import { listPricingRules } from "@/lib/api/economics";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import { listStations } from "@/lib/api/stations";
import type { PricingRule } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { Station } from "@/lib/types/stations";
import { enabledOfficialModelCatalog, type OfficialModelProvider } from "./officialModelCatalog";
import {
  buildPricingComparisonViewModel,
  type PricingComparisonRow,
  type PricingModelSection,
} from "./pricingComparisonViewModel";
```

Replace the component state block with this state:

```ts
const toast = useToast();
const [pricingRules, setPricingRules] = useState<PricingRule[]>([]);
const [stations, setStations] = useState<Station[]>([]);
const [groupBindings, setGroupBindings] = useState<StationGroupBinding[]>([]);
const [groupRates, setGroupRates] = useState<GroupRateRecord[]>([]);
const [loading, setLoading] = useState(true);
const [error, setError] = useState<string | null>(null);
const [providerFilter, setProviderFilter] = useState<OfficialModelProvider | "all">("all");
const [modelQuery, setModelQuery] = useState("");
const [selectedStationId, setSelectedStationId] = useState<string>("all");
const [verifiedOnly, setVerifiedOnly] = useState(false);
```

- [ ] **Step 2: Update refresh data loading**

Replace the existing `refresh` function body with this implementation:

```ts
async function refresh(showSuccess = false) {
  setLoading(true);
  setError(null);
  try {
    const [nextPricing, nextStations] = await Promise.all([listPricingRules(), listStations()]);
    const [bindingLists, rateRecordLists] = await Promise.all([
      Promise.all(nextStations.map((station) => listStationGroupBindings(station.id))),
      Promise.all(nextStations.map((station) => listGroupRateRecords(station.id))),
    ]);
    setPricingRules(nextPricing);
    setStations(nextStations);
    setGroupBindings(bindingLists.flat());
    setGroupRates(rateRecordLists.flat());
    if (showSuccess) {
      toast.success("价格表已刷新");
    }
  } catch (requestError) {
    const message = readError(requestError);
    setError(message);
    toast.error("刷新价格表失败", message);
  } finally {
    setLoading(false);
  }
}
```

- [ ] **Step 3: Add view model memo**

Below the `visibleStations` equivalent area, add this memo and remove `comparisonRows`, `filteredRows`, `filteredRateRows`, `sourceOptions`, `selected`, `cheapest`, `modelGroups`, `priceMatrix`, `rateMatrix`, and `hasCurrentData`:

```ts
const viewModel = useMemo(
  () =>
    buildPricingComparisonViewModel({
      models: enabledOfficialModelCatalog(),
      stations,
      groupBindings,
      groupRates,
      pricingRules,
      modelEvidence: [],
      filters: {
        provider: providerFilter,
        modelQuery,
        stationId: selectedStationId,
        verifiedOnly,
      },
    }),
  [groupBindings, groupRates, modelQuery, pricingRules, providerFilter, selectedStationId, stations, verifiedOnly],
);
```

- [ ] **Step 4: Replace the JSX content**

Replace the JSX inside `<PageScaffold>` with this structure:

```tsx
<div className="grid gap-[var(--shell-page-gap)] md:grid-cols-3">
  <MetricCard
    icon={Layers3}
    label="覆盖模型"
    value={`${viewModel.metrics.coveredModelCount}`}
    detail="默认模型目录中的可比价模型"
  />
  <MetricCard
    icon={ShieldCheck}
    label="可比价分组"
    value={`${viewModel.metrics.comparableGroupCount}`}
    detail="站点与分组组合"
  />
  <MetricCard
    icon={TrendingDown}
    label="最低折算倍率"
    value={viewModel.metrics.lowestEffectiveMultiplier == null ? "暂无" : formatMultiplier(viewModel.metrics.lowestEffectiveMultiplier)}
    detail={viewModel.metrics.lowestEffectiveMultiplierLabel ?? "尚无可比价分组"}
  />
</div>

<SectionCard title="模型比价" contentClassName="p-0">
  <Toolbar>
    <div className="flex flex-wrap items-center gap-2">
      <SegmentedControl
        value={providerFilter}
        options={[
          { value: "all", label: "全部" },
          { value: "openai", label: "OpenAI" },
          { value: "anthropic", label: "Anthropic" },
          { value: "google", label: "Google" },
        ]}
        onChange={(value) => setProviderFilter(value as OfficialModelProvider | "all")}
      />
      <label className="sr-only" htmlFor="pricing-model-search">搜索模型</label>
      <input
        id="pricing-model-search"
        className={inputClassName}
        value={modelQuery}
        onChange={(event) => setModelQuery(event.target.value)}
        placeholder="搜索模型"
      />
      <SelectControl
        ariaLabel="按中转站筛选模型比价"
        className={inputClassName}
        value={selectedStationId}
        options={[
          { value: "all", label: "全部中转站" },
          ...stations.map((station) => ({ value: station.id, label: station.name })),
        ]}
        onChange={setSelectedStationId}
      />
      <label className="inline-flex h-8 cursor-pointer items-center gap-2 rounded-[12px] border border-border bg-white px-3 text-sm text-slate-700 transition-colors hover:bg-slate-50">
        <input
          type="checkbox"
          className="h-4 w-4 rounded border-slate-300 text-cyan-600 focus:ring-cyan-200"
          checked={verifiedOnly}
          onChange={(event) => setVerifiedOnly(event.target.checked)}
        />
        只看已验证可用
      </label>
    </div>
  </Toolbar>

  {error && <div role="alert" className="border-b border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">{error}</div>}
  {loading ? (
    <div className="px-4 py-5 text-sm text-muted-foreground">正在读取价格与倍率...</div>
  ) : viewModel.sections.length === 0 ? (
    <PricingEmptyState reason={viewModel.emptyReason} />
  ) : (
    <div className="divide-y divide-border">
      {viewModel.sections.map((section) => (
        <ModelPricingSection key={section.modelId} section={section} />
      ))}
    </div>
  )}
</SectionCard>
```

- [ ] **Step 5: Add section/table components and formatters**

In the same file, replace old helper components from `PriceCellView` through `AvailabilityCellView` and `MatrixTable` with this code:

```tsx
function ModelPricingSection({ section }: { section: PricingModelSection }) {
  return (
    <section className="grid gap-3 px-4 py-4">
      <div className="flex flex-wrap items-end justify-between gap-2">
        <div>
          <h2 className="text-base font-semibold text-slate-950">{section.displayName}</h2>
          <p className="mt-1 text-xs text-muted-foreground">
            官方价：输入 {formatUsd(section.officialInputPrice)} / 输出 {formatUsd(section.officialOutputPrice)} per 1M tokens
          </p>
        </div>
        <a
          className="text-xs font-medium text-cyan-700 hover:text-cyan-800"
          href={section.priceSourceUrl}
          target="_blank"
          rel="noreferrer"
        >
          {section.priceSourceLabel}
        </a>
      </div>
      {section.rows.length === 0 ? (
        <EmptyState title="暂无可比价分组" description="该模型暂未匹配到任何站点分组倍率。" />
      ) : (
        <PricingRowsTable rows={section.rows} />
      )}
    </section>
  );
}

function PricingRowsTable({ rows }: { rows: PricingComparisonRow[] }) {
  return (
    <div className="overflow-x-auto rounded-[var(--surface-radius)] border border-border">
      <table className="min-w-[940px] w-full border-collapse text-sm">
        <thead className="bg-slate-50 text-xs font-semibold text-muted-foreground">
          <tr>
            <th className={thClassName}>站点</th>
            <th className={thClassName}>分组</th>
            <th className={thClassNameRight}>分组倍率</th>
            <th className={thClassNameRight}>充值倍率</th>
            <th className={thClassNameRight}>折算倍率</th>
            <th className={thClassNameRight}>输入价</th>
            <th className={thClassNameRight}>输出价</th>
            <th className={thClassName}>证据</th>
            <th className={thClassName}>更新时间</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border bg-white">
          {rows.map((row) => (
            <tr key={`${row.stationId}-${row.groupBindingId ?? row.groupName}`} className={row.isCheapest ? "bg-emerald-50/65" : "hover:bg-slate-50"}>
              <td className={tdClassName}>
                <div className="font-medium text-slate-900">{row.stationName}</div>
              </td>
              <td className={tdClassName}>{row.groupName}</td>
              <td className={tdClassNameRight}>{formatNullableMultiplier(row.groupMultiplier)}</td>
              <td className={tdClassNameRight}>{formatRecharge(row.creditPerCny)}</td>
              <td className={`${tdClassNameRight} font-semibold text-slate-950`}>{formatNullableMultiplier(row.effectiveMultiplier)}</td>
              <td className={tdClassNameRight}>{formatCny(row.estimatedInputCny)}</td>
              <td className={`${tdClassNameRight} font-semibold text-slate-950`}>{formatCny(row.estimatedOutputCny)}</td>
              <td className={tdClassName}>
                <StatusBadge tone={evidenceTone(row.evidenceStatus)}>{row.evidenceLabel}</StatusBadge>
              </td>
              <td className={tdClassName}>{formatTime(row.updatedAt)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function PricingEmptyState({ reason }: { reason: ReturnType<typeof buildPricingComparisonViewModel>["emptyReason"] }) {
  if (reason === "no_catalog_models") {
    return <EmptyState title="未配置默认模型目录" description="需要先启用至少一个官方模型目录条目。" />;
  }
  if (reason === "no_group_rates") {
    return <EmptyState title="尚未采集分组倍率" description="先在中转站资产中同步 Sub2API 或 NewAPI 分组倍率。" />;
  }
  return <EmptyState title="当前筛选无结果" description="调整模型、站点或已验证筛选后再查看。" />;
}
```

Then replace helper functions below with these helpers:

```ts
function evidenceTone(status: PricingComparisonRow["evidenceStatus"]): StatusTone {
  if (status === "discovered") {
    return "healthy";
  }
  if (status === "unavailable") {
    return "danger";
  }
  return "info";
}

function formatNullableMultiplier(value: number | null) {
  return value == null ? "未知" : formatMultiplier(value);
}

function formatMultiplier(value: number) {
  return `${value.toFixed(4).replace(/0+$/, "").replace(/\.$/, "")}x`;
}

function formatRecharge(value: number) {
  return `${value.toFixed(4).replace(/0+$/, "").replace(/\.$/, "")} 额度/元`;
}

function formatCny(value: number | null) {
  return value == null ? "暂无" : `¥${value.toFixed(4)}`;
}

function formatUsd(value: number) {
  return `$${value.toFixed(4).replace(/0+$/, "").replace(/\.$/, "")}`;
}

function formatTime(value: string) {
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value || "未知";
  }
  return date.toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

const inputClassName =
  "h-8 rounded-[12px] border border-cyan-100 bg-cyan-50/45 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";
const thClassName = "whitespace-nowrap px-3 py-2 text-left";
const thClassNameRight = "whitespace-nowrap px-3 py-2 text-right";
const tdClassName = "whitespace-nowrap px-3 py-2.5 align-middle text-slate-700";
const tdClassNameRight = `${tdClassName} text-right`;
```

- [ ] **Step 6: Run test to verify page source and behavior pass**

Run:

```powershell
node scripts/pricing-comparison-view-model.test.mjs
```

Expected: PASS with exit code 0.

- [ ] **Step 7: Commit the page replacement**

Run:

```powershell
git add -- src/features/pricing/PricingPage.tsx
git commit -m "feat: render pricing by concrete model sections"
```

Expected: commit succeeds with only `src/features/pricing/PricingPage.tsx` staged.

## Task 5: Remove Interim Wildcard Helpers

**Files:**
- Modify: `src/features/pricing/pricingMatrix.ts`
- Modify: `src/features/pricing/rateSnapshotParser.ts`
- Test: `scripts/pricing-comparison-view-model.test.mjs`

- [ ] **Step 1: Remove wildcard pricing-rule generation**

In `src/features/pricing/pricingMatrix.ts`, delete the entire `buildGroupRateOnlyPricingRules` export and delete these private helpers:

```ts
function newestRatesByStationGroup(rates: RateMultiplierRow[]) {
  const latest = new Map<string, RateMultiplierRow>();
  for (const rate of rates) {
    const key = `${rate.stationId}:${rate.groupBindingId ?? rate.groupName}`;
    const previous = latest.get(key);
    if (!previous || toTime(rate.updatedAt) > toTime(previous.updatedAt)) {
      latest.set(key, rate);
    }
  }
  return Array.from(latest.values());
}

function matchingModels(modelNames: string[], prefixes: string[], fallbacks: string[]) {
  const matches = modelNames.filter((model) =>
    prefixes.some((prefix) => model.toLowerCase().startsWith(prefix.toLowerCase())),
  );
  return matches.length > 0 ? matches : fallbacks;
}

function fallbackModelsForProvider(provider: string | null) {
  if (provider === "openai") {
    return ["gpt-*"];
  }
  if (provider === "anthropic") {
    return ["claude-*"];
  }
  return [];
}

function hasRuleForStationModel(rules: PricingRule[], stationId: string, model: string) {
  return rules.some((rule) => rule.enabled && rule.stationId === stationId && rule.model === model);
}
```

If `RateMultiplierRow` is no longer imported by `pricingMatrix.ts`, remove that import.

- [ ] **Step 2: Remove interim model-family fields from rate rows**

In `src/features/pricing/rateSnapshotParser.ts`, replace the `RateMultiplierRow` type with:

```ts
export type RateMultiplierRow = {
  stationId: string;
  groupBindingId: string | null;
  groupName: string;
  multiplier: number | null;
  source: string;
  status: string;
  confidence: number;
  updatedAt: string;
};
```

Replace `rateRowsFromGroupFacts` with:

```ts
export function rateRowsFromGroupFacts(
  bindings: StationGroupBinding[],
  records: GroupRateRecord[],
): RateMultiplierRow[] {
  const bindingRows = bindings
    .filter((binding) => binding.bindingKind === "station_group")
    .map((binding) => ({
      stationId: binding.stationId,
      groupBindingId: binding.id,
      groupName: binding.groupName,
      multiplier: binding.effectiveRateMultiplier,
      source: binding.rateSource ?? "binding",
      status: binding.bindingStatus,
      confidence: binding.confidence,
      updatedAt: binding.lastCheckedAt ?? binding.updatedAt,
    }));

  const recordRows = records.map((record) => ({
    stationId: record.stationId,
    groupBindingId: record.groupBindingId,
    groupName: record.groupName,
    multiplier: record.effectiveRateMultiplier,
    source: record.source,
    status: "history",
    confidence: record.confidence,
    updatedAt: record.checkedAt,
  }));

  return [...bindingRows, ...recordRows];
}
```

Delete the private `ModelCoverage`, `providerCoverage`, `inferModelCoverage`, and `collectRawText` code from `rateSnapshotParser.ts`.

- [ ] **Step 3: Run wildcard grep and focused test**

Run:

```powershell
rg -n "gpt-\\*|claude-\\*|gemini-\\*|buildGroupRateOnlyPricingRules|modelPrefixes|modelFamilies" src/features/pricing scripts/pricing-comparison-view-model.test.mjs
node scripts/pricing-comparison-view-model.test.mjs
```

Expected:

- `rg` exits 1 with no matches.
- `node scripts/pricing-comparison-view-model.test.mjs` exits 0.

- [ ] **Step 4: Commit wildcard cleanup**

Run:

```powershell
git add -- src/features/pricing/pricingMatrix.ts src/features/pricing/rateSnapshotParser.ts
git commit -m "refactor: remove wildcard pricing fallbacks"
```

Expected: commit succeeds with only the two pricing helper files staged.

## Task 6: Build Verification

**Files:**
- Verify: `scripts/pricing-comparison-view-model.test.mjs`
- Verify: `src/features/pricing/PricingPage.tsx`
- Verify: `src/features/pricing/officialModelCatalog.ts`
- Verify: `src/features/pricing/pricingComparisonViewModel.ts`

- [ ] **Step 1: Run focused test**

Run:

```powershell
node scripts/pricing-comparison-view-model.test.mjs
```

Expected: PASS with exit code 0.

- [ ] **Step 2: Run frontend build**

Run:

```powershell
pnpm.cmd build
```

Expected: `tsc --noEmit` passes and `vite build` exits 0. A Vite chunk-size warning is acceptable if it is the only warning.

- [ ] **Step 3: Check changed paths**

Run:

```powershell
git status --short -- scripts/pricing-comparison-view-model.test.mjs src/features/pricing/officialModelCatalog.ts src/features/pricing/pricingComparisonViewModel.ts src/features/pricing/PricingPage.tsx src/features/pricing/pricingMatrix.ts src/features/pricing/rateSnapshotParser.ts
```

Expected: no output if all task commits were made, or only the exact listed paths if the executor chose to defer commits.

## Self-Review Checklist

- Spec coverage:
  - Concrete model sections: Task 3 and Task 4.
  - No wildcard rows: Task 1, Task 5.
  - Multiple groups per station/model: Task 1 and Task 3.
  - `creditPerCny` in price math: Task 1 and Task 3.
  - Input/output prices and multipliers visible: Task 4.
  - Section-local sorting by output price: Task 1 and Task 3.
  - Compact accessible filters: Task 4.
  - Data shaping out of `PricingPage.tsx`: Task 1 and Task 4.
- Placeholder scan:
  - No `TBD`, `TODO`, `FIXME`, or angle-bracket path placeholders are used.
- Type consistency:
  - `OfficialModelProvider`, `OfficialModelCatalogEntry`, `PricingComparisonRow`, and `PricingModelSection` are defined before use.
  - `PricingPage.tsx` imports the same helper names defined by `pricingComparisonViewModel.ts`.
