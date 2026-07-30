import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";

import {
  checkForAppUpdate,
  closePendingUpdate,
  currentAppVersion,
  downloadPendingUpdate,
  installPendingUpdateAndRelaunch,
} from "./updater";

describe("updater backend cutover", () => {
  const updater = {
    currentAppVersion: vi.fn(async () => "0.3.2"),
    checkForAppUpdate: vi.fn(async () => ({ kind: "current" as const, currentVersion: "0.3.2" })),
    downloadPendingUpdate: vi.fn(async () => undefined),
    installPendingUpdateAndRelaunch: vi.fn(async () => undefined),
    closePendingUpdate: vi.fn(async () => undefined),
  };

  beforeEach(() => {
    setActiveBackendClient(testBackendClient({ updater: updater as BackendClient["updater"] }));
    for (const fn of Object.values(updater)) {
      fn.mockClear();
    }
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes updater operations through the active backend client", async () => {
    const onProgress = vi.fn();

    await expect(currentAppVersion()).resolves.toBe("0.3.2");
    await expect(checkForAppUpdate()).resolves.toEqual({ kind: "current", currentVersion: "0.3.2" });
    await downloadPendingUpdate(onProgress);
    await installPendingUpdateAndRelaunch();
    await closePendingUpdate();

    expect(updater.currentAppVersion).toHaveBeenCalledTimes(1);
    expect(updater.checkForAppUpdate).toHaveBeenCalledTimes(1);
    expect(updater.downloadPendingUpdate).toHaveBeenCalledWith(onProgress);
    expect(updater.installPendingUpdateAndRelaunch).toHaveBeenCalledTimes(1);
    expect(updater.closePendingUpdate).toHaveBeenCalledTimes(1);
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
