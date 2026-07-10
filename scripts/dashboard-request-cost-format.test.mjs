import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import ts from "typescript";

async function importRequestCostFormat() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-dashboard-cost-format-"));
  const outputPath = join(tempRoot, "requestCostFormat.mjs");
  const source = await readFile("src/features/dashboard/requestCostFormat.ts", "utf8");
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

const { formatRequestCost } = await importRequestCostFormat();
const { requestBaseCostValue } = await importRequestCostFormat();

assert.equal(
  formatRequestCost(0.00001725, "USD", "base_price_only"),
  "$0.00001725",
  "non-zero micro costs should not round down to zero",
);

assert.equal(
  formatRequestCost(0.000000001, "USD", "priced"),
  "< $0.00000001",
  "costs below the displayed precision should still be shown as non-zero",
);

assert.equal(formatRequestCost(1.25, "USD", "priced"), "$1.2500");
assert.equal(formatRequestCost(null, "USD", "usage_only"), "未定价");
assert.equal(formatRequestCost(null, "USD", null), "-");

assert.equal(
  requestBaseCostValue({
    estimatedTotalCost: 0.00001725,
    baseTotalCost: null,
    costStatus: "base_price_only",
  }),
  0.00001725,
  "base-price-only logs should fall back to actual cost when older base snapshots are missing",
);

assert.equal(
  requestBaseCostValue({
    estimatedTotalCost: 0.00001725,
    baseTotalCost: null,
    costStatus: "priced",
  }),
  null,
  "priced logs without base snapshots should not treat actual charged cost as 1x base cost",
);
