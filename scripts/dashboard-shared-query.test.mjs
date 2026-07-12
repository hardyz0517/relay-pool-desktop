import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

for (const option of [
  "proxyStatusQueryOptions",
  "requestLogsQueryOptions",
  "keyPoolQueryOptions",
  "stationsQueryOptions",
  "balanceSnapshotsQueryOptions",
  "settingsQueryOptions",
  "changeEventsQueryOptions",
]) {
  assert.ok(source.includes(option), `Dashboard should consume ${option}`);
}
assert.match(source, /usePageActivity\(\)/);
assert.match(source, /useActivityQuery/);
assert.ok(!source.includes("loadDashboardWorkspace"));
assert.ok(!source.includes("window.setInterval"));
assert.ok(!source.includes("setProxyStatus"));
assert.ok(!source.includes("setRequestLogs"));

console.log("dashboard shared query contract passed");
