import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const logsSource = await readFile("src/features/logs/LogsPage.tsx", "utf8");

assert.ok(
  logsSource.includes("settingsQueryOptions") &&
    logsSource.includes("useActivityQuery(refreshEnabled, settingsQueryOptions())"),
  "logs page should read the shared application settings through resource queries",
);

assert.ok(
  logsSource.includes("const developerModeEnabled = settingsQuery.data?.developerModeEnabled ?? false;"),
  "logs page should default details to hidden from the developer-mode query state",
);

assert.match(
  logsSource,
  /\{developerModeEnabled\s*&&\s*\(\s*<InspectorPanel[\s\S]*?title="日志详情"[\s\S]*?<\/InspectorPanel>\s*\)\}/,
  "request log details should only render while developer mode is enabled",
);
