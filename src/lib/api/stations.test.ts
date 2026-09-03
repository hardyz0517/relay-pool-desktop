import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  createStation: vi.fn(),
  deleteStation: vi.fn(),
  listStationEndpointHealth: vi.fn(),
  listStations: vi.fn(),
  loadStationAssets: vi.fn(),
  loadStationDetail: vi.fn(),
  getStationAssetsRevision: vi.fn(),
  openExternalUrl: vi.fn(),
  pingStationEndpoint: vi.fn(),
  reorderStations: vi.fn(),
  updateStation: vi.fn(),
}));
const transport = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/transport", () => transport);

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import { DesktopBackend } from "@/lib/bridge/DesktopBackend";
import {
  getStationAssetsRevision,
  loadStationAssets,
  loadStationDetail,
  openStationWebsite,
  pingStationEndpoint,
} from "./stations";

describe("station endpoint ping generated transport cutover", () => {
  beforeEach(() => {
    setActiveBackendClient(new DesktopBackend());
    generated.pingStationEndpoint.mockReset().mockResolvedValue({
      stationId: "station-1",
      ok: true,
      status: "success",
      latencyMs: 12,
      checkedAt: "2026-07-22T00:00:00.000Z",
      errorSummary: null,
    });
    generated.openExternalUrl.mockReset().mockResolvedValue(undefined);
    generated.loadStationAssets.mockReset().mockResolvedValue({
      schemaVersion: 1,
      generatedAtMs: 10,
      domainRevision: 7,
      page: { limit: 500, returned: 1, nextCursor: null },
      data: {
        rows: [{
          station: { id: "station-1", enabled: true, collectorProxyMode: "inherit" },
          keys: [],
          groupIdentityHashes: [],
          collectionSummary: { status: "healthy", reasonCodes: [], revision: 4 },
          authorizationSummary: {
            status: "valid",
            credentialRevision: 3,
            reasonCode: null,
            revision: 5,
          },
        }],
      },
    });
    generated.getStationAssetsRevision.mockReset().mockResolvedValue({
      scope: "read_model:station_assets",
      revision: 7,
    });
    generated.loadStationDetail.mockReset().mockResolvedValue({
      schemaVersion: 1,
      generatedAtMs: 10,
      domainRevision: 7,
      page: { limit: 1, returned: 1, nextCursor: null },
      data: {
        asset: {
          station: { id: "station-1", enabled: true, collectorProxyMode: "inherit" },
          keys: [],
          groupIdentityHashes: [],
          collectionSummary: { status: "healthy", reasonCodes: [], revision: 4 },
          authorizationSummary: { status: "valid", credentialRevision: 3, reasonCode: null, revision: 5 },
        },
        credentials: { stationId: "station-1" },
        groupBindings: [{
          id: "binding-1",
          inferredGroupCategory: "unexpected-category",
          groupCategoryOverride: null,
        }],
        groupRates: [{
          id: "rate-1",
          inferredGroupCategory: "unexpected-category",
        }],
        collectorRuns: [],
        latestSnapshot: null,
        balances: [],
        incidents: [{
          id: "incident-1",
          eventType: "collector_failed",
          lifecycleState: "open",
          severity: "unexpected-severity",
          groupName: null,
          stationId: "station-1",
          episodeNumber: 1,
          occurrenceCount: 1,
          lastSeenAtMs: 10,
        }],
        limits: { groupBindings: 500, groupRates: 500, collectorRuns: 100, balances: 200, incidents: 100 },
      },
    });
    transport.invoke.mockReset().mockResolvedValue(undefined);
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes endpoint ping through the generated non-idempotent wrapper", async () => {
    await pingStationEndpoint("station-1");

    expect(generated.pingStationEndpoint).toHaveBeenCalledWith({ stationId: "station-1" });
    expect(transport.invoke).not.toHaveBeenCalled();
  });

  it("routes website opening through the generated external URL wrapper", async () => {
    await openStationWebsite("https://example.test");

    expect(generated.openExternalUrl).toHaveBeenCalledWith({ url: "https://example.test" });
    expect(transport.invoke).not.toHaveBeenCalled();
  });

  it("loads the typed Station asset envelope through generated bindings", async () => {
    const envelope = await loadStationAssets();
    const revision = await getStationAssetsRevision();

    expect(generated.loadStationAssets).toHaveBeenCalledWith();
    expect(generated.getStationAssetsRevision).toHaveBeenCalledWith();
    expect(envelope.domainRevision).toBe(7);
    expect(envelope.data.rows[0].station.status).toBe("unchecked");
    expect(envelope.data.rows[0].collectionSummary.status).toBe("healthy");
    expect(revision).toEqual({ scope: "read_model:station_assets", revision: 7 });
  });

  it("loads the typed Station detail envelope through generated bindings", async () => {
    const envelope = await loadStationDetail("station-1");

    expect(generated.loadStationDetail).toHaveBeenCalledWith({ stationId: "station-1" });
    expect(envelope.data.asset.station.status).toBe("unchecked");
    expect(envelope.data.groupBindings[0].inferredGroupCategory).toBeNull();
    expect(envelope.data.groupRates[0].inferredGroupCategory).toBeNull();
    expect(envelope.data.incidents[0].severity).toBe("info");
    expect(envelope.data.limits.collectorRuns).toBe(100);
  });
});
