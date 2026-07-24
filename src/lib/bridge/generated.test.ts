import { beforeEach, describe, expect, it, vi } from "vitest";

const transport = vi.hoisted(() => ({
  invoke: vi.fn(),
  invokeNonIdempotent: vi.fn(),
}));

vi.mock("@/lib/bridge/transport", () => transport);

import {
  bindRemoteStationKey,
  clearStationCredentials,
  createLocalStationKeyFromRemote,
  createRemoteStationKey,
  createStation,
  createStationKey,
  deleteStationKey,
  deleteStation,
  getRemoteKeyCapability,
  getSettings,
  getStationCredentials,
  listKeyPoolItems,
  listRemoteStationKeys,
  listStationKeys,
  listStations,
  reorderKeyPool,
  reorderStationKeys,
  reorderStations,
  saveStationKeyWithDefaults,
  scanRemoteStationKeys,
  unbindRemoteStationKey,
  updateStationCredentials,
  updateStationKey,
  updateStationKeyGroupBinding,
  updateStationSession,
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

  it("sends every ordinary station-key command through its generated transport policy", async () => {
    const stationId = "station-1";
    const keyId = "key-1";
    const remoteKeyId = "remote-1";
    const createInput = {
      stationId,
      name: "Fixture key",
      apiKey: "fixture-not-a-real-api-key",
      enabled: true,
      groupName: null,
      tierLabel: null,
      note: null,
    };
    const updateInput = {
      ...createInput,
      id: keyId,
      apiKey: null,
      priority: 0,
      maxConcurrency: 3,
      schedulable: true,
      status: "unchecked" as const,
    };
    const createRemoteInput = {
      stationId,
      name: "Fixture remote key",
      groupBindingId: null,
      groupIdHash: null,
      groupName: null,
    };
    const credentialsInput = {
      stationId,
      loginUsername: "fixture-user",
      loginPassword: "fixture-not-a-real-password",
      rememberPassword: false,
    };
    const sessionInput = {
      stationId,
      accessToken: "fixture-not-a-real-access-token",
      refreshToken: null,
      cookie: null,
      newapiUserId: null,
      tokenExpiresAt: null,
    };
    const saveInput = {
      mode: "create" as const,
      id: null,
      stationId,
      name: "Fixture defaults",
      apiKey: "fixture-not-a-real-api-key",
      enabled: true,
      groupSelection: { kind: "clear" as const },
    };

    await listStationKeys({ stationId });
    await updateStationKey(updateInput);
    await updateStationKeyGroupBinding({ stationKeyId: keyId, groupBindingId: "group-1" });
    await deleteStationKey({ id: keyId });
    await reorderStationKeys({ stationId, keyIds: [keyId] });
    await getRemoteKeyCapability({ stationId });
    await listRemoteStationKeys({ stationId });
    await scanRemoteStationKeys({ stationId });
    await bindRemoteStationKey({ remoteKeyId, stationKeyId: keyId });
    await unbindRemoteStationKey({ remoteKeyId, stationId });
    await listKeyPoolItems();
    await reorderKeyPool({ keyIds: [keyId] });
    await getStationCredentials({ stationId });
    await updateStationCredentials(credentialsInput);
    await updateStationSession(sessionInput);
    await clearStationCredentials({ stationId });
    await createStationKey(createInput);
    await saveStationKeyWithDefaults(saveInput);
    await createRemoteStationKey(createRemoteInput);
    await createLocalStationKeyFromRemote({ remoteKeyId, stationId });

    expect(transport.invoke.mock.calls).toEqual([
      ["list_station_keys", { input: { stationId } }],
      ["update_station_key", { input: updateInput }],
      ["update_station_key_group_binding", { input: { stationKeyId: keyId, groupBindingId: "group-1" } }],
      ["delete_station_key", { input: { id: keyId } }],
      ["reorder_station_keys", { input: { stationId, keyIds: [keyId] } }],
      ["get_remote_key_capability", { input: { stationId } }],
      ["list_remote_station_keys", { input: { stationId } }],
      ["scan_remote_station_keys", { input: { stationId } }],
      ["bind_remote_station_key", { input: { remoteKeyId, stationKeyId: keyId } }],
      ["unbind_remote_station_key", { input: { remoteKeyId, stationId } }],
      ["list_key_pool_items", { input: {} }],
      ["reorder_key_pool", { input: { keyIds: [keyId] } }],
      ["get_station_credentials", { input: { stationId } }],
      ["update_station_credentials", { input: credentialsInput }],
      ["update_station_session", { input: sessionInput }],
      ["clear_station_credentials", { input: { stationId } }],
    ]);
    expect(transport.invokeNonIdempotent.mock.calls).toEqual([
      ["create_station_key", { input: createInput }],
      ["save_station_key_with_defaults", { input: saveInput }],
      ["create_remote_station_key", { input: createRemoteInput }],
      ["create_local_station_key_from_remote", { input: { remoteKeyId, stationId } }],
    ]);
  });
});
