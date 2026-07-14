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
    "export function buildCurrentStationBalanceFacts() { return new Map(); }",
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

const { stationIssueTags, filterStationAssetRowsByIssue, STATION_ISSUE_FILTER_OPTIONS } = await importStationAssetViewModels();

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
  !viewModelSource.includes('label: "变更风险"') &&
    !viewModelSource.includes('label: "变更提醒"') &&
    !viewModelSource.includes('label: "采集需关注"') &&
    !viewModelSource.includes("路由"),
  "issue tags should avoid broad severity labels and route-related wording",
);

function tagLabels(input) {
  return stationIssueTags(input).map((tag) => tag.label);
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

function change(overrides = {}) {
  return {
    id: "change-a",
    stationId: "station-a",
    severity: "warning",
    status: "unread",
    ...overrides,
  };
}
