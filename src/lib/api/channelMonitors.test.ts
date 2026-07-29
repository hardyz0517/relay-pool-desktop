import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";

import {
  createChannelMonitor,
  createChannelMonitorTemplate,
  deleteChannelMonitor,
  deleteChannelMonitorTemplate,
  duplicateChannelMonitorTemplate,
  listChannelMonitorRuns,
  listChannelMonitorSummaries,
  listChannelMonitorTemplates,
  listChannelMonitors,
  listChannelStatusSummaries,
  runChannelMonitorNow,
  updateChannelMonitor,
  updateChannelMonitorTemplate,
} from "./channelMonitors";

describe("channel monitor backend cutover", () => {
  const channels = {
    listChannelMonitors: vi.fn(async () => []),
    listChannelMonitorSummaries: vi.fn(async () => []),
    listChannelStatusSummaries: vi.fn(async () => []),
    createChannelMonitor: vi.fn(async (input) => ({ id: "monitor-1", createdAt: "now", updatedAt: "now", ...input })),
    updateChannelMonitor: vi.fn(async (input) => ({ createdAt: "now", updatedAt: "now", ...input })),
    deleteChannelMonitor: vi.fn(async () => undefined),
    runChannelMonitorNow: vi.fn(async () => []),
    listChannelMonitorRuns: vi.fn(async () => []),
    listChannelMonitorTemplates: vi.fn(async () => []),
    createChannelMonitorTemplate: vi.fn(async (input) => ({
      id: "template-1",
      builtIn: false,
      createdAt: "now",
      updatedAt: "now",
      ...input,
    })),
    updateChannelMonitorTemplate: vi.fn(async (input) => ({
      builtIn: false,
      createdAt: "now",
      updatedAt: "now",
      ...input,
    })),
    duplicateChannelMonitorTemplate: vi.fn(async () => ({} as never)),
    deleteChannelMonitorTemplate: vi.fn(async () => undefined),
    loadChannelMonitoringWorkspace: vi.fn(async () => ({} as never)),
    loadChannelStatusWorkspace: vi.fn(async () => ({} as never)),
  };

  beforeEach(() => {
    setActiveBackendClient(testBackendClient({ channels: channels as BackendClient["channels"] }));
    for (const fn of Object.values(channels)) {
      fn.mockClear();
    }
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes monitor reads and mutations through the active backend client", async () => {
    const monitorInput = {
      name: "fixture monitor",
      targetType: "station_key",
      stationId: "station-1",
      stationKeyId: "key-1",
      templateId: "template-1",
      fallbackModels: [] as string[],
      intervalSeconds: 300,
      jitterSeconds: 30,
      timeoutSeconds: 30,
      maxConcurrency: 1,
      consecutiveFailureThreshold: 3,
      enabled: true,
      note: null,
    } as const;
    const templateInput = {
      name: "fixture template",
      endpointKind: "chat_completions",
      method: "POST",
      path: "/v1/chat/completions",
      requestBodyJson: "{}",
      enabled: true,
      note: null,
    } as const;

    await listChannelMonitors();
    await listChannelMonitorSummaries({ runLimit: 5, runSince: "2026-07-22T00:00:00.000Z" });
    await listChannelStatusSummaries();
    await createChannelMonitor(monitorInput);
    await updateChannelMonitor({ ...monitorInput, id: "monitor-1" });
    await deleteChannelMonitor("monitor-1");
    await runChannelMonitorNow("monitor-1");
    await listChannelMonitorRuns("monitor-1");
    await listChannelMonitorTemplates();
    await createChannelMonitorTemplate(templateInput);
    await updateChannelMonitorTemplate({ ...templateInput, id: "template-1" });
    await duplicateChannelMonitorTemplate("template-1");
    await deleteChannelMonitorTemplate("template-1");

    expect(channels.listChannelMonitors).toHaveBeenCalledTimes(1);
    expect(channels.listChannelMonitorSummaries).toHaveBeenCalledWith({
      runLimit: 5,
      runSince: "2026-07-22T00:00:00.000Z",
    });
    expect(channels.listChannelStatusSummaries).toHaveBeenCalledTimes(1);
    expect(channels.createChannelMonitor).toHaveBeenCalledWith(monitorInput);
    expect(channels.updateChannelMonitor).toHaveBeenCalledWith({ ...monitorInput, id: "monitor-1" });
    expect(channels.deleteChannelMonitor).toHaveBeenCalledWith("monitor-1");
    expect(channels.runChannelMonitorNow).toHaveBeenCalledWith("monitor-1");
    expect(channels.listChannelMonitorRuns).toHaveBeenCalledWith("monitor-1");
    expect(channels.listChannelMonitorTemplates).toHaveBeenCalledTimes(1);
    expect(channels.createChannelMonitorTemplate).toHaveBeenCalledWith(templateInput);
    expect(channels.updateChannelMonitorTemplate).toHaveBeenCalledWith({ ...templateInput, id: "template-1" });
    expect(channels.duplicateChannelMonitorTemplate).toHaveBeenCalledWith("template-1");
    expect(channels.deleteChannelMonitorTemplate).toHaveBeenCalledWith("template-1");
  });
});

function testBackendClient(overrides: Partial<BackendClient>): BackendClient {
  return {
    mode: "desktop",
    settings: {} as BackendClient["settings"],
    stations: {} as BackendClient["stations"],
    stationKeys: {} as BackendClient["stationKeys"],
    changeEvents: {} as BackendClient["changeEvents"],
    collectorRuns: {} as BackendClient["collectorRuns"],
    collectors: {} as BackendClient["collectors"],
    proxy: {} as BackendClient["proxy"],
    runtime: {} as BackendClient["runtime"],
    localRouting: {} as BackendClient["localRouting"],
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
