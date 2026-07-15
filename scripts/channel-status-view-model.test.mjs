import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
const esbuild = require("../node_modules/.pnpm/node_modules/esbuild");

const outFile = resolve(tmpdir(), "relay-pool-channel-status-view-model.test.mjs");
await mkdir(dirname(outFile), { recursive: true });
await esbuild.build({
  entryPoints: ["src/features/channels/channelStatusViewModel.ts"],
  outfile: outFile,
  bundle: true,
  platform: "node",
  format: "esm",
  external: ["react", "lucide-react", "@tauri-apps/api/core"],
});

const {
  hslForAvailabilityPercent,
  recentOutcomeBarHeightPercent,
  buildMonitorRecentOutcomes,
  buildMonitorTimelineOutcomes,
  buildRecentOutcomes,
  enabledStationKeyMonitorsByKey,
  monitorRunToStationKeyStatus,
  orderChannelsBySavedOrder,
  resolveChannelLatencyMetrics,
  selectChannelStatusWindowSummary,
} = await import(pathToFileURL(outFile).href);

assert.equal(
  hslForAvailabilityPercent(0),
  "hsl(0 72% 42%)",
  "0% availability should use Sub2API's red end of the continuous HSL scale",
);
assert.equal(
  hslForAvailabilityPercent(50),
  "hsl(60 72% 42%)",
  "50% availability should use Sub2API's yellow midpoint instead of a warning text token",
);
assert.equal(
  hslForAvailabilityPercent(100),
  "hsl(120 72% 42%)",
  "100% availability should use Sub2API's green end of the continuous HSL scale",
);
assert.equal(
  hslForAvailabilityPercent(null),
  undefined,
  "missing availability should let the UI fall back to a neutral color",
);

assert.equal(
  recentOutcomeBarHeightPercent("success"),
  100,
  "successful monitor timeline bars should be the tallest bars",
);
assert.equal(
  recentOutcomeBarHeightPercent("warning"),
  65,
  "warning monitor timeline bars should be shorter than successes",
);
assert.equal(
  recentOutcomeBarHeightPercent("failed"),
  35,
  "failed monitor timeline bars should be visibly shorter than healthy bars",
);
assert.equal(
  recentOutcomeBarHeightPercent("unknown"),
  15,
  "unknown monitor timeline placeholders should be the shortest bars",
);

const outcomes = buildRecentOutcomes([], {
  successCount: 2,
  failureCount: 2,
});
assert.equal(outcomes.length, 60, "outcome strip should keep 60 slots");
assert.equal(outcomes.filter((item) => item === "success").length, 2);
assert.equal(outcomes.filter((item) => item === "failed").length, 2);
assert.equal(outcomes.at(-1), "success", "latest health success should color the newest slot");

const orderedChannels = orderChannelsBySavedOrder(
  [
    { id: "new-channel", name: "new" },
    { id: "second", name: "second" },
    { id: "first", name: "first" },
  ],
  ["first", "second", "missing"],
);
assert.deepEqual(
  orderedChannels.map((channel) => channel.id),
  ["first", "second", "new-channel"],
  "saved channel order should be preserved while new channels append at the end",
);

assert.deepEqual(
  resolveChannelLatencyMetrics({ requestLatencyMs: null, healthLatencyMs: 5422, endpointPingMs: 38 }),
  { conversationLatencyMs: 5422, endpointPingMs: 38 },
  "endpoint PING should display the station endpoint latency synced during monitor runs",
);

assert.deepEqual(
  resolveChannelLatencyMetrics({ requestLatencyMs: 1280, healthLatencyMs: 5422, endpointPingMs: null }),
  { conversationLatencyMs: 1280, endpointPingMs: null },
  "real proxy request logs should stay the preferred conversation latency source without inventing endpoint PING",
);

const enabledMonitorByKey = enabledStationKeyMonitorsByKey([
  {
    id: "old-monitor",
    targetType: "station_key",
    stationKeyId: "key-a",
    enabled: true,
    updatedAt: "1000",
  },
  {
    id: "station-monitor",
    targetType: "station",
    stationKeyId: null,
    enabled: true,
    updatedAt: "3000",
  },
  {
    id: "disabled-monitor",
    targetType: "station_key",
    stationKeyId: "key-b",
    enabled: false,
    updatedAt: "4000",
  },
  {
    id: "latest-monitor",
    targetType: "station_key",
    stationKeyId: "key-a",
    enabled: true,
    updatedAt: "5000",
  },
]);
assert.deepEqual(
  [...enabledMonitorByKey.entries()].map(([keyId, monitor]) => [keyId, monitor.id]),
  [["key-a", "latest-monitor"]],
  "channel status should be driven only by the latest enabled station-key monitor for each key",
);

assert.equal(
  monitorRunToStationKeyStatus({ status: "success" }),
  "healthy",
  "successful monitor run should render as healthy key status",
);
assert.equal(
  monitorRunToStationKeyStatus({ status: "failed" }),
  "error",
  "failed monitor run should render as error key status",
);
assert.equal(
  monitorRunToStationKeyStatus(null),
  "unchecked",
  "missing monitor run should render as unchecked instead of leaking stale key status",
);

const monitorOutcomes = buildMonitorRecentOutcomes([
  { status: "success", startedAt: "1000" },
  { status: "failed", startedAt: "2000" },
  { status: "warning", startedAt: "3000" },
]);
assert.equal(monitorOutcomes.length, 60, "monitor outcome strip should keep 60 slots");
assert.deepEqual(
  monitorOutcomes.slice(-3),
  ["success", "failed", "warning"],
  "monitor outcomes should preserve chronological run order in the newest slots",
);

const backendSummary = {
  recent: { window: "recent", totalCount: 3, availabilityPercent: 66.67 },
  last24h: { window: "24h", totalCount: 1, availabilityPercent: 100 },
  last7d: { window: "7d", totalCount: 2, availabilityPercent: 50 },
};
assert.equal(
  selectChannelStatusWindowSummary(backendSummary, "24h").availabilityPercent,
  100,
  "24h cards should use backend 24h summary instead of frontend raw-run slicing",
);
assert.equal(
  selectChannelStatusWindowSummary(backendSummary, "7d").availabilityPercent,
  50,
  "7d cards should use backend 7d summary",
);

const timelineOutcomes = buildMonitorTimelineOutcomes([
  { status: "warning" },
  { status: "failed" },
  { status: "success" },
]);
assert.deepEqual(
  timelineOutcomes.slice(-3),
  ["success", "failed", "warning"],
  "backend timeline points arrive newest-first and should render oldest-to-newest in the strip",
);
