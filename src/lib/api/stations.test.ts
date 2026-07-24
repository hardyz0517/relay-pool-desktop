import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  createStation: vi.fn(),
  deleteStation: vi.fn(),
  listStationEndpointHealth: vi.fn(),
  listStations: vi.fn(),
  openExternalUrl: vi.fn(),
  pingStationEndpoint: vi.fn(),
  reorderStations: vi.fn(),
  updateStation: vi.fn(),
}));
const transport = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/transport", () => transport);

import { openStationWebsite, pingStationEndpoint } from "./stations";

describe("station endpoint ping generated transport cutover", () => {
  beforeEach(() => {
    generated.pingStationEndpoint.mockReset().mockResolvedValue(undefined);
    generated.openExternalUrl.mockReset().mockResolvedValue(undefined);
    transport.invoke.mockReset().mockResolvedValue(undefined);
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
});
