import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  invokeCommand: vi.fn(),
  listStations: vi.fn(),
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
  Channel: class<Event> {
    onmessage?: (event: Event) => void;
  },
}));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@tauri-apps/api/core", () => tauri);

import {
  bindRemoteStationKey,
  clearStationCredentials,
  createLocalStationKeyFromRemote,
  createRemoteStationKey,
  createStationKey,
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
    for (const fn of Object.values(generated)) {
      fn.mockReset().mockResolvedValue(undefined);
    }
    generated.listStations.mockResolvedValue([]);
    tauri.invoke.mockReset().mockResolvedValue(undefined);
  });

  it("routes all twenty ordinary commands through generated wrappers", async () => {
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
    expect(generated.bindRemoteStationKey).toHaveBeenCalledWith({ remoteKeyId, stationKeyId: keyId });
    expect(generated.clearStationCredentials).toHaveBeenCalledWith({ stationId });
  });
});
