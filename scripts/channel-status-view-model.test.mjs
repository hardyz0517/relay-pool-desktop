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
  availabilityToneClassName,
  buildMonitorRecentOutcomes,
  buildRecentOutcomes,
  enabledStationKeyMonitorsByKey,
  monitorRunToStationKeyStatus,
  orderChannelsBySavedOrder,
  resolveChannelLatencyMetrics,
} = await import(pathToFileURL(outFile).href);

assert.equal(
  availabilityToneClassName({ status: "healthy", availabilityPercent: 50 }),
  "text-orange-600",
  "50% availability should be orange, not red",
);
assert.equal(
  availabilityToneClassName({ status: "healthy", availabilityPercent: 49.99 }),
  "text-rose-600",
  "availability below 50% should be red",
);
assert.equal(
  availabilityToneClassName({ status: "healthy", availabilityPercent: 75 }),
  "text-emerald-600",
  "75% availability should be green",
);
assert.equal(
  availabilityToneClassName({ status: "healthy", availabilityPercent: 91.3 }),
  "text-emerald-600",
  "91.3% availability should be green",
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
