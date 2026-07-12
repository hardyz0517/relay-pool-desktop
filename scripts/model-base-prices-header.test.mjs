import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/pricing/ModelBasePricesPage.tsx", "utf8");

assert.ok(
  source.includes('title="模型基准价格"'),
  "model base prices page should keep the current page title",
);

assert.ok(
  source.includes("stickyHeader"),
  "model base prices page header should stay fixed while the page scrolls",
);

assert.ok(
  source.includes("backAction={"),
  "model base prices back affordance should live in the scaffold left header slot",
);

assert.ok(
  source.includes("backLabel: string;") &&
    source.includes("label={backLabel}") &&
    !source.includes('label="返回价格 / 倍率"'),
  "back affordance should use the explicit caller-provided accessible label",
);

assert.ok(
  !/<Button[\s\S]*?<ArrowLeft[\s\S]*?返回[\s\S]*?<\/Button>/.test(source),
  "back label should not occupy the right-side page actions",
);
