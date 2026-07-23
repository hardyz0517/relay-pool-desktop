import { beforeEach, describe, expect, it, vi } from "vitest";

const transport = vi.hoisted(() => ({
  invoke: vi.fn(),
  invokeNonIdempotent: vi.fn(),
}));

vi.mock("@/lib/bridge/transport", () => transport);

import {
  createStation,
  deleteStation,
  getSettings,
  listStations,
  reorderStations,
  updateSettings,
  updateStation,
  type CreateStationInputDto,
  type UpdateSettingsInputDto,
} from "./generated";

const stationInput: CreateStationInputDto = {
  name: "Smoke station",
  stationType: "openai-compatible",
  websiteUrl: "https://example.test",
  apiBaseUrl: "https://example.test/v1",
  apiKey: "sk-smoke-redacted",
  collectorProxyMode: "inherit",
  collectorProxyUrl: null,
  enabled: true,
  creditPerCny: 1,
  lowBalanceThresholdCny: null,
  collectionIntervalMinutes: 10,
  note: null,
};

const settingsInput = {
  localProxyPort: 8317,
  defaultRoutingStrategy: "automatic_balanced",
  collectorProxyMode: "direct",
  collectorProxyUrl: null,
  maxRateMultiplier: null,
  lowBalanceThresholdCny: 10,
  collectorIntervalMinutes: 5,
  balanceIntervalMinutes: 5,
  groupRateIntervalMinutes: 20,
  modelListIntervalMinutes: 60,
  pricingRefreshIntervalMinutes: 60,
  collectorTimeoutSeconds: 15,
  collectorMaxConcurrency: 3,
  allowDepletedFallback: false,
  developerModeEnabled: false,
} satisfies UpdateSettingsInputDto;

describe("generated settings/stations transport envelopes", () => {
  beforeEach(() => {
    transport.invoke.mockReset().mockResolvedValue(undefined);
    transport.invokeNonIdempotent.mockReset().mockResolvedValue(undefined);
  });

  it("sends every migrated command with the Tauri { input } envelope", async () => {
    await getSettings();
    await listStations();
    await updateSettings(settingsInput);
    await createStation(stationInput);
    await updateStation({ ...stationInput, id: "station-1", apiKey: null });
    await deleteStation({ id: "station-1" });
    await reorderStations({ stationIds: ["station-1"] });

    expect(transport.invoke.mock.calls).toEqual([
      ["get_settings", { input: {} }],
      ["list_stations", { input: {} }],
      ["update_settings", { input: settingsInput }],
      ["update_station", { input: { ...stationInput, id: "station-1", apiKey: null } }],
      ["delete_station", { input: { id: "station-1" } }],
      ["reorder_stations", { input: { stationIds: ["station-1"] } }],
    ]);
    expect(transport.invokeNonIdempotent).toHaveBeenCalledExactlyOnceWith(
      "create_station",
      { input: stationInput },
    );
  });
});
