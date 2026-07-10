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
      costCurrency: "USD",
      costStatus: "priced",
    }),
    requestLog("cny-today", {
      startedAt: "2026-07-10T10:00:00.000Z",
      estimatedTotalCost: 8,
      costCurrency: "CNY",
      costStatus: "base_price_only",
    }),
    requestLog("usd-today-legacy", {
      startedAt: "2026-07-10T11:00:00.000Z",
      estimatedTotalCost: 0.75,
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
    requestCount: row.requestCount,
  })),
  [
    { currency: "CNY", totalCost: 8, requestCount: 1 },
    { currency: "USD", totalCost: 2, requestCount: 2 },
  ],
  "dashboard should keep today's request cost totals grouped by currency",
);

assert.deepEqual(
  summary.allTotalsByCurrency.map((row) => ({
    currency: row.currency,
    totalCost: row.totalCost,
    requestCount: row.requestCount,
  })),
  [
    { currency: "CNY", totalCost: 8, requestCount: 1 },
    { currency: "USD", totalCost: 7, requestCount: 3 },
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
