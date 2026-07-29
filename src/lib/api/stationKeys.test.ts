import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  chooseDataDir: vi.fn(),
  createStation: vi.fn(),
  invokeCommand: vi.fn(),
  deleteStation: vi.fn(),
  getLocalAccessKey: vi.fn(),
  getRuntimeContractInfo: vi.fn(),
  getSettings: vi.fn(),
  importRelayPoolToCcswitch: vi.fn(),
  listStationEndpointHealth: vi.fn(),
  listStations: vi.fn(),
  openExternalUrl: vi.fn(),
  pingStationEndpoint: vi.fn(),
  reorderStations: vi.fn(),
  resetDataDir: vi.fn(),
  updateLocalAccessKey: vi.fn(),
  updateSettings: vi.fn(),
  updateStation: vi.fn(),
  listStationKeys: vi.fn(),
  createStationKey: vi.fn(),
  updateStationKey: vi.fn(),
  deleteStationKey: vi.fn(),
  reorderStationKeys: vi.fn(),
  listKeyPoolItems: vi.fn(),
  reorderKeyPool: vi.fn(),
  getRemoteKeyCapability: vi.fn(),
  listRemoteStationKeys: vi.fn(),
  scanRemoteStationKeys: vi.fn(),
  createRemoteStationKey: vi.fn(),
  createLocalStationKeyFromRemote: vi.fn(),
  deleteRemoteStationKey: vi.fn(),
  bindRemoteStationKey: vi.fn(),
  unbindRemoteStationKey: vi.fn(),
  getStationCredentials: vi.fn(),
  updateStationCredentials: vi.fn(),
  updateStationSession: vi.fn(),
  clearStationCredentials: vi.fn(),
  saveStationKeyWithDefaults: vi.fn(),
  updateStationKeyGroupBinding: vi.fn(),
}));

const streamingAdapter = vi.hoisted(() => ({
  invokeStationKeyConnectivityStream: vi.fn(),
}));

const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  Channel: class<Event> {
    onmessage?: (event: Event) => void;
  },
}));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/streamingAdapter", () => streamingAdapter);
vi.mock("@tauri-apps/api/core", () => tauri);

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import { DemoBackendUnsupportedError } from "@/lib/bridge/BackendClient";
import { DemoBackend } from "@/lib/bridge/DemoBackend";
import { DesktopBackend } from "@/lib/bridge/DesktopBackend";
import {
  bindRemoteStationKey,
  clearStationCredentials,
  createLocalStationKeyFromRemote,
  createRemoteStationKey,
  createStationKey,
  deleteRemoteStationKey,
  deleteStationKey,
  getRemoteKeyCapability,
  getStationCredentials,
  listKeyPoolItems,
  listRemoteStationKeys,
  listStationKeys,
  reorderKeyPool,
  reorderStationKeys,
  saveStationKeyWithDefaults,
  scanRemoteStationKeys,
  unbindRemoteStationKey,
  updateStationCredentials,
  updateStationKey,
  updateStationKeyGroupBinding,
  updateStationSession,
  testStationKeyConnectivity,
} from "./stationKeys";

describe("station key ordinary generated transport cutover", () => {
  beforeEach(() => {
    setActiveBackendClient(new DesktopBackend());
    for (const fn of Object.values(generated)) {
      fn.mockReset().mockResolvedValue(undefined);
    }
    generated.listStations.mockResolvedValue([]);
    streamingAdapter.invokeStationKeyConnectivityStream.mockReset().mockResolvedValue({
      stationKeyId: "key-1",
      ok: true,
      statusCode: 200,
      durationMs: 42,
      model: "gpt-4o-mini",
      message: "ok",
      responseMode: "stream",
      streamFallbackReason: null,
    });
    tauri.invoke.mockReset().mockResolvedValue(undefined);
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes all ordinary commands through generated wrappers", async () => {
    const stationId = "station-1";
    const keyId = "key-1";
    const remoteKeyId = "remote-1";
    const keyInput = {
      stationId,
      name: "Fixture key",
      apiKey: "sk-test-obviously-fake",
      enabled: true,
      groupName: null,
      tierLabel: null,
      note: null,
    };

    await listStationKeys(stationId);
    await createStationKey(keyInput);
    await updateStationKey({
      ...keyInput,
      id: keyId,
      apiKey: null,
      priority: 0,
      maxConcurrency: 3,
      loadFactor: null,
      schedulable: true,
      groupBindingId: null,
      groupIdHash: null,
      rateMultiplier: null,
      rateSource: null,
      status: "unchecked",
    });
    await deleteStationKey(keyId);
    await reorderStationKeys(stationId, [keyId]);
    await listKeyPoolItems();
    await reorderKeyPool([keyId]);
    await getRemoteKeyCapability(stationId);
    await listRemoteStationKeys(stationId);
    await scanRemoteStationKeys(stationId);
    await createRemoteStationKey({
      stationId,
      name: "Fixture remote key",
      groupBindingId: null,
      groupIdHash: null,
      groupName: null,
    });
    await createLocalStationKeyFromRemote(remoteKeyId, stationId);
    await deleteRemoteStationKey(remoteKeyId, stationId);
    await bindRemoteStationKey(remoteKeyId, keyId);
    await unbindRemoteStationKey(remoteKeyId, stationId);
    await getStationCredentials(stationId);
    await updateStationCredentials({
      stationId,
      loginUsername: "fixture-user",
      loginPassword: "fixture-password-obviously-fake",
      rememberPassword: false,
    });
    await updateStationSession({
      stationId,
      accessToken: "fixture-access-token-obviously-fake",
      refreshToken: null,
      cookie: null,
      newapiUserId: null,
      tokenExpiresAt: null,
    });
    await clearStationCredentials(stationId);
    await saveStationKeyWithDefaults({
      mode: "create",
      id: null,
      stationId,
      name: "Fixture defaults",
      apiKey: "sk-test-obviously-fake",
      enabled: true,
      groupSelection: { kind: "clear" },
    });
    await updateStationKeyGroupBinding(keyId, "group-1");

    expect(generated.listStationKeys).toHaveBeenCalledWith({ stationId });
    expect(generated.createStationKey).toHaveBeenCalledWith(keyInput);
    expect(generated.deleteStationKey).toHaveBeenCalledWith({ id: keyId });
    expect(generated.reorderStationKeys).toHaveBeenCalledWith({ stationId, keyIds: [keyId] });
    expect(generated.listKeyPoolItems).toHaveBeenCalledWith();
    expect(generated.createLocalStationKeyFromRemote).toHaveBeenCalledWith({ remoteKeyId, stationId });
    expect(generated.deleteRemoteStationKey).toHaveBeenCalledWith({ remoteKeyId, stationId });
    expect(generated.bindRemoteStationKey).toHaveBeenCalledWith({ remoteKeyId, stationKeyId: keyId });
    expect(generated.clearStationCredentials).toHaveBeenCalledWith({ stationId });
  });

  it("routes connectivity probes through the desktop streaming adapter", async () => {
    const onEvent = vi.fn();

    await expect(testStationKeyConnectivity("key-1", "gpt-4o-mini", { onEvent })).resolves.toMatchObject({
      stationKeyId: "key-1",
      responseMode: "stream",
    });

    expect(streamingAdapter.invokeStationKeyConnectivityStream).toHaveBeenCalledWith(
      { stationKeyId: "key-1", model: "gpt-4o-mini" },
      { onEvent },
    );
  });
});

describe("station key demo backend unsupported cutover", () => {
  beforeEach(() => {
    setActiveBackendClient(new DemoBackend());
    for (const fn of Object.values(generated)) {
      fn.mockReset().mockResolvedValue(undefined);
    }
    streamingAdapter.invokeStationKeyConnectivityStream.mockReset().mockResolvedValue(undefined);
    tauri.invoke.mockReset().mockResolvedValue(undefined);
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("does not fake key-pool list success in demo mode", async () => {
    await expect(listKeyPoolItems()).rejects.toBeInstanceOf(DemoBackendUnsupportedError);

    expect(generated.listKeyPoolItems).not.toHaveBeenCalled();
    expect(tauri.invoke).not.toHaveBeenCalled();
  });

  it("does not fake connectivity success in demo mode", async () => {
    await expect(testStationKeyConnectivity("key-1", "gpt-4o-mini")).rejects.toMatchObject({
      code: "unsupported",
      capability: "station_keys.connectivity",
    });

    expect(streamingAdapter.invokeStationKeyConnectivityStream).not.toHaveBeenCalled();
    expect(tauri.invoke).not.toHaveBeenCalled();
  });
});
