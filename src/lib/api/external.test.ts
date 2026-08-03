import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";

import { openExternalUrl } from "./external";

describe("external URL backend cutover", () => {
  const stations = {
    openStationWebsite: vi.fn(async () => undefined),
  };

  beforeEach(() => {
    setActiveBackendClient({
      mode: "desktop",
      settings: {} as never,
      stations: stations as never,
      stationKeys: {} as never,
      changeEvents: {} as never,
      collectorRuns: {} as never,
      collectors: {} as never,
      proxy: {} as never,
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
    stations.openStationWebsite.mockReset().mockResolvedValue(undefined);
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes URL opening through the active backend client", async () => {
    await openExternalUrl("https://example.test");

    expect(stations.openStationWebsite).toHaveBeenCalledWith("https://example.test");
  });
});
