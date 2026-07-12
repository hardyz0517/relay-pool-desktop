import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import ts from "typescript";

async function importRequestCostSummary() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-dashboard-cost-summary-"));
  const outputPath = join(tempRoot, "requestCostSummary.mjs");
  const source = await readFile("src/features/dashboard/requestCostSummary.ts", "utf8");
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      verbatimModuleSyntax: true,
    },
  }).outputText;
  await writeFile(outputPath, output, "utf8");
  return import(`file://${outputPath.replaceAll("\\", "/")}`);
}

const { summarizeDashboardRequestCosts } = await importRequestCostSummary();

const summary = summarizeDashboardRequestCosts(
  [
    requestLog("usd-today-a", {
      startedAt: "2026-07-10T09:00:00.000Z",
      estimatedTotalCost: 1.25,
      baseTotalCost: 0.5,
      costCurrency: "USD",
      costStatus: "priced",
    }),
    requestLog("cny-today", {
      startedAt: "2026-07-10T10:00:00.000Z",
      estimatedTotalCost: 8,
      baseTotalCost: 4,
      costCurrency: "CNY",
      costStatus: "base_price_only",
    }),
    requestLog("usd-today-legacy", {
      startedAt: "2026-07-10T11:00:00.000Z",
      estimatedTotalCost: 0.75,
      baseTotalCost: 0.25,
      costCurrency: "usd",
      costStatus: "legacy_estimate",
    }),
    requestLog("missing-rate", {
      startedAt: "2026-07-10T12:00:00.000Z",
      estimatedTotalCost: null,
      costCurrency: null,
      costStatus: "missing_rate",
    }),
    requestLog("unsupported", {
      startedAt: "2026-07-10T13:00:00.000Z",
      estimatedTotalCost: null,
      costCurrency: null,
      costStatus: "unsupported_billing_mode",
    }),
    requestLog("usd-yesterday", {
      startedAt: "2026-07-09T12:00:00.000Z",
      estimatedTotalCost: 5,
      baseTotalCost: 2,
      costCurrency: "USD",
      costStatus: "priced",
    }),
  ],
  new Date("2026-07-10T15:00:00.000Z"),
);

assert.deepEqual(
  summary.todayTotalsByCurrency.map((row) => ({
    currency: row.currency,
    totalCost: row.totalCost,
    baseTotalCost: row.baseTotalCost,
    requestCount: row.requestCount,
  })),
  [
    { currency: "CNY", totalCost: 8, baseTotalCost: 4, requestCount: 1 },
    { currency: "USD", totalCost: 2, baseTotalCost: 0.75, requestCount: 2 },
  ],
  "dashboard should keep today's request cost totals grouped by currency",
);

assert.deepEqual(
  summary.allTotalsByCurrency.map((row) => ({
    currency: row.currency,
    totalCost: row.totalCost,
    baseTotalCost: row.baseTotalCost,
    requestCount: row.requestCount,
  })),
  [
    { currency: "CNY", totalCost: 8, baseTotalCost: 4, requestCount: 1 },
    { currency: "USD", totalCost: 7, baseTotalCost: 2.75, requestCount: 3 },
  ],
  "dashboard should keep cumulative request cost totals grouped by currency",
);

assert.equal(summary.statusCounts.priced, 2);
assert.equal(summary.statusCounts.basePriceOnly, 1);
assert.equal(summary.statusCounts.legacyEstimate, 1);
assert.equal(summary.statusCounts.missingRate, 1);
assert.equal(summary.statusCounts.unsupportedBillingMode, 1);
assert.equal(summary.unpricedCount, 2);
assert.equal(summary.legacyEstimateCount, 1);
assert.equal(summary.unsupportedBillingModeCount, 1);

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
assert.match(
  dashboardSource,
  /currency: "USD", totalCost: 0, baseTotalCost: 0, requestCount: 0/,
  "empty cost totals should render the explicit paired zero values shown in the dashboard design",
);
assert.match(
  dashboardSource,
  /<span>总计: <\/span>/,
  "the cumulative paired costs should use the 总计: label as a separate muted prefix",
);
assert.match(
  dashboardSource,
  /title="实际花费"/,
  "the charged side of the dashboard cost pair should identify its actual-cost semantics",
);
assert.match(
  dashboardSource,
  /title="1倍率 Token 花费"/,
  "the standard side of the dashboard cost pair should identify its 1x token-cost semantics",
);
assert.match(
  dashboardSource,
  /text-sm font-normal text-slate-400/,
  "the slash and 1x token cost should be smaller and gray like Sub2API",
);
assert.doesNotMatch(
  dashboardSource,
  /tone: requestCostSummary\.todayTotalsByCurrency\.length > 0 \? "warning" : "neutral"/,
  "actual cost should keep the purple cost accent instead of turning into a warning color",
);
assert.match(
  dashboardSource,
  /formatRecentRequestCost\(requestBaseCostValue\(request\),\s*request\.costCurrency,\s*request\.costStatus\)/,
  "recent usage rows should display the single request 1x base cost after the actual charged cost",
);
assert.doesNotMatch(
  dashboardSource,
  /requestCostSummary\.allTotalsByCurrency[\s\S]{0,160}tokens/,
  "recent usage rows should not use cumulative request totals as the per-row base cost",
);

function requestLog(id, overrides) {
  return {
    id,
    startedAt: "2026-07-10T00:00:00.000Z",
    finishedAt: null,
    durationMs: null,
    method: "POST",
    path: "/v1/chat/completions",
    model: "gpt-5.4-mini",
    stream: false,
    status: "success",
    stationKeyId: "key-1",
    stationId: "station-1",
    upstreamBaseUrl: "https://relay.example.test",
    fallbackCount: 0,
    errorMessage: null,
    routePolicy: null,
    routeReason: null,
    rejectedCandidatesJson: null,
    promptTokens: 100,
    completionTokens: 50,
    totalTokens: 150,
    estimatedInputCost: null,
    estimatedOutputCost: null,
    estimatedTotalCost: null,
    baseInputCost: null,
    baseOutputCost: null,
    baseFixedCost: null,
    baseTotalCost: null,
    costCurrency: null,
    pricingRuleId: null,
    pricingSource: null,
    costStatus: null,
    groupBindingId: null,
    normalizationStatus: null,
    balanceScope: null,
    economicContextJson: null,
    createdAt: "2026-07-10T00:00:00.000Z",
    ...overrides,
  };
}
