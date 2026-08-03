import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";

import {
  clearRequestLogs,
  listRequestLogs,
  prepareLocalProxyForUpdate,
  restartLocalProxy,
  startLocalProxy,
  stopLocalProxy,
} from "./proxy";

describe("request log backend cutover", () => {
  const proxy = {
    clearRequestLogs: vi.fn(async () => undefined),
    getProxyStatus: vi.fn(async () => proxyStatus()),
    listRequestLogs: vi.fn(async () => []),
    prepareLocalProxyForUpdate: vi.fn(async () => proxyStatus()),
    restartLocalProxy: vi.fn(async () => proxyStatus()),
    startLocalProxy: vi.fn(async () => proxyStatus({ running: true, lifecycle: "running" })),
    stopLocalProxy: vi.fn(async () => proxyStatus({ running: false, lifecycle: "stopped" })),
  };

  beforeEach(() => {
    setActiveBackendClient({
      mode: "desktop",
      settings: {} as never,
      stations: {} as never,
      stationKeys: {} as never,
      changeEvents: {} as never,
      collectorRuns: {} as never,
      collectors: {} as never,
      proxy: proxy as never,
      dashboard: {} as never,
      runtime: {} as never,
      localRouting: {} as never,
      dataRecovery: {} as never,
      dataMigration: {} as never,
      economics: {} as never,
      groupFacts: {} as never,
      pricing: {} as never,
      routing: {} as never,
      channels: {} as never,
      updater: {} as never,
      handshake: vi.fn(async () => ({}) as never),
    });
    for (const fn of Object.values(proxy)) {
      fn.mockReset();
    }
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes request-log commands through the active backend client", async () => {
    await listRequestLogs();
    await clearRequestLogs();

    expect(proxy.listRequestLogs).toHaveBeenCalledTimes(1);
    expect(proxy.clearRequestLogs).toHaveBeenCalledTimes(1);
  });

  it("routes proxy lifecycle commands through the active backend client", async () => {
    await startLocalProxy();
    await stopLocalProxy();
    await restartLocalProxy();
    await prepareLocalProxyForUpdate();

    expect(proxy.startLocalProxy).toHaveBeenCalledTimes(1);
    expect(proxy.stopLocalProxy).toHaveBeenCalledTimes(1);
    expect(proxy.restartLocalProxy).toHaveBeenCalledTimes(1);
    expect(proxy.prepareLocalProxyForUpdate).toHaveBeenCalledTimes(1);
  });
});

function proxyStatus(
  overrides: Partial<{
    running: boolean;
    lifecycle: "stopped" | "starting" | "running" | "draining" | "stopping" | "failed";
  }> = {},
) {
  return {
    running: false,
    lifecycle: "stopped",
    bindAddr: "127.0.0.1",
    port: 8787,
    startedAt: null,
    lastError: null,
    activeRequests: 0,
    requestCount: 0,
    ...overrides,
  };
}
