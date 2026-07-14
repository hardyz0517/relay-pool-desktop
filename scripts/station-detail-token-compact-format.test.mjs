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

const { buildUsageCards } = module.exports;

const station = { id: "station-a" };
const cards = buildUsageCards(station, [
  {
    id: "balance-a",
    stationId: "station-a",
    scope: "station",
    totalTokenCount: 1_014_918_318,
    totalInputTokenCount: 93_859_016,
    totalOutputTokenCount: 5_764_966,
    todayTokenCount: 7_512,
    todayInputTokenCount: 6_400,
    todayOutputTokenCount: 1_200,
    status: "normal",
    source: "test",
    updatedAt: "2000",
    collectedAt: "2000",
  },
]);

const todayToken = cards.find((card) => card.label === "今日 Token");
const totalToken = cards.find((card) => card.label === "累计 Token");

assert.equal(todayToken.value, "7.5K");
assert.equal(todayToken.helper, "输入: 6.4K / 输出: 1.2K");
assert.equal(totalToken.value, "1B");
assert.equal(totalToken.helper, "输入: 93.9M / 输出: 5.8M");
assert.ok(!String(totalToken.value).includes(","));
assert.ok(!totalToken.helper.includes("93,859,016"));

const missingTokenCards = buildUsageCards(station, [
  {
    id: "balance-missing-token",
    stationId: "station-a",
    scope: "station",
    todayRequestCount: 1049,
    totalRequestCount: 16038,
    todayConsumption: 4.5767,
    totalConsumption: 42.228,
    todayTokenCount: null,
    todayInputTokenCount: null,
    todayOutputTokenCount: null,
    totalTokenCount: null,
    totalInputTokenCount: null,
    totalOutputTokenCount: null,
    status: "normal",
    source: "newapi_user_self",
    updatedAt: "2000",
    collectedAt: "2000",
  },
]);

const missingTodayToken = missingTokenCards.find((card) => card.label === "今日 Token");
const missingTotalToken = missingTokenCards.find((card) => card.label === "累计 Token");

assert.equal(missingTodayToken.value, "-");
assert.equal(missingTodayToken.helper, "输入: - / 输出: -");
assert.equal(missingTotalToken.value, "-");
assert.equal(missingTotalToken.helper, "输入: - / 输出: -");
