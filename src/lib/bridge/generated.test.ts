import { beforeEach, describe, expect, it, vi } from "vitest";

const transport = vi.hoisted(() => ({
  invoke: vi.fn(),
  invokeNonIdempotent: vi.fn(),
}));

vi.mock("@/lib/bridge/transport", () => transport);

import {
  bindRemoteStationKey,
  clearChangeEvents,
  clearRequestLogs,
  clearStationCredentials,
  createLocalStationKeyFromRemote,
  createRemoteStationKey,
  createStation,
  createStationKey,
  deleteStationKey,
  deleteStation,
  dismissChangeEvent,
  getRemoteKeyCapability,
  getLatestCollectorSnapshot,
  getSettings,
  getStationCredentials,
  listKeyPoolItems,
  listChangeEvents,
  listChangeEventsForStation,
  listBalanceSnapshots,
  listBalanceSnapshotsForStation,
  listCollectorRuns,
  listCollectorSnapshots,
  listCurrentStationBalanceSnapshots,
  listGroupRateRecords,
  listRemoteStationKeys,
  listRequestLogs,
  listStationKeys,
  listStations,
  listStationGroupBindings,
  listStationGroupOptions,
  markChangeEventRead,
  markChangeEventsRead,
  reorderKeyPool,
  reorderStationKeys,
  reorderStations,
  resolveChangeEvent,
  saveStationKeyWithDefaults,
  scanRemoteStationKeys,
  unbindRemoteStationKey,
  updateStationCredentials,
  updateStationKey,
  updateStationKeyGroupBinding,
  updateStationSession,
  updateSettings,
  updateStation,
  upsertBalanceSnapshot,
  upsertChangeEvent,
  upsertStationGroupBinding,
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

  it("sends every changes/logs command through generated envelopes", async () => {
    const input = {
      severity: "warning" as const,
      eventType: "fixture.changed",
      title: "Fixture change",
      message: "Fixture message",
      objectType: "station",
      objectId: "station-1",
      stationId: "station-1",
      stationKeyId: null,
      pricingRuleId: null,
      requestLogId: null,
      oldValueJson: null,
      newValueJson: "{}",
      impactJson: null,
      dedupeKey: "fixture-change-1",
      source: "fixture",
    };

    await listRequestLogs();
    await clearRequestLogs();
    await listChangeEvents();
    await clearChangeEvents();
    await listChangeEventsForStation({ stationId: "station-1" });
    await upsertChangeEvent(input);
    await markChangeEventRead({ id: "change-1" });
    await markChangeEventsRead({ ids: ["change-1", "change-2"] });
    await dismissChangeEvent({ id: "change-1" });
    await resolveChangeEvent({ id: "change-1" });

    expect(transport.invoke.mock.calls.slice(-10)).toEqual([
      ["list_request_logs", { input: {} }],
      ["clear_request_logs", { input: {} }],
      ["list_change_events", { input: {} }],
      ["clear_change_events", { input: {} }],
      ["list_change_events_for_station", { input: { stationId: "station-1" } }],
      ["upsert_change_event", { input }],
      ["mark_change_event_read", { input: { id: "change-1" } }],
      ["mark_change_events_read", { input: { ids: ["change-1", "change-2"] } }],
      ["dismiss_change_event", { input: { id: "change-1" } }],
      ["resolve_change_event", { input: { id: "change-1" } }],
    ]);
  });

  it("sends every collector facts/snapshots command through generated envelopes", async () => {
    const stationId = "station-1";
    const balanceInput = {
      id: null,
      stationId,
      stationKeyId: null,
      scope: "station" as const,
      value: 12.5,
      currency: "CNY",
      creditUnit: null,
      usedValue: null,
      totalValue: null,
      todayRequestCount: null,
      totalRequestCount: null,
      todayConsumption: null,
      totalConsumption: null,
      todayBaseConsumption: null,
      totalBaseConsumption: null,
      todayTokenCount: null,
      totalTokenCount: null,
      todayInputTokenCount: null,
      todayOutputTokenCount: null,
      totalInputTokenCount: null,
      totalOutputTokenCount: null,
      accountConcurrencyLimit: null,
      lowBalanceThreshold: 5,
      status: "normal" as const,
      source: "fixture",
      confidence: 0.9,
      collectedAt: "1700000000000",
    };
    const bindingInput = {
      stationId,
      stationKeyId: null,
      bindingKind: "station_group" as const,
      parentGroupBindingId: null,
      groupKeyHash: "group-hash-1",
      groupIdHash: "group-id-hash-1",
      groupName: "default",
      bindingStatus: "available" as const,
      defaultRateMultiplier: 1,
      userRateMultiplier: null,
      effectiveRateMultiplier: 1,
      inferredGroupCategory: "gpt" as const,
      groupCategoryOverride: null,
      rateSource: "fixture",
      confidence: 0.9,
      lastSeenAt: "1700000000000",
      rawJsonRedacted: null,
    };

    await listBalanceSnapshots();
    await listCurrentStationBalanceSnapshots();
    await listBalanceSnapshotsForStation({ stationId });
    await upsertBalanceSnapshot(balanceInput);
    await listStationGroupBindings({ stationId });
    await listStationGroupOptions({ stationId });
    await upsertStationGroupBinding(bindingInput);
    await listGroupRateRecords({ stationId });
    await listCollectorRuns({ stationId });
    await listCollectorSnapshots({ stationId });
    await getLatestCollectorSnapshot({ stationId });

    expect(transport.invoke.mock.calls.slice(-11)).toEqual([
      ["list_balance_snapshots", { input: {} }],
      ["list_current_station_balance_snapshots", { input: {} }],
      ["list_balance_snapshots_for_station", { input: { stationId } }],
      ["upsert_balance_snapshot", { input: balanceInput }],
      ["list_station_group_bindings", { input: { stationId } }],
      ["list_station_group_options", { input: { stationId } }],
      ["upsert_station_group_binding", { input: bindingInput }],
      ["list_group_rate_records", { input: { stationId } }],
      ["list_collector_runs", { input: { stationId } }],
      ["list_collector_snapshots", { input: { stationId } }],
      ["get_latest_collector_snapshot", { input: { stationId } }],
    ]);
  });
});
