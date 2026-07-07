import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const files = {
  stationKeysApi: await readFile("src/lib/api/stationKeys.ts", "utf8"),
  groupFactsApi: await readFile("src/lib/api/groupFacts.ts", "utf8"),
  channelApi: await readFile("src/lib/api/channelMonitors.ts", "utf8"),
  addKey: await readFile("src/features/key-pool/AddKeyPage.tsx", "utf8"),
  editKey: await readFile("src/features/key-pool/EditKeyPage.tsx", "utf8"),
  keyPool: await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8"),
  createRemoteKeyDialog: await readFile("src/features/stations/components/CreateRemoteKeyDialog.tsx", "utf8"),
  stationKeyRowsEditor: await readFile("src/features/stations/components/StationKeyRowsEditor.tsx", "utf8"),
  channelQueries: await readFile("src/lib/queries/channelQueries.ts", "utf8"),
  channelMonitoring: await readFile("src/features/channels/ChannelMonitoringTab.tsx", "utf8"),
  channelStatus: await readFile("src/features/channels/ChannelStatusTab.tsx", "utf8"),
  rustCommands: await readFile("src-tauri/src/commands/mod.rs", "utf8"),
  rustLib: await readFile("src-tauri/src/lib.rs", "utf8"),
  rustSharedCapabilities: await readFile("src-tauri/src/services/shared_capabilities.rs", "utf8"),
};

assert.ok(
  files.stationKeysApi.includes("saveStationKeyWithDefaults"),
  "stationKeys API should expose saveStationKeyWithDefaults",
);
assert.ok(
  files.groupFactsApi.includes("listStationGroupOptions"),
  "groupFacts API should expose listStationGroupOptions",
);
assert.ok(
  files.channelApi.includes("listChannelMonitorSummaries"),
  "channelMonitors API should expose listChannelMonitorSummaries",
);
assert.ok(
  files.rustCommands.includes("save_station_key_with_defaults") &&
    files.rustCommands.includes("list_station_group_options") &&
    files.rustCommands.includes("list_channel_monitor_summaries"),
  "Tauri commands should expose the three shared capabilities",
);
assert.ok(
  files.rustLib.includes("commands::save_station_key_with_defaults") &&
    files.rustLib.includes("commands::list_station_group_options") &&
    files.rustLib.includes("commands::list_channel_monitor_summaries"),
  "Tauri invoke handler should register the shared capability commands",
);

for (const [name, source] of [
  ["AddKeyPage", files.addKey],
  ["EditKeyPage", files.editKey],
]) {
  assert.ok(
    source.includes("saveStationKeyWithDefaults"),
    `${name} should save keys through saveStationKeyWithDefaults`,
  );
  assert.ok(
    !source.includes("updateStationKeyCapabilities"),
    `${name} should not persist default capabilities directly`,
  );
  assert.ok(
    !source.includes("updateStationKeyGroupBinding"),
    `${name} should not compose key save and group binding update in page code`,
  );
}

assert.ok(
  files.keyPool.includes("saveStationKeyWithDefaults"),
  "KeyPoolPage dialog fallback should use saveStationKeyWithDefaults",
);
assert.ok(
  !files.keyPool.includes("updateStationKeyCapabilities"),
  "KeyPoolPage should not persist default capabilities directly",
);

assert.ok(
  !files.createRemoteKeyDialog.includes("groupOptionValue(index)") &&
    !files.createRemoteKeyDialog.includes("Number(groupValue.replace"),
  "CreateRemoteKeyDialog should not use index-based group option values",
);
assert.ok(
  !files.stationKeyRowsEditor.includes("function normalizeGroupOptions") &&
    !files.stationKeyRowsEditor.includes("function groupOptionValue"),
  "StationKeyRowsEditor should use shared group option helpers",
);

assert.ok(
  files.channelQueries.includes("listChannelMonitorSummaries") &&
    files.channelMonitoring.includes("loadChannelMonitoringWorkspace"),
  "ChannelMonitoringTab should load monitor summaries through the shared query service",
);
assert.ok(
  files.channelQueries.includes("listChannelMonitorSummaries") &&
    files.channelStatus.includes("loadChannelStatusWorkspace"),
  "ChannelStatusTab should load monitor summaries through the shared query service",
);
assert.ok(
  !files.channelMonitoring.includes("listChannelMonitorRuns(monitor.id)") &&
    !files.channelStatus.includes("listChannelMonitorRuns(monitor.id)"),
  "channel tabs should not issue page-local per-monitor run loading",
);

assert.ok(
  files.rustSharedCapabilities.includes("group_binding_id"),
  "shared capabilities must persist the selected local group binding id",
);

assert.ok(
  files.rustSharedCapabilities.includes("group_id_hash"),
  "shared capabilities must preserve remote group identity hash as separate metadata",
);

assert.ok(
  files.rustSharedCapabilities.includes("group_name"),
  "shared capabilities must preserve group display name without using it as the primary identity",
);
