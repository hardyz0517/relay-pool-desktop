import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const rustModels = await readFile("src-tauri/src/models/shared_capabilities.rs", "utf8");
const tsTypes = await readFile("src/lib/types/channelMonitors.ts", "utf8");
const apiSource = await readFile("src/lib/api/channelMonitors.ts", "utf8");
const querySource = await readFile("src/lib/queries/channelQueries.ts", "utf8");
const statusSource = await readFile("src/features/channels/ChannelStatusTab.tsx", "utf8");
const databaseSource = await readFile("src-tauri/src/services/database.rs", "utf8");

assert.ok(
  rustModels.includes("pub struct ChannelStatusWindowSummary") &&
    rustModels.includes("pub struct ChannelStatusTimelinePoint") &&
    rustModels.includes("pub struct ChannelStatusSummary"),
  "Rust shared capability models should expose channel status summary DTOs",
);

assert.ok(
  tsTypes.includes("export type ChannelStatusWindowSummary") &&
    tsTypes.includes("export type ChannelStatusTimelinePoint") &&
    tsTypes.includes("export type ChannelStatusSummary"),
  "TypeScript channel monitor types should mirror backend status summary DTOs",
);

assert.ok(
  apiSource.includes("listChannelStatusSummaries") &&
    apiSource.includes('invoke<ChannelStatusSummary[]>("list_channel_status_summaries"'),
  "channel monitor API should expose a typed channel status summary command wrapper",
);

assert.ok(
  querySource.includes("listChannelStatusSummaries()") &&
    querySource.includes("channelStatusSummaries: ChannelStatusSummary[]"),
  "channel status query service should load backend summaries",
);

assert.ok(
  !statusSource.includes("list_channel_status_summaries") &&
    !statusSource.includes("@tauri-apps/api/core"),
  "ChannelStatusTab must consume query/API helpers instead of invoking Tauri directly",
);

assert.ok(
  databaseSource.includes("CHANNEL_STATUS_TIMELINE_LIMIT") &&
    databaseSource.includes(".clamp(1, CHANNEL_STATUS_TIMELINE_LIMIT)") &&
    databaseSource.includes("idx_channel_monitor_runs_monitor_started_at") &&
    databaseSource.includes("WITH latest_runs AS"),
  "channel status backend summaries should be bounded and indexed",
);

assert.ok(
  (statusSource.includes('timeWindow === "recent" ? key.endpointPingMs : null') ||
    statusSource.includes('(timeWindow === "recent" ? key.endpointPingMs : null)')) &&
    statusSource.includes("availabilityLabelForWindow"),
  "channel status cards should avoid non-window fallbacks and label the selected window",
);
