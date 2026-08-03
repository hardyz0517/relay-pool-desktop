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

const requestCostFormat = await importRequestCostFormat();
const { formatRequestCost, formatRecentRequestCost } = requestCostFormat;

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
  formatRecentRequestCost(0.00001725, "USD", "base_price_only"),
  "< $0.0001",
  "recent usage costs should show at most four decimal places without hiding non-zero costs",
);
assert.equal(
  formatRecentRequestCost(0.000000001, "USD", "priced"),
  "< $0.0001",
  "recent usage costs below the four-decimal precision should stay compact",
);
assert.equal(formatRecentRequestCost(1.25, "USD", "priced"), "$1.2500");
assert.equal(formatRecentRequestCost(null, "USD", "usage_only"), "未定价");

assert.ok(
  !("requestBaseCostValue" in requestCostFormat),
  "dashboard request cost formatting should no longer export the legacy base-cost helper",
);
