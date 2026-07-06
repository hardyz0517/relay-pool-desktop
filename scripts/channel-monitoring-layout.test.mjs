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
  /grid-cols-\[minmax\(0,0\.9fr\)_minmax\(0,1\.1fr\)_minmax\(0,1\.15fr\)_minmax\(0,0\.75fr\)_minmax\(0,1fr\)_minmax\(0,0\.5fr\)\]/,
  "monitor header and rows should share compact responsive zero-min grid columns",
);

assert.ok(
  source.includes("hidden lg:grid"),
  "monitor table header and row should only render as a table layout on large windows",
);

assert.ok(
  source.includes("lg:hidden") && source.includes("MonitorCardField"),
  "monitor list should render card fields on small windows",
);
