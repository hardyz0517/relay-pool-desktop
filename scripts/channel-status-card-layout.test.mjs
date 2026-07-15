import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/channels/ChannelStatusTab.tsx", "utf8");

assert.ok(
  source.includes("sm:grid-cols-[repeat(auto-fill,minmax(320px,360px))]"),
  "channel status cards should keep a bounded desktop width instead of stretching with the page",
);

assert.ok(
  source.includes("grid-cols-1"),
  "channel status cards should remain a single full-width column on narrow screens",
);

assert.ok(
  source.includes("justify-start"),
  "channel status grid should leave extra wide-screen space at the end instead of stretching cards",
);

assert.ok(
  !source.includes("md:grid-cols-2 2xl:grid-cols-3"),
  "fractional breakpoint columns make channel status cards grow too wide on large pages",
);

assert.ok(
  !source.includes('{channel.lastError ?? ""}'),
  "channel status cards should not render the last error summary as a bottom line because it changes card height",
);

assert.ok(
  !source.includes('runChannelMonitorNow'),
  "channel status page should not expose a separate model-detection action; scheduled monitors own detection",
);

assert.ok(
  !source.includes('pingStationEndpoint'),
  "channel status page should not expose a separate endpoint PING action",
);

assert.ok(
  !source.includes('检测模型') && !source.includes('PING 中'),
  "channel status toolbar should not contain manual model detection or PING controls",
);

assert.ok(
  source.includes('label="端点 PING"'),
  "channel status card should show endpoint PING that is refreshed by monitor runs",
);

assert.ok(
  source.includes("selectChannelStatusWindowSummary(backendSummary, timeWindow)") &&
    source.includes('const aggregateHealth = timeWindow === "recent" ? keyHealth : null') &&
    source.includes("buildMonitorTimelineOutcomes(windowSummary.timeline)"),
  "channel status time-window tabs should use backend summaries and reserve aggregate health fallback for recent mode",
);

assert.ok(
  source.includes("hslForAvailabilityPercent(channel.availabilityPercent)") &&
    source.includes("recentOutcomeBarHeightPercent(outcome)") &&
    source.includes("items-end"),
  "channel status cards should use Sub2API-style continuous availability color and variable-height outcome bars",
);
