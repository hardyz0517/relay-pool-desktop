import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import ts from "../node_modules/typescript/lib/typescript.js";

async function importStationAssetViewModels() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-station-tags-"));
  const outputPath = join(tempRoot, "stationAssetViewModels.mjs");
  const balancePath = join(tempRoot, "balanceFacts.mjs");
  const groupPath = join(tempRoot, "groupFacts.mjs");
  let source = await readFile("src/features/stations/stationAssetViewModels.ts", "utf8");
  source = source
    .replaceAll("@/lib/projections/balanceFacts", "./balanceFacts.mjs")
    .replaceAll("@/lib/projections/groupFacts", "./groupFacts.mjs");
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      verbatimModuleSyntax: true,
    },
  }).outputText;

  await writeFile(outputPath, output, "utf8");
  await writeFile(
    balancePath,
    [
      "export function buildCurrentStationBalanceFacts({ stations }) {",
      "  return new Map(stations.map((station) => [station.id, {",
      "    value: 20,",
      "    lowBalanceThreshold: 10,",
      "    status: 'normal',",
      "    source: 'station_cache',",
      "    currency: 'CNY',",
      "    sourceSnapshot: null,",
      "  }]));",
      "}",
    ].join("\n"),
    "utf8",
  );
  await writeFile(
    groupPath,
    [
      "export function buildCurrentStationGroupFacts() { return []; }",
      "export function isDisplayableStationGroupCurrentFact() { return true; }",
    ].join("\n"),
    "utf8",
  );

  return import(`file://${outputPath.replaceAll("\\", "/")}`);
}

const {
  buildStationAssetRows,
  stationIssueTags,
  filterStationAssetRowsByIssue,
  STATION_ISSUE_FILTER_OPTIONS,
} = await importStationAssetViewModels();

assert.deepEqual(
  tagLabels(row({ riskEvents: [change({ severity: "critical" }), change({ severity: "warning" })] })),
  [],
  "change-event severity alone should not create broad risk/reminder tags",
);

assert.deepEqual(
  tagLabels(row({
    balanceFactsReady: false,
    currentBalance: { value: null, lowBalanceThreshold: null, status: null, source: "missing", currency: "CNY" },
  })),
  [],
  "balance-missing tags should wait until balance enrichment has finished to avoid return-navigation flicker",
);

assert.deepEqual(
  tagLabels(row({
    currentBalance: { value: 0, lowBalanceThreshold: 10, status: "low", currency: "CNY" },
  })),
  ["余额为零"],
  "zero balance should be a concrete balance tag instead of a generic warning",
);

assert.deepEqual(
  tagLabels(row({
    currentBalance: { value: 6, lowBalanceThreshold: 10, status: "low", currency: "CNY" },
  })),
  ["余额偏低"],
  "low balance should name the specific balance problem",
);

assert.deepEqual(
  tagLabels(row({
    station: { apiKeyPresent: false, keyCount: 0 },
    currentBalance: { value: 20, lowBalanceThreshold: 10, status: "normal", currency: "CNY" },
  })),
  ["缺 API Key"],
  "missing credentials should be shown as a key/configuration problem",
);

assert.deepEqual(
  tagLabels(row({
    station: { status: "healthy", keyCount: 2 },
    enabledKeyCount: 0,
    currentBalance: { value: 20, lowBalanceThreshold: 10, status: "normal", currency: "CNY" },
  })),
  ["无可用 Key"],
  "disabled local keys should be shown as a key problem without mentioning routing",
);

assert.deepEqual(
  tagLabels(row({
    latestSnapshot: { status: "manual_required", errorMessage: null, summaryJson: { loginRequired: true } },
    currentBalance: { value: 20, lowBalanceThreshold: 10, status: "normal", currency: "CNY" },
  })),
  ["需登录"],
  "manual login collection states should be concrete",
);

assert.deepEqual(
  STATION_ISSUE_FILTER_OPTIONS.map((option) => option.value),
  [
    "all",
    "balance_zero",
    "balance_low",
    "balance_missing",
    "login_required",
    "collection_failed",
    "missing_api_key",
    "no_enabled_key",
    "key_warning",
    "group_issue",
    "missing_rate",
    "disabled",
    "not_collected",
  ],
  "station issue filter options should use stable issue kinds instead of translated labels",
);

{
  const rows = [
    row({ station: { id: "healthy", name: "Healthy" } }),
    row({
      station: { id: "zero", name: "Zero Balance" },
      currentBalance: { value: 0, lowBalanceThreshold: 10, status: "low", currency: "CNY" },
    }),
    row({
      station: { id: "login", name: "Login Required" },
      latestSnapshot: { status: "manual_required", errorMessage: null, summaryJson: { loginRequired: true } },
    }),
  ];

  assert.deepEqual(
    filterStationAssetRowsByIssue(rows, "all").map((candidate) => candidate.station.id),
    ["healthy", "zero", "login"],
    "all issue filter should preserve every station in the current order",
  );

  assert.deepEqual(
    filterStationAssetRowsByIssue(rows, "balance_zero").map((candidate) => candidate.station.id),
    ["zero"],
    "zero-balance issue filter should match rows by stable issue kind",
  );

  assert.deepEqual(
    filterStationAssetRowsByIssue(rows, "login_required").map((candidate) => candidate.station.id),
    ["login"],
    "manual-login issue filter should reuse station issue tags",
  );
}

assert.equal(
  groupIssueTagFor({
    bindings: [
      groupBinding({ id: "missing-plus", groupName: "Plus", bindingStatus: "missing" }),
      groupBinding({ id: "available-plus", groupName: " plus ", bindingStatus: "available" }),
    ],
    keys: [stationKey({ groupBindingId: "missing-plus", groupName: "Plus" })],
  }),
  undefined,
  "a current same-name group should suppress a historical missing identity",
);

assert.equal(
  groupIssueTagFor({
    bindings: [groupBinding({ groupName: "Legacy", bindingStatus: "missing" })],
    keys: [],
  }),
  undefined,
  "an unreferenced historical group should not be a current station anomaly",
);

assert.equal(
  groupIssueTagFor({
    bindings: [groupBinding({ id: "missing-disabled-key", groupName: "Disabled key group", bindingStatus: "missing" })],
    keys: [stationKey({ enabled: false, groupBindingId: "missing-disabled-key" })],
  }),
  undefined,
  "references from disabled keys should not keep a group anomaly active",
);

assert.equal(
  groupIssueTagFor({
    bindings: [groupBinding({ id: "missing-pro", groupName: "Pro", bindingStatus: "missing" })],
    keys: [stationKey({ name: "生产 Key", groupBindingId: "missing-pro", groupName: "Pro" })],
  })?.title,
  "分组「Pro」已下架，但仍被启用 Key「生产 Key」使用。",
  "an enabled key should expose a concrete missing-group reason through binding id",
);

assert.equal(
  groupIssueTagFor({
    bindings: [
      groupBinding({
        id: "disabled-hash",
        groupName: "Hash group",
        groupIdHash: "hash-group",
        bindingStatus: "disabled",
      }),
    ],
    keys: [stationKey({ name: "Hash Key", groupBindingId: null, groupIdHash: "hash-group", groupName: null })],
  })?.title,
  "分组「Hash group」已禁用，但仍被启用 Key「Hash Key」使用。",
  "an enabled key should match a disabled group by non-empty group id hash",
);

assert.equal(
  groupIssueTagFor({
    bindings: [groupBinding({ id: "missing-name", groupName: " Name Group ", bindingStatus: "missing" })],
    keys: [
      stationKey({ name: "名称 Key A", groupBindingId: null, groupIdHash: null, groupName: "name group" }),
      stationKey({ name: "名称 Key B", groupBindingId: null, groupIdHash: null, groupName: " NAME GROUP " }),
    ],
  })?.title,
  "分组「Name Group」已下架，但仍被 2 个启用 Key 使用：名称 Key A、名称 Key B。",
  "normalized group names should match and list multiple affected enabled keys",
);

assert.equal(
  groupIssueTagFor({
    bindings: [groupBinding({ id: "missing-name-fallback", groupName: "Shared Name", bindingStatus: "missing" })],
    keys: [
      stationKey({
        name: "明确绑定 Key",
        groupBindingId: "another-current-binding",
        groupIdHash: null,
        groupName: "Shared Name",
      }),
    ],
  }),
  undefined,
  "an explicit binding id should take precedence over a coincidental normalized-name match",
);

assert.equal(
  groupIssueTagFor({
    bindings: [
      groupBinding({
        id: "missing-hash-fallback",
        groupName: "Hash First",
        groupIdHash: "missing-hash",
        bindingStatus: "missing",
      }),
    ],
    keys: [
      stationKey({
        name: "明确哈希 Key",
        groupBindingId: null,
        groupIdHash: "another-hash",
        groupName: "Hash First",
      }),
    ],
  }),
  undefined,
  "an explicit group id hash should take precedence over a coincidental normalized-name match",
);

{
  const duplicateTag = groupIssueTagFor({
    bindings: [
      groupBinding({ id: "missing-name-v1", groupName: "Legacy Pro", bindingStatus: "missing" }),
      groupBinding({ id: "missing-name-v2", groupName: " legacy pro ", bindingStatus: "missing" }),
    ],
    keys: [stationKey({ name: "生产 Key", groupBindingId: null, groupIdHash: null, groupName: "Legacy Pro" })],
  });
  assert.equal(duplicateTag?.title, "分组「Legacy Pro」已下架，但仍被启用 Key「生产 Key」使用。");
  assert.equal(duplicateTag?.title?.split("\n").length, 1, "duplicate historical identities should produce one reason");
}

const pageSource = await readFile("src/features/stations/StationsPage.tsx", "utf8");
const viewModelSource = await readFile("src/features/stations/stationAssetViewModels.ts", "utf8");

assert.ok(
  pageSource.includes("stationIssueTags(row)") || pageSource.includes("const issueTags = stationIssueTags"),
  "station list row should render explicit issue tags from the row model",
);

assert.ok(
  pageSource.includes('station.stationType === "sub2api"') &&
    pageSource.includes('station.stationType === "newapi"') &&
    pageSource.includes("onAuthorize(station)"),
  "station list manual authorization action should be available for both Sub2API and NewAPI rows",
);

assert.ok(
  pageSource.includes("startManualAuthorization(station.id)") &&
    !pageSource.includes("finishWebAuthorizationSession(station.id)"),
  "station row authorization should start the automatic popup flow without a second required finish action",
);

assert.ok(
  pageSource.includes("STATION_ISSUE_FILTER_OPTIONS") &&
    pageSource.includes("filterStationAssetRowsByIssue(stationAssetRows, issueFilter)") &&
    pageSource.includes('ariaLabel="筛选问题标签"') &&
    pageSource.includes("<SortableContext items={filteredStationIds}") &&
    pageSource.includes("filteredStationAssetRows.map((row)") &&
    pageSource.includes('title="没有匹配的问题站点"'),
  "station page should expose an issue-tag filter that drives the visible sortable list and empty state",
);

assert.ok(
  !pageSource.includes("statusDotClassName") && !pageSource.includes("h-1.5 w-1.5 shrink-0 rounded-full"),
  "station list row should not use an ambiguous status color dot",
);

assert.ok(
  pageSource.includes("function StationIssueTagBadge") &&
    pageSource.includes("title={tag.title ?? tag.label}") &&
    !pageSource.includes('role="tooltip"') &&
    !pageSource.includes("group-hover/tag:visible") &&
    !pageSource.includes("group-focus/tag:visible") &&
    !pageSource.includes("aria-describedby={tag.title ? tooltipId : undefined}"),
  "detailed station issue tags should rely on the native title and not render a duplicate custom tooltip",
);

assert.ok(
  !viewModelSource.includes('label: "变更风险"') &&
    !viewModelSource.includes('label: "变更提醒"') &&
    !viewModelSource.includes('label: "采集需关注"') &&
    !viewModelSource.includes("路由"),
  "issue tags should avoid broad severity labels and route-related wording",
);

function tagLabels(input) {
  return stationIssueTags(input).map((tag) => tag.label);
}

function groupIssueTagFor({ bindings, keys }) {
  const projected = buildStationAssetRows({
    stations: [station()],
    keysByStation: new Map([["station-a", keys]]),
    balances: [],
    snapshotsByStation: new Map([["station-a", null]]),
    groupBindingsByStation: new Map([["station-a", bindings]]),
    groupRatesByStation: new Map([["station-a", []]]),
    changes: [],
  })[0];
  return stationIssueTags(projected).find((tag) => tag.kind === "group_issue");
}

function row(overrides = {}) {
  return {
    station: station(overrides.station),
    enabledKeyCount: 1,
    warningKeyCount: 0,
    latestBalance: null,
    balanceFactsReady: true,
    currentBalance: {
      value: 20,
      lowBalanceThreshold: 10,
      status: "normal",
      source: "station_cache",
      currency: "CNY",
      ...overrides.currentBalance,
    },
    latestSnapshot: null,
    riskEvents: [],
    rateChips: [],
    participatesInRouting: true,
    ...overrides,
    station: station(overrides.station),
  };
}

function station(overrides = {}) {
  return {
    id: "station-a",
    name: "Station A",
    stationType: "sub2api",
    websiteUrl: "https://station.example.test",
    apiBaseUrl: "https://station.example.test/v1",
    endpointRevision: 1,
    apiKeyMasked: "sk-...",
    apiKeyPresent: true,
    keyCount: 1,
    enabled: true,
    priority: 0,
    creditPerCny: 10,
    balanceRaw: null,
    balanceCny: 20,
    lowBalanceThresholdCny: 10,
    collectionIntervalMinutes: 5,
    status: "healthy",
    latencyMs: null,
    lastCheckedAt: "2026-07-09T00:00:00.000Z",
    lastPricingFetchedAt: null,
    note: null,
    createdAt: "2026-07-09T00:00:00.000Z",
    updatedAt: "2026-07-09T00:00:00.000Z",
    ...overrides,
  };
}

function groupBinding(overrides = {}) {
  return {
    id: "group-a",
    stationId: "station-a",
    stationKeyId: null,
    bindingKind: "station_group",
    parentGroupBindingId: null,
    groupKeyHash: "group-key-a",
    groupIdHash: null,
    groupName: "Group A",
    bindingStatus: "available",
    defaultRateMultiplier: 1,
    userRateMultiplier: 1,
    effectiveRateMultiplier: 1,
    inferredGroupCategory: "unknown",
    groupCategoryOverride: null,
    rateSource: "sub2api_groups_rates",
    confidence: 0.9,
    lastSeenAt: null,
    lastCheckedAt: "2026-07-16T00:00:00.000Z",
    lastRateChangedAt: null,
    rawJsonRedacted: null,
    createdAt: "2026-07-16T00:00:00.000Z",
    updatedAt: "2026-07-16T00:00:00.000Z",
    ...overrides,
  };
}

function stationKey(overrides = {}) {
  return {
    id: "key-a",
    stationId: "station-a",
    name: "Key A",
    apiKeyMasked: "sk-...",
    apiKeyPresent: true,
    enabled: true,
    priority: 0,
    maxConcurrency: 1,
    loadFactor: null,
    schedulable: true,
    groupBindingId: null,
    groupIdHash: null,
    groupName: null,
    tierLabel: null,
    rateMultiplier: null,
    manualRateMultiplier: null,
    manualRateUpdatedAt: null,
    rateSource: null,
    rateCollectedAt: null,
    balanceScope: null,
    status: "healthy",
    lastCheckedAt: null,
    lastUsedAt: null,
    note: null,
    createdAt: "2026-07-16T00:00:00.000Z",
    updatedAt: "2026-07-16T00:00:00.000Z",
    ...overrides,
  };
}

function change(overrides = {}) {
  return {
    id: "change-a",
    stationId: "station-a",
    severity: "warning",
    status: "unread",
    ...overrides,
  };
}
