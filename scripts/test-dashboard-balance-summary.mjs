import assert from "node:assert/strict";
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import vm from "node:vm";
import ts from "typescript";

const root = process.cwd();
const require = createRequire(import.meta.url);
const sourcePath = path.join(root, "src", "features", "dashboard", "dashboardBalanceSummary.ts");
const source = fs.readFileSync(sourcePath, "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.CommonJS,
    target: ts.ScriptTarget.ES2022,
  },
});

const module = { exports: {} };
vm.runInNewContext(compiled.outputText, {
  exports: module.exports,
  module,
  require,
}, { filename: sourcePath });

const { summarizeDashboardBalances } = module.exports;

const balances = [
  {
    id: "key-raw-newer",
    stationId: "station-a",
    scope: "station_key",
    value: 100,
    currency: "CNY",
    status: "normal",
    updatedAt: "5000",
  },
  {
    id: "station-normalized",
    stationId: "station-a",
    scope: "station",
    value: 10,
    currency: "CNY",
    todayRequestCount: 12,
    totalRequestCount: 120,
    todayConsumption: 0.75,
    totalConsumption: 8.5,
    todayTokenCount: 34567,
    totalTokenCount: 456789,
    todayInputTokenCount: 28000,
    todayOutputTokenCount: 6567,
    totalInputTokenCount: 410000,
    totalOutputTokenCount: 46789,
    status: "normal",
    updatedAt: "2000",
  },
  {
    id: "station-b-old-low",
    stationId: "station-b",
    scope: "station",
    value: 5,
    currency: "CNY",
    status: "low",
    updatedAt: "1000",
  },
  {
    id: "station-b-newer-normal",
    stationId: "station-b",
    scope: "station",
    value: 6,
    currency: "CNY",
    todayRequestCount: 8,
    totalRequestCount: 80,
    todayConsumption: 0.25,
    totalConsumption: 2.5,
    todayTokenCount: 1000,
    totalTokenCount: 2000,
    todayInputTokenCount: 800,
    todayOutputTokenCount: 200,
    totalInputTokenCount: 1500,
    totalOutputTokenCount: 500,
    status: "normal",
    updatedAt: "4000",
  },
  {
    id: "station-c-usd",
    stationId: "station-c",
    scope: "station",
    value: 2,
    currency: "USD",
    status: "depleted",
    updatedAt: "3000",
  },
];

const stations = [
  {
    id: "station-a",
    creditPerCny: 10,
  },
  {
    id: "station-b",
    creditPerCny: 5,
  },
  {
    id: "station-c",
    creditPerCny: 1,
  },
];

const summary = summarizeDashboardBalances(balances, stations);

assert.equal(summary.totalBalance, 18);
assert.equal(summary.lowBalanceStations, 1);
assert.equal(summary.primaryBalanceCurrency, "CNY");
assert.deepEqual(JSON.parse(JSON.stringify(summary.stationUsage)), {
  todayRequestCount: 20,
  totalRequestCount: 200,
  todayConsumption: 0.125,
  totalConsumption: 1.35,
  todayTokenCount: 35567,
  totalTokenCount: 458789,
  todayInputTokenCount: 28800,
  todayOutputTokenCount: 6767,
  totalInputTokenCount: 411500,
  totalOutputTokenCount: 47289,
});
assert.equal(
  summary.latestStationBalances.map((balance) => balance.id).join(","),
  "station-normalized,station-b-newer-normal,station-c-usd",
);
