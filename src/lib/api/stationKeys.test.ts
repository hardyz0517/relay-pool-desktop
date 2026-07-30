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

const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@/lib/bridge/generated", () => generated);
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
} from "./stationKeys";

describe("station key ordinary generated transport cutover", () => {
  beforeEach(() => {
    setActiveBackendClient(new DesktopBackend());
    for (const fn of Object.values(generated)) {
      fn.mockReset().mockResolvedValue(undefined);
    }
    generated.listStations.mockResolvedValue([]);
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

  it("strips output-only fields before updating a station key", async () => {
    const input = {
      id: "key-1",
      stationId: "station-1",
      name: "Fixture key",
      apiKey: null,
      enabled: true,
      priority: 0,
      maxConcurrency: 4,
      loadFactor: null,
      schedulable: true,
      groupBindingId: null,
      groupIdHash: null,
      groupName: null,
      tierLabel: null,
      rateMultiplier: 2,
      rateSource: "remote_scan",
      balanceScope: "station_key",
      status: "unchecked" as const,
      note: "created from remote key",
      apiKeyMasked: "masked-output-only",
      apiKeyPresent: true,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    };

    await updateStationKey(input);

    expect(generated.updateStationKey).toHaveBeenCalledWith({
      id: "key-1",
      stationId: "station-1",
      name: "Fixture key",
      apiKey: null,
      enabled: true,
      priority: 0,
      maxConcurrency: 4,
      loadFactor: null,
      schedulable: true,
      groupBindingId: null,
      groupIdHash: null,
      groupName: null,
      tierLabel: null,
      rateMultiplier: 2,
      rateSource: "remote_scan",
      balanceScope: "station_key",
      status: "unchecked",
      note: "created from remote key",
    });
  });
});

describe("station key demo backend unsupported cutover", () => {
  beforeEach(() => {
    setActiveBackendClient(new DemoBackend());
    for (const fn of Object.values(generated)) {
      fn.mockReset().mockResolvedValue(undefined);
    }
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

});
