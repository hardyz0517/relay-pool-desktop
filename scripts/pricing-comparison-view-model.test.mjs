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
  officialModelCatalog.every((model) => !/\*$/.test(model.modelId) && !/\*$/.test(model.displayName)),
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

const historicalRateView = buildPricingComparisonViewModel({
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
  ],
  stations: [station("station-history", "History Hub", 10)],
  groupBindings: [
    group(
      "station-history",
      "group-history-default",
      "default",
      0.8,
      "OpenAI green default",
      "2026-07-06T01:00:00.000Z",
    ),
  ],
  groupRates: [
    rate(
      "station-history",
      "rate-history-newer",
      "group-history-default",
      "default",
      0.8,
      "OpenAI green default",
      "2026-07-06T03:00:00.000Z",
    ),
    rate(
      "station-history",
      "rate-history-older",
      "group-history-default",
      "default",
      0.05,
      "OpenAI green default",
      "2026-07-06T02:00:00.000Z",
    ),
  ],
  pricingRules,
  modelEvidence: [],
  filters: {
    provider: "all",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: false,
  },
});
const historicalGpt = historicalRateView.sections.find((section) => section.modelId === "gpt-5-mini");
assert.ok(historicalGpt, "historical GPT section should exist");
assert.deepEqual(
  historicalGpt.rows.map((row) => ({
    groupRateRecordId: row.groupRateRecordId,
    groupMultiplier: row.groupMultiplier,
    isCheapest: row.isCheapest,
  })),
  [{ groupRateRecordId: "rate-history-newer", groupMultiplier: 0.8, isCheapest: true }],
  "bound groups should consume all related historical rates and keep the latest checkedAt row",
);

const stationNameLeakView = buildPricingComparisonViewModel({
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
  ],
  stations: [station("station-openai-name", "OpenAI Hub", 1)],
  groupBindings: [
    group(
      "station-openai-name",
      "group-openai-name-claude",
      "claude",
      1,
      "yellow",
      "2026-07-06T01:00:00.000Z",
    ),
  ],
  groupRates,
  pricingRules,
  modelEvidence: [],
  filters: {
    provider: "all",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: false,
  },
});
const stationNameLeakGpt = stationNameLeakView.sections.find((section) => section.modelId === "gpt-5-mini");
const stationNameLeakClaude = stationNameLeakView.sections.find((section) => section.modelId === "claude-sonnet-5");
assert.ok(stationNameLeakGpt, "station-name leak GPT section should exist");
assert.ok(stationNameLeakClaude, "station-name leak Claude section should exist");
assert.equal(
  stationNameLeakGpt.rows.length,
  0,
  "OpenAI in the station name must not make a Claude-only group appear in the GPT section",
);
assert.equal(
  stationNameLeakClaude.rows.length,
  1,
  "Claude/yellow group facts should still match the Claude section",
);

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

function rate(stationId, id, groupBindingId, groupName, multiplier, rawText, checkedAt) {
  return {
    id,
    stationId,
    stationKeyId: null,
    groupBindingId,
    bindingKind: "station_group",
    groupKeyHash: `${stationId}-${groupBindingId}`,
    groupName,
    defaultRateMultiplier: multiplier,
    userRateMultiplier: null,
    effectiveRateMultiplier: multiplier,
    source: "sub2api_groups_rates",
    confidence: 0.9,
    rawJsonRedacted: { label: rawText },
    checkedAt,
    createdAt: checkedAt,
  };
}
