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
  group("station-a", "gpt-pro", "gpt_pro", 0.5, "OpenAI green pro", "2026-07-06T02:00:00.000Z"),
  group("station-b", "group-b-claude", "claude", 0.1, "Anthropic yellow", "2026-07-06T03:00:00.000Z"),
  group("station-b", "group-b-gemini", "gemini", 0.2, "Google Gemini", "2026-07-06T04:00:00.000Z"),
  group("station-c", "group-c-misc", "杂色", 1, "misc", "2026-07-06T05:00:00.000Z"),
];

const groupRates = [];
const pricingRules = [];
const stationKeys = [
  stationKey("station-a", "key-a-special", "生产 Key"),
];
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
  stationKeys,
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

const searchModels = view.sections.map((section) => ({
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
}));

const stationNameSearch = buildPricingComparisonViewModel({
  models: searchModels,
  stations,
  stationKeys,
  groupBindings,
  groupRates,
  pricingRules,
  modelEvidence,
  filters: {
    provider: "all",
    modelQuery: "FYLinkApi",
    stationId: "all",
    verifiedOnly: false,
  },
});
assert.deepEqual(
  stationNameSearch.sections.flatMap((section) => section.rows.map((row) => row.stationName)),
  ["FYLinkApi", "FYLinkApi"],
  "search should match relay station names even when the model name does not match",
);

const groupNameSearch = buildPricingComparisonViewModel({
  models: searchModels,
  stations,
  stationKeys,
  groupBindings,
  groupRates,
  pricingRules,
  modelEvidence,
  filters: {
    provider: "all",
    modelQuery: "gpt_pro",
    stationId: "all",
    verifiedOnly: false,
  },
});
assert.deepEqual(
  groupNameSearch.sections.flatMap((section) => section.rows.map((row) => row.groupName)),
  ["gpt_pro"],
  "search should match group names without relying on the model name",
);

const keyNameSearch = buildPricingComparisonViewModel({
  models: searchModels,
  stations,
  stationKeys,
  groupBindings: [
    group(
      "station-a",
      "key-bound-gpt",
      "key_gpt",
      0.4,
      "OpenAI green key",
      "2026-07-06T06:00:00.000Z",
      "key-a-special",
    ),
  ],
  groupRates,
  pricingRules,
  modelEvidence,
  filters: {
    provider: "all",
    modelQuery: "生产",
    stationId: "all",
    verifiedOnly: false,
  },
});
assert.deepEqual(
  keyNameSearch.sections.flatMap((section) =>
    section.rows.map((row) => ({ groupName: row.groupName, stationKeyName: row.stationKeyName })),
  ),
  [{ groupName: "key_gpt", stationKeyName: "生产 Key" }],
  "search should match station key names carried by group bindings",
);

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
      "claude",
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

const neutralDiscountGroupView = buildPricingComparisonViewModel({
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
  stations: [station("station-neutral-discount", "FYLinkApi", 1)],
  groupBindings: [
    group(
      "station-neutral-discount",
      "group-neutral-discount",
      "特惠分组",
      0.001,
      "特惠分组",
      "2026-07-06T01:00:00.000Z",
    ),
  ],
  groupRates: [],
  pricingRules,
  modelEvidence: [],
  filters: {
    provider: "openai",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: false,
  },
});
const neutralDiscountGpt = neutralDiscountGroupView.sections.find((section) => section.modelId === "gpt-5-mini");
assert.ok(neutralDiscountGpt, "neutral discount GPT section should exist");
assert.deepEqual(
  neutralDiscountGpt.rows.map((row) => ({ groupName: row.groupName, effectiveMultiplier: row.effectiveMultiplier })),
  [],
  "group names such as 特惠分组 must not classify a group into GPT pricing without a structured group type",
);
assert.equal(
  neutralDiscountGroupView.metrics.lowestEffectiveMultiplier,
  null,
  "name-only discount groups must not participate in the lowest effective multiplier metric",
);

const structuredDiscountGroupView = buildPricingComparisonViewModel({
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
  stations: [station("station-1783311325734-4639", "FYLinkApi", 1)],
  groupBindings: [
    group(
      "station-1783311325734-4639",
      "23",
      "特惠分组",
      0.001,
      "特惠分组",
      "2026-07-06T01:00:00.000Z",
    ),
  ],
  groupRates: [],
  pricingRules,
  modelEvidence: [],
  filters: {
    provider: "openai",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: false,
  },
});
const structuredDiscountGpt = structuredDiscountGroupView.sections.find((section) => section.modelId === "gpt-5-mini");
assert.ok(structuredDiscountGpt, "structured discount GPT section should exist");
assert.deepEqual(
  structuredDiscountGpt.rows.map((row) => ({ groupName: row.groupName, effectiveMultiplier: row.effectiveMultiplier })),
  [{ groupName: "特惠分组", effectiveMultiplier: 0.001 }],
  "FYLinkApi remote group type 23 should classify the group into GPT pricing",
);
assert.equal(
  structuredDiscountGroupView.metrics.lowestEffectiveMultiplier,
  0.001,
  "structured discount group types should participate in the lowest effective multiplier metric",
);

const imageNamedGptGroupView = buildPricingComparisonViewModel({
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
      provider: "openai",
      modelId: "gpt-image-1",
      displayName: "GPT Image 1",
      officialInputPrice: 5,
      officialOutputPrice: 40,
      currency: "USD",
      unit: "per_1m_tokens",
      aliases: ["gpt-image-1"],
      groupMatchers: ["image", "images", "图片", "图像", "绘图", "画图", "生图"],
      enabledByDefault: true,
    },
  ],
  stations: [station("station-image-gpt", "Image Hub", 1)],
  groupBindings: [
    group(
      "station-image-gpt",
      "gpt",
      "GPT画图分组",
      2,
      "gpt image group",
      "2026-07-06T01:00:00.000Z",
    ),
  ],
  groupRates: [],
  pricingRules,
  modelEvidence: [],
  filters: {
    provider: "openai",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: false,
  },
});
const imageNamedTextGpt = imageNamedGptGroupView.sections.find((section) => section.modelId === "gpt-5-mini");
const imageNamedGptImage = imageNamedGptGroupView.sections.find((section) => section.modelId === "gpt-image-1");
assert.ok(imageNamedTextGpt, "text GPT section should exist for image-name classification coverage");
assert.ok(imageNamedGptImage, "image generation section should exist for image-name classification coverage");
assert.deepEqual(
  imageNamedTextGpt.rows.map((row) => row.groupName),
  [],
  "GPT-typed groups whose names contain image wording should not be classified into text GPT pricing",
);
assert.deepEqual(
  imageNamedGptImage.rows.map((row) => row.groupName),
  ["GPT画图分组"],
  "GPT-typed groups whose names contain image wording should be classified into image generation pricing",
);

const missingGroupView = buildPricingComparisonViewModel({
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
  stations: [station("station-missing-group", "Missing Hub", 1)],
  groupBindings: [
    group(
      "station-missing-group",
      "gpt",
      "gpt_stale",
      0.03,
      "OpenAI stale group",
      "2026-07-06T01:00:00.000Z",
      null,
      "missing",
    ),
  ],
  groupRates: [],
  pricingRules,
  modelEvidence: [],
  filters: {
    provider: "openai",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: false,
  },
});
const missingGroupGpt = missingGroupView.sections.find((section) => section.modelId === "gpt-5-mini");
assert.ok(missingGroupGpt, "missing group GPT section should exist");
assert.deepEqual(
  missingGroupGpt.rows.map((row) => row.groupName),
  [],
  "missing station groups should not remain in the active pricing list",
);
assert.equal(
  missingGroupView.metrics.lowestEffectiveMultiplier,
  null,
  "missing station groups should not participate in pricing metrics",
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

const duplicateCurrentGroupView = buildPricingComparisonViewModel({
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
  stations: [station("station-current-dedup", "Current Dedup Hub", 1)],
  stationKeys: [],
  groupBindings: [
    {
      ...group(
        "station-current-dedup",
        "binding-current",
        "default",
        0.8,
        "OpenAI green default",
        "2026-07-06T01:00:00.000Z",
      ),
      groupKeyHash: "stable-local-group",
      groupIdHash: "remote-group-default",
    },
  ],
  groupRates: [
    {
      ...rate(
        "station-current-dedup",
        "rate-current",
        "binding-current",
        "default",
        0.7,
        "OpenAI green default latest",
        "2026-07-06T03:00:00.000Z",
      ),
      groupKeyHash: "stable-local-group",
    },
    {
      ...rate(
        "station-current-dedup",
        "rate-shadow",
        null,
        "default",
        0.7,
        "OpenAI green default shadow",
        "2026-07-06T03:30:00.000Z",
      ),
      groupKeyHash: "stable-local-group",
    },
  ],
  pricingRules,
  modelEvidence: [],
  filters: {
    provider: "openai",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: false,
  },
});
const duplicateCurrentGroupGpt = duplicateCurrentGroupView.sections.find(
  (section) => section.modelId === "gpt-5-mini",
);
assert.ok(duplicateCurrentGroupGpt, "duplicate-current-group GPT section should exist");
assert.deepEqual(
  duplicateCurrentGroupGpt.rows.map((row) => ({
    groupBindingId: row.groupBindingId,
    groupRateRecordId: row.groupRateRecordId,
    groupName: row.groupName,
  })),
  [{ groupBindingId: "binding-current", groupRateRecordId: "rate-current", groupName: "default" }],
  "same current group identity must not appear twice when binding and standalone rate share groupKeyHash",
);

const distinctIdentityView = buildPricingComparisonViewModel({
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
  stations: [station("station-distinct-identity", "Distinct Identity Hub", 1)],
  stationKeys: [],
  groupBindings: [
    {
      ...group(
        "station-distinct-identity",
        "binding-a",
        "default",
        0.8,
        "same remote id first local group",
        "2026-07-06T01:00:00.000Z",
      ),
      groupKeyHash: "local-group-a",
      groupIdHash: "gpt",
    },
    {
      ...group(
        "station-distinct-identity",
        "binding-b",
        "default",
        0.6,
        "same remote id second local group",
        "2026-07-06T02:00:00.000Z",
      ),
      groupKeyHash: "local-group-b",
      groupIdHash: "gpt",
    },
  ],
  groupRates: [],
  pricingRules,
  modelEvidence: [],
  filters: {
    provider: "openai",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: false,
  },
});
const distinctIdentityGpt = distinctIdentityView.sections.find(
  (section) => section.modelId === "gpt-5-mini",
);
assert.ok(distinctIdentityGpt, "distinct-identity GPT section should exist");
assert.deepEqual(
  distinctIdentityGpt.rows.map((row) => ({
    groupBindingId: row.groupBindingId,
    groupName: row.groupName,
    groupMultiplier: row.groupMultiplier,
  })),
  [
    { groupBindingId: "binding-b", groupName: "default", groupMultiplier: 0.6 },
    { groupBindingId: "binding-a", groupName: "default", groupMultiplier: 0.8 },
  ],
  "group_key_hash and group_id_hash must not be treated as interchangeable identities",
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
assert.ok(
  !pageSource.includes(">分组倍率</th>"),
  "PricingPage comparison table should hide the raw group multiplier column",
);
assert.ok(
  !pageSource.includes(">充值倍率</th>"),
  "PricingPage comparison table should hide the recharge multiplier column",
);
assert.ok(
  pageSource.includes(">倍率</th>"),
  "PricingPage comparison table should rename effective multiplier to 倍率",
);
assert.ok(
  !pageSource.includes("{row.source || \"未知来源\"}"),
  "PricingPage comparison table should not show source text under station names",
);
assert.ok(
  !pageSource.includes("bg-slate-50"),
  "PricingPage comparison table header should be a lightweight divider instead of a shaded band",
);
assert.ok(
  pageSource.includes("<colgroup>"),
  "PricingPage comparison table should define stable column widths",
);
assert.ok(
  pageSource.includes("/M"),
  "PricingPage comparison prices should display the per-million unit",
);
assert.ok(
  pageSource.includes('contentClassName="overflow-visible rounded-none border-0 bg-transparent p-0 shadow-none"'),
  "PricingPage model comparison section should not render an outer card frame",
);
assert.ok(
  pageSource.includes('<th className={tableHeaderClassName}>倍率</th>'),
  "PricingPage multiplier header should align close to the group column instead of being pushed right",
);
assert.ok(
  pageSource.includes('className={`${tableCellClassName} tabular-nums font-semibold text-slate-800`}'),
  "PricingPage multiplier cells should align close to the group column instead of being pushed right",
);
assert.ok(
  !pageSource.includes("priceSourceLabel"),
  "PricingPage model headers should not show external price source labels",
);
assert.ok(
  pageSource.includes('const tableScrollClassName = "overflow-x-auto border-y border-border"'),
  "PricingPage comparison tables should use horizontal borders only",
);
assert.ok(
  pageSource.includes('const tableClassName = "min-w-[840px] w-full table-fixed text-left text-sm"'),
  "PricingPage comparison tables should keep enough minimum width for separated price and update columns",
);
assert.ok(
  pageSource.includes("priceOutputHeaderClassName") && pageSource.includes("updatedAtHeaderClassName"),
  "PricingPage comparison table should use dedicated spacing classes for the output price and updated-at headers",
);
assert.ok(
  pageSource.includes("priceOutputCellClassName") && pageSource.includes("updatedAtCellClassName"),
  "PricingPage comparison table should use dedicated spacing classes for the output price and updated-at cells",
);
assert.ok(
  !pageSource.includes("仅已发现"),
  "PricingPage should remove the discovered-only checkbox",
);
assert.ok(
  pageSource.includes("搜索模型 / 中转站 / Key / 分组"),
  "PricingPage search placeholder should explain that station, key, and group names are searchable",
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

function group(
  stationId,
  id,
  groupName,
  multiplier,
  rawText,
  updatedAt,
  stationKeyId = null,
  bindingStatus = "available",
) {
  return {
    id,
    stationId,
    stationKeyId,
    bindingKind: "station_group",
    parentGroupBindingId: null,
    groupKeyHash: `${stationId}-${id}`,
    groupIdHash: id,
    groupName,
    bindingStatus,
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
