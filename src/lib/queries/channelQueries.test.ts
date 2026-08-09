import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";

import { loadChannelMonitoringWorkspace, loadChannelStatusWorkspace } from "./channelQueries";

describe("channel query backend cutover", () => {
  const channels = {
    loadChannelMonitoringWorkspace: vi.fn(async () => ({
      monitors: [],
      statusWorkspace: {} as never,
      stations: [],
      keyPoolItems: [],
      templates: [],
    })),
    loadChannelStatusWorkspace: vi.fn(async () => ({
      keyPoolItems: [],
      requestLogs: [],
      stationKeyHealth: [],
      channelStatusSummaries: [],
    })),
  };

  beforeEach(() => {
    setActiveBackendClient(testBackendClient({ channels: channels as unknown as BackendClient["channels"] }));
    channels.loadChannelMonitoringWorkspace.mockClear();
    channels.loadChannelStatusWorkspace.mockClear();
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes channel workspaces through the active backend client", async () => {
    await loadChannelMonitoringWorkspace();
    await loadChannelStatusWorkspace();

    expect(channels.loadChannelMonitoringWorkspace).toHaveBeenCalledTimes(1);
    expect(channels.loadChannelStatusWorkspace).toHaveBeenCalledTimes(1);
  });
});

function testBackendClient(overrides: Partial<BackendClient>): BackendClient {
  return {
    mode: "desktop",
    settings: {} as BackendClient["settings"],
    stations: {} as BackendClient["stations"],
    stationKeys: {} as BackendClient["stationKeys"],
    alerting: {} as BackendClient["alerting"],
    collectorRuns: {} as BackendClient["collectorRuns"],
    collectors: {} as BackendClient["collectors"],
    proxy: {} as BackendClient["proxy"],
    dashboard: {} as BackendClient["dashboard"],
    runtime: {} as BackendClient["runtime"],
    dataRecovery: {} as BackendClient["dataRecovery"],
    dataMigration: {} as BackendClient["dataMigration"],
    economics: {} as BackendClient["economics"],
    groupFacts: {} as BackendClient["groupFacts"],
    pricing: {} as BackendClient["pricing"],
    routing: {} as BackendClient["routing"],
    channels: {} as BackendClient["channels"],
    updater: {} as BackendClient["updater"],
    handshake: vi.fn(async () => ({}) as never),
    ...overrides,
  };
}
