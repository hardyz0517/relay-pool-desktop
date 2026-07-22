import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const rustModels = await readFile("src-tauri/src/models/shared_capabilities.rs", "utf8");
const tsTypes = await readFile("src/lib/types/channelMonitors.ts", "utf8");
const apiSource = await readFile("src/lib/api/channelMonitors.ts", "utf8");
const querySource = await readFile("src/lib/queries/channelQueries.ts", "utf8");
const statusSource = await readFile("src/features/channels/ChannelStatusTab.tsx", "utf8");
const statusQuerySource = await readFile(
  "src-tauri/src/application/queries/channel_status.rs",
  "utf8",
);
const monitoringStoreSource = await readFile(
  "src-tauri/src/persistence/stores/monitoring_store.rs",
  "utf8",
);
const monitoringMigrationSource = await readFile(
  "src-tauri/src/persistence/migrations/0007_pricing_monitoring.sql",
  "utf8",
);

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
  statusQuerySource.includes("const RECENT_RUN_LIMIT: u32 = 60") &&
    statusQuerySource.includes(".recent_status_runs(read, monitor_limit.get(), RECENT_RUN_LIMIT)") &&
    monitoringStoreSource.includes("WITH bounded_monitors AS") &&
    monitoringStoreSource.includes("LIMIT ?2") &&
    monitoringStoreSource.includes("idx_channel_monitor_runs_monitor_started") &&
    monitoringMigrationSource.includes("CREATE INDEX idx_channel_monitor_runs_monitor_started"),
  "channel status backend summaries should be bounded and indexed",
);

assert.ok(
  (statusSource.includes('timeWindow === "recent" ? key.endpointPingMs : null') ||
    statusSource.includes('(timeWindow === "recent" ? key.endpointPingMs : null)')) &&
    statusSource.includes("availabilityLabelForWindow"),
  "channel status cards should avoid non-window fallbacks and label the selected window",
);
