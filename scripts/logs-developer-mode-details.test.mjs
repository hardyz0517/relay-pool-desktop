import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const logsSource = await readFile("src/features/logs/LogsPage.tsx", "utf8");

assert.ok(
  logsSource.includes('import { getSettings } from "@/lib/api/settings";'),
  "logs page should read the shared application settings",
);

assert.ok(
  logsSource.includes("const [developerModeEnabled, setDeveloperModeEnabled] = useState(false);") &&
    logsSource.includes("setDeveloperModeEnabled(settings.developerModeEnabled);"),
  "logs page should default details to hidden and refresh the developer-mode setting",
);

assert.match(
  logsSource,
  /\{developerModeEnabled\s*&&\s*\(\s*<InspectorPanel[\s\S]*?title="日志详情"[\s\S]*?<\/InspectorPanel>\s*\)\}/,
  "request log details should only render while developer mode is enabled",
);
