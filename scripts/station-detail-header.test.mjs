import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/components/StationDetailContent.tsx", "utf8");

assert.ok(
  source.includes('import { PageScaffold } from "@/components/shell/PageScaffold"'),
  "station detail should use the shared page scaffold header",
);

assert.ok(
  source.includes('title="中转站详情"'),
  "station detail page title should describe the current page",
);

assert.ok(
  source.includes('label="返回中转站资产"'),
  "back affordance should stay as an icon button accessible label",
);

assert.ok(
  !/<Button[\s\S]*?<ArrowLeft[\s\S]*?返回中转站资产[\s\S]*?<\/Button>/.test(source),
  "back label should not occupy the page title position",
);
