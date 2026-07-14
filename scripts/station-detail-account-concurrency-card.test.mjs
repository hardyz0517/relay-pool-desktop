import assert from "node:assert/strict";
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import vm from "node:vm";
import ts from "typescript";

const root = process.cwd();
const require = createRequire(import.meta.url);
const sourcePath = path.join(root, "src", "features", "stations", "stationDetailViewModels.ts");
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
  require: (specifier) => {
    if (specifier === "@/lib/projections/balanceFacts") {
      return {
        currentStationBalanceFor({ station, balances }) {
          const sourceSnapshot =
            balances.find((balance) => balance.stationId === station.id && balance.scope === "station") ?? null;
          return {
            sourceSnapshot,
            collectedAt: sourceSnapshot?.collectedAt ?? null,
            currency: sourceSnapshot?.currency ?? "CNY",
            value: sourceSnapshot?.value ?? null,
            lowBalanceThreshold: sourceSnapshot?.lowBalanceThreshold ?? null,
            status: sourceSnapshot?.status ?? "unknown",
            source: sourceSnapshot ? sourceSnapshot.source : "missing",
            sourceLabel: sourceSnapshot ? sourceSnapshot.source : "missing",
            updatedAt: sourceSnapshot?.updatedAt ?? null,
          };
        },
      };
    }
    if (specifier === "@/lib/projections/groupFacts") {
      return {
        buildCurrentStationGroupFacts: () => [],
        isDisplayableStationGroupCurrentFact: () => false,
      };
    }
    if (specifier === "@/lib/time") {
      return { toTimestampMillis: (value) => Number(value) || Date.parse(value) };
    }
    if (specifier === "@/lib/formatters") {
      return { formatTrimmedDecimal: (value, digits = 2) => Number(value).toFixed(digits).replace(/\.?0+$/, "") };
    }
    return require(specifier);
  },
}, { filename: sourcePath });

const { buildBalanceCards } = module.exports;

const snapshot = {
  id: "balance-a",
  stationId: "station-a",
  stationKeyId: null,
  scope: "station",
  value: 66.78,
  currency: "CNY",
  accountConcurrencyLimit: 5,
  lowBalanceThreshold: 10,
  status: "normal",
  source: "sub2api_account_profile",
  updatedAt: "2000",
  collectedAt: "2000",
};

const sub2apiCards = buildBalanceCards({ id: "station-a", stationType: "sub2api" }, [snapshot]);
const concurrencyCard = sub2apiCards.find((card) => card.label === "并发限制");

assert.ok(concurrencyCard, "Sub2API station should show account concurrency card beside balance cards");
assert.equal(concurrencyCard.value, "5 路");
assert.match(concurrencyCard.helper, /Sub2API/);

const newapiCards = buildBalanceCards({ id: "station-a", stationType: "newapi" }, [snapshot]);
assert.ok(
  !newapiCards.some((card) => card.label === "并发限制"),
  "NewAPI station should not show Sub2API-only account concurrency card",
);
