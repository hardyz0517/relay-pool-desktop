import { describe, expect, it } from "vitest";
import type { StationGroupDraft } from "../../components/StationGroupRowsEditor";
import type { StationKeyDraft, StationKeyGroupOption } from "../../components/StationKeyRowsEditor";
import type { StationGroupBinding } from "@/lib/types/groupFacts";
import type { RemoteStationKey, StationKey } from "@/lib/types/stationKeys";
import {
  bindableLocalKeysForRemote,
  collectRemoteGroupOptions,
  dedupeGroupRows,
  groupBindingsToDrafts,
  legacyRemoteLocalKeyNote,
  remoteLocalKeyNote,
  resolveRemoteCreatedLocalKeyIds,
  stationKeyToUpdateInput,
  validateGroupRows,
  validateKeyRows,
} from "./keyGroupModel";

function stationKey(overrides: Partial<StationKey> = {}): StationKey {
  return {
    id: "key-1",
    stationId: "station-1",
    name: "Default Key",
    apiKeyMasked: "sk-***",
    apiKeyPresent: true,
    enabled: true,
    priority: 0,
    maxConcurrency: 4,
    loadFactor: null,
    schedulable: true,
    groupBindingId: null,
    groupIdHash: null,
    groupName: null,
    tierLabel: null,
    rateMultiplier: null,
    manualRateMultiplier: null,
    manualRateUpdatedAt: null,
    rateSource: null,
    rateCollectedAt: null,
    balanceScope: null,
    status: "unchecked",
    lastCheckedAt: null,
    lastUsedAt: null,
    note: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function remoteKey(overrides: Partial<RemoteStationKey> = {}): RemoteStationKey {
  return {
    id: "remote-1",
    stationId: "station-1",
    remoteKeyIdHash: "remote-hash",
    remoteKeyName: "Remote Key",
    apiKeyMasked: "sk-remote",
    apiKeyFingerprint: null,
    groupIdHash: "group-hash",
    groupName: "default",
    tierLabel: null,
    rateMultiplier: 2,
    rateSource: "remote_scan",
    createdAt: null,
    lastUsedAt: null,
    rawSource: "fixture",
    matchStatus: "unbound",
    matchedStationKeyId: null,
    matchConfidence: 0,
    collectedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function groupDraft(overrides: Partial<StationGroupDraft> = {}): StationGroupDraft {
  return {
    clientId: "draft-1",
    groupBindingId: null,
    groupKeyHash: "",
    groupIdHash: null,
    groupName: "default",
    rateMultiplier: "1",
    inferredGroupCategory: "unknown",
    groupCategoryOverride: null,
    source: "manual",
    deleteRequested: false,
    ...overrides,
  };
}

function stationGroupBinding(overrides: Partial<StationGroupBinding> = {}): StationGroupBinding {
  return {
    id: "binding-1",
    stationId: "station-1",
    stationKeyId: null,
    bindingKind: "station_group",
    parentGroupBindingId: null,
    groupKeyHash: "remote:group-hash",
    groupIdHash: "group-hash",
    groupName: "default",
    bindingStatus: "available",
    defaultRateMultiplier: 1.5,
    userRateMultiplier: null,
    effectiveRateMultiplier: 1.5,
    inferredGroupCategory: "unknown",
    groupCategoryOverride: null,
    rateSource: "remote_scan",
    confidence: 0.95,
    lastSeenAt: null,
    lastCheckedAt: null,
    lastRateChangedAt: null,
    rawJsonRedacted: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("add provider key/group model", () => {
  it("hydrates only collected station group bindings into editable drafts", () => {
    const drafts = groupBindingsToDrafts([
      stationGroupBinding(),
      stationGroupBinding({ id: "disabled", bindingStatus: "disabled", groupName: "old" }),
    ], []);

    expect(drafts).toHaveLength(1);
    expect(drafts[0]).toMatchObject({
      groupBindingId: "binding-1",
      groupIdHash: "group-hash",
      groupName: "default",
      rateMultiplier: "1.5",
      source: "remote",
    });
  });

  it("dedupes group drafts while preserving the stable client id", () => {
    expect(
      dedupeGroupRows([
        groupDraft({ clientId: "manual", groupName: "default", rateMultiplier: "1" }),
        groupDraft({
          clientId: "remote",
          groupName: "default",
          groupIdHash: "group-hash",
          rateMultiplier: "2",
          source: "remote",
        }),
      ]),
    ).toEqual([
      expect.objectContaining({
        clientId: "manual",
        groupIdHash: "group-hash",
        groupName: "default",
        rateMultiplier: "2",
        source: "remote",
      }),
    ]);
  });

  it("validates key and group drafts before save controllers run", () => {
    const invalidKey: StationKeyDraft = {
      clientId: "key-draft",
      id: null,
      name: "named but secretless",
      apiKey: "",
      groupBindingId: null,
      groupIdHash: null,
      groupName: "",
      rateMultiplier: "",
      enabled: true,
      note: "",
      deleteRequested: false,
    };

    expect(() => validateKeyRows([invalidKey])).toThrow("新增密钥请填写密钥内容");
    expect(() => validateGroupRows([groupDraft({ groupName: "", rateMultiplier: "abc" })])).toThrow("请填写分组名称");
  });

  it("builds remote group options and resolves switch-created local keys by note", () => {
    const remote = remoteKey();
    const groups = collectRemoteGroupOptions([remote, remoteKey({ id: "remote-2" })], 1);
    const localKey = stationKey({ id: "local-1", note: remoteLocalKeyNote(remote) });

    expect(groups).toHaveLength(1);
    expect(groups[0]).toMatchObject<Partial<StationKeyGroupOption>>({
      groupIdHash: "group-hash",
      groupName: "default",
      selectableForRemoteKey: true,
    });
    expect(resolveRemoteCreatedLocalKeyIds([remote], [localKey])).toEqual({ "remote-1": "local-1" });
  });

  it("recognizes legacy switch-created keys only when they still match the remote key", () => {
    const matchedRemote = remoteKey({
      matchStatus: "matched",
      matchedStationKeyId: "legacy-local",
    });
    const legacyLocal = stationKey({
      id: "legacy-local",
      note: legacyRemoteLocalKeyNote,
    });

    expect(resolveRemoteCreatedLocalKeyIds([matchedRemote], [legacyLocal])).toEqual({
      "remote-1": "legacy-local",
    });
    expect(
      resolveRemoteCreatedLocalKeyIds(
        [matchedRemote],
        [stationKey({ id: "legacy-local", note: "手工创建" })],
      ),
    ).toEqual({});
    expect(
      resolveRemoteCreatedLocalKeyIds(
        [remoteKey({ matchedStationKeyId: null })],
        [legacyLocal],
      ),
    ).toEqual({});
  });

  it("offers only unclaimed local keys when binding a remote key", () => {
    const remotes = [
      remoteKey({ id: "remote-1", matchedStationKeyId: "local-1", matchStatus: "matched" }),
      remoteKey({ id: "remote-2", matchedStationKeyId: null }),
    ];
    const locals = [stationKey({ id: "local-1" }), stationKey({ id: "local-2" })];

    expect(bindableLocalKeysForRemote("remote-2", remotes, locals).map((key) => key.id)).toEqual([
      "local-2",
    ]);
    expect(bindableLocalKeysForRemote("remote-1", remotes, locals).map((key) => key.id)).toEqual([
      "local-1",
      "local-2",
    ]);
  });

  it("builds an update payload without read-only station key fields", () => {
    const input = stationKeyToUpdateInput(stationKey(), {
      rateMultiplier: 2,
      note: "created from remote key",
    });

    expect(input).toEqual({
      id: "key-1",
      stationId: "station-1",
      name: "Default Key",
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
      manualRateMultiplier: null,
      rateSource: null,
      balanceScope: null,
      status: "unchecked",
      note: "created from remote key",
    });
    expect(input).not.toHaveProperty("apiKeyMasked");
    expect(input).not.toHaveProperty("createdAt");
    expect(input).not.toHaveProperty("updatedAt");
  });
});
