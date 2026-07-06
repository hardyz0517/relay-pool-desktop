import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/channels/ChannelMonitoringTab.tsx", "utf8");

assert.ok(
  !source.includes("min-w-[780px]") && !source.includes("min-w-[880px]"),
  "monitor list grid should not force a fixed minimum width",
);

assert.ok(
  !source.includes('className="overflow-x-auto"'),
  "monitor list should not use horizontal scrolling for the normal desktop layout",
);

assert.match(
  source,
  /grid-cols-\[minmax\(0,0\.9fr\)_minmax\(0,1\.15fr\)_minmax\(0,1\.15fr\)_minmax\(0,0\.75fr\)_minmax\(0,0\.75fr\)\]/,
  "monitor header and rows should share compact responsive zero-min grid columns",
);

assert.ok(
  !source.includes("<TableHeadCell>测试模板</TableHeadCell>") && !source.includes('label="测试模板"'),
  "monitor list should not expose request templates as a visible column or card field",
);

assert.ok(
  source.includes("<TableHeadCell>主模型</TableHeadCell>") && source.includes('label="主模型"'),
  "monitor list should show the primary detection model instead of the request template",
);

assert.ok(
  !source.includes("<TableHeadCell>最近检测</TableHeadCell>") && !source.includes('label="最近检测"'),
  "monitor list should not expose a latest detection time column or card field",
);

assert.ok(
  source.includes("立即检测"),
  "monitor actions should expose an immediate detection action through labels or tooltips",
);

assert.ok(
  !source.includes('{running ? "检测中" : "立即检测"}') &&
    !source.includes('{running ? "运行中" : "立即检测"}'),
  "immediate detection should be an icon action, not visible button text",
);

assert.ok(
  source.includes("hidden lg:grid"),
  "monitor table header and row should only render as a table layout on large windows",
);

assert.ok(
  source.includes("lg:hidden") && source.includes("MonitorCardField"),
  "monitor list should render card fields on small windows",
);
