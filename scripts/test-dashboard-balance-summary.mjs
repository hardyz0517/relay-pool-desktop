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

const summary = summarizeDashboardBalances([
  {
    id: "key-raw-old",
    stationId: "station-a",
    scope: "station_key",
    value: 100,
    currency: "CNY",
    status: "normal",
    updatedAt: "3000",
  },
  {
    id: "station-normalized",
    stationId: "station-a",
    scope: "station",
    value: 10,
    currency: "CNY",
    status: "normal",
    updatedAt: "2000",
  },
  {
    id: "station-b",
    stationId: "station-b",
    scope: "station",
    value: 5,
    currency: "CNY",
    status: "low",
    updatedAt: "1000",
  },
  {
    id: "station-b-newer",
    stationId: "station-b",
    scope: "station",
    value: 6,
    currency: "CNY",
    status: "normal",
    updatedAt: "4000",
  },
]);

assert.equal(summary.totalBalance, 16);
assert.equal(summary.lowBalanceStations, 0);
assert.equal(summary.primaryBalanceCurrency, "CNY");
assert.equal(
  summary.latestStationBalances.map((balance) => balance.id).join(","),
  "station-normalized,station-b-newer",
);
