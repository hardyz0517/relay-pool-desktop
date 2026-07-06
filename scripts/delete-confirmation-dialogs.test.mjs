import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const sourceFiles = [
  "src/features/changes/ChangeCenterPage.tsx",
  "src/features/channels/ChannelMonitoringTab.tsx",
  "src/features/channels/ChannelMonitorTemplateManager.tsx",
  "src/features/logs/LogsPage.tsx",
  "src/features/routing/RoutingPage.tsx",
  "src/features/stations/StationsPage.tsx",
  "src/features/key-pool/KeyPoolPage.tsx",
];

for (const file of sourceFiles) {
  const source = await readFile(file, "utf8");
  assert.ok(
    !source.includes("window.confirm"),
    `${file} should use the in-app ConfirmDialog instead of native window.confirm`,
  );
}

const destructiveSurfaces = [
  "src/features/changes/ChangeCenterPage.tsx",
  "src/features/channels/ChannelMonitoringTab.tsx",
  "src/features/channels/ChannelMonitorTemplateManager.tsx",
  "src/features/logs/LogsPage.tsx",
  "src/features/routing/RoutingPage.tsx",
  "src/features/stations/StationsPage.tsx",
  "src/features/key-pool/KeyPoolPage.tsx",
];

for (const file of destructiveSurfaces) {
  const source = await readFile(file, "utf8");
  assert.ok(
    source.includes("ConfirmDialog"),
    `${file} should render an in-app second confirmation for destructive actions`,
  );
}

console.log("delete confirmation dialog checks passed");
