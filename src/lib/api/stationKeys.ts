import { invoke } from "@tauri-apps/api/core";
import {
  getStationKeyCapabilities,
  updateStationKeyCapabilities,
} from "@/lib/api/routing";
import { listStations } from "@/lib/api/stations";
import type {
  CreateLocalStationKeyFromRemoteResult,
  CreateRemoteStationKeyInput,
  CreateRemoteStationKeyResult,
  CreateStationKeyInput,
  KeyPoolItem,
  RemoteKeyCapability,
  RemoteKeyScanResult,
  RemoteStationKey,
  StationKeyConnectivityTestResult,
  StationCredentials,
  StationKey,
  SaveStationKeyWithDefaultsInput,
  SaveStationKeyWithDefaultsResult,
  UpdateStationSessionInput,
  UpdateStationKeyInput,
} from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import type {
  StationKeyCapabilities,
  UpdateStationKeyCapabilitiesInput,
} from "@/lib/types/routing";

const memoryKeys = new Map<string, StationKey[]>();
const memoryRemoteKeys = new Map<string, RemoteStationKey[]>();
const memoryCredentials = new Map<string, StationCredentials>();
let memoryKeyPool: KeyPoolItem[] = [];
let memoryIdCounter = 0;

export const KEY_POOL_ITEMS_UPDATED_EVENT = "relay-pool:key-pool-items-updated";

function notifyKeyPoolItemsUpdated() {
  if (typeof window === "undefined") {
    return;
  }
  window.dispatchEvent(new CustomEvent(KEY_POOL_ITEMS_UPDATED_EVENT));
}

function withKeyPoolItemsInvalidation<T>(request: Promise<T>): Promise<T> {
  return request.then((result) => {
    notifyKeyPoolItemsUpdated();
    return result;
  });
}

export function listStationKeys(stationId: string) {
  return invoke<StationKey[]>("list_station_keys", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryKeys.get(stationId) ?? [];
    }
    throw error;
  });
}

export function getRemoteKeyCapability(stationId: string): Promise<RemoteKeyCapability> {
  return invoke<RemoteKeyCapability>("get_remote_key_capability", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryRemoteKeyCapability(stationId);
    }
    throw error;
  });
}

export function listRemoteStationKeys(stationId: string): Promise<RemoteStationKey[]> {
  return invoke<RemoteStationKey[]>("list_remote_station_keys", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryRemoteKeys.get(stationId) ?? [];
    }
    throw error;
  });
}

export function scanRemoteStationKeys(stationId: string): Promise<RemoteKeyScanResult> {
  return invoke<RemoteKeyScanResult>("scan_remote_station_keys", { stationId }).catch(async (error) => {
    if (isInvokeUnavailable(error)) {
      const keys = memoryRemoteKeys.get(stationId) ?? [];
      return {
        stationId,
        capability: await memoryRemoteKeyCapability(stationId),
        keys,
        syncedStationKeyIds: keys.flatMap((key) => key.matchedStationKeyId ? [key.matchedStationKeyId] : []),
        message: "浏览器预览模式：使用本地临时远端密钥数据。",
      };
    }
    throw error;
  });
}

export function createRemoteStationKey(input: CreateRemoteStationKeyInput): Promise<CreateRemoteStationKeyResult> {
  return withKeyPoolItemsInvalidation(invoke<CreateRemoteStationKeyResult>("create_remote_station_key", { input }).catch(async (error) => {
    if (isInvokeUnavailable(error)) {
      const fullKeyOnce = `sk-browser-preview-${nextMemoryId("secret")}`;
      const stationKey = await createStationKey({
        stationId: input.stationId,
        name: input.name,
        apiKey: fullKeyOnce,
        enabled: true,
        groupBindingId: input.groupBindingId,
        groupIdHash: input.groupIdHash,
        groupName: input.groupName,
        tierLabel: null,
        rateMultiplier: null,
        rateSource: null,
        balanceScope: null,
        note: "浏览器预览模式创建的远端密钥",
      });
      const remoteKey = memoryRemoteKeyFromStationKey(stationKey, input);
      memoryRemoteKeys.set(input.stationId, [
        remoteKey,
        ...(memoryRemoteKeys.get(input.stationId) ?? []).filter((key) => key.id !== remoteKey.id),
      ]);
      return {
        remoteKey,
        stationKey,
        message: "浏览器预览模式：已创建本地临时密钥，真实远端创建将在桌面端执行。",
        fullKeyOnce: null,
      };
    }
    throw error;
  }));
}

export function createLocalStationKeyFromRemote(
  remoteKeyId: string,
  stationId: string,
): Promise<CreateLocalStationKeyFromRemoteResult> {
  return withKeyPoolItemsInvalidation(invoke<CreateLocalStationKeyFromRemoteResult>("create_local_station_key_from_remote", {
    remoteKeyId,
    stationId,
  }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      throw new Error("浏览器预览模式无法读取远端完整 Key，请在桌面端同步或手动补全。");
    }
    throw error;
  }));
}

export function bindRemoteStationKey(remoteKeyId: string, stationKeyId: string): Promise<RemoteStationKey[]> {
  return invoke<RemoteStationKey[]>("bind_remote_station_key", {
    input: { remoteKeyId, stationKeyId },
  }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const targetKey = Array.from(memoryKeys.values())
        .flat()
        .find((key) => key.id === stationKeyId);
      if (!targetKey) {
        throw new Error("浏览器预览模式：未找到要绑定的本地密钥。");
      }

      let matched = false;
      const keys = memoryRemoteKeys.get(targetKey.stationId) ?? [];
      const nextKeys = keys.map((key) => {
        if (key.id !== remoteKeyId) {
          return key;
        }
        matched = true;
        return {
          ...key,
          matchStatus: "matched" as const,
          matchedStationKeyId: stationKeyId,
          matchConfidence: 1,
          collectedAt: now,
        };
      });
      if (!matched) {
        throw new Error("浏览器预览模式：未找到同中转站的远端密钥。");
      }

      memoryRemoteKeys.set(targetKey.stationId, nextKeys);
      return nextKeys;
    }
    throw error;
  });
}

export function unbindRemoteStationKey(remoteKeyId: string, stationId: string): Promise<RemoteStationKey[]> {
  return invoke<RemoteStationKey[]>("unbind_remote_station_key", { remoteKeyId, stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const keys = memoryRemoteKeys.get(stationId) ?? [];
      const nextKeys = keys.map((key) =>
        key.id === remoteKeyId
          ? {
              ...key,
              matchStatus: "unbound" as const,
              matchedStationKeyId: null,
              matchConfidence: 0,
              collectedAt: now,
            }
          : key,
      );
      memoryRemoteKeys.set(stationId, nextKeys);
      return nextKeys;
    }
    throw error;
  });
}

export function createStationKey(input: CreateStationKeyInput) {
  return withKeyPoolItemsInvalidation(invoke<StationKey>("create_station_key", { input }).catch(async (error) => {
    if (isInvokeUnavailable(error)) {
      const key = memoryKeyFromInput(input);
      memoryKeys.set(input.stationId, [...(memoryKeys.get(input.stationId) ?? []), key]);
      const station = (await listStations()).find((item) => item.id === input.stationId) ?? null;
      memoryKeyPool = [
        memoryPoolItemFromKey(key, station),
        ...memoryKeyPool.filter((item) => item.id !== key.id),
      ];
      return key;
    }
    throw error;
  }));
}

export function updateStationKey(input: UpdateStationKeyInput) {
  return withKeyPoolItemsInvalidation(invoke<StationKey>("update_station_key", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const keys = memoryKeys.get(input.stationId) ?? [];
      const nextKeys = keys.map((key) => key.id === input.id ? { ...key, ...input, apiKeyPresent: input.apiKey ? true : key.apiKeyPresent } : key);
      memoryKeys.set(input.stationId, nextKeys);
      return nextKeys.find((key) => key.id === input.id) ?? keys[0];
    }
    throw error;
  }));
}

export function saveStationKeyWithDefaults(
  input: SaveStationKeyWithDefaultsInput,
): Promise<SaveStationKeyWithDefaultsResult> {
  return withKeyPoolItemsInvalidation(invoke<SaveStationKeyWithDefaultsResult>("save_station_key_with_defaults", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return saveStationKeyWithDefaultsInMemory(input);
    }
    throw error;
  }));
}

export function updateStationKeyGroupBinding(stationKeyId: string, groupBindingId: string) {
  return withKeyPoolItemsInvalidation(invoke<StationKey>("update_station_key_group_binding", {
    input: { stationKeyId, groupBindingId },
  }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      for (const [stationId, keys] of memoryKeys) {
        const nextKeys = keys.map((key) =>
          key.id === stationKeyId
            ? {
                ...key,
                groupBindingId,
                rateSource: "manual",
                rateCollectedAt: new Date().toISOString(),
                updatedAt: new Date().toISOString(),
              }
            : key,
        );
        memoryKeys.set(stationId, nextKeys);
      }
      const item = memoryKeyPool.find((key) => key.id === stationKeyId);
      if (item) {
        item.groupBindingId = groupBindingId;
        item.rateSource = "manual";
        item.rateCollectedAt = new Date().toISOString();
        item.updatedAt = new Date().toISOString();
        return item;
      }
      return memoryKeyPool[0];
    }
    throw error;
  }));
}

export function deleteStationKey(id: string) {
  return withKeyPoolItemsInvalidation(invoke<void>("delete_station_key", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      for (const [stationId, keys] of memoryKeys) {
        memoryKeys.set(stationId, keys.filter((key) => key.id !== id));
      }
      return;
    }
    throw error;
  }));
}

export function reorderStationKeys(stationId: string, keyIds: string[]) {
  return withKeyPoolItemsInvalidation(invoke<StationKey[]>("reorder_station_keys", { stationId, keyIds }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const byId = new Map((memoryKeys.get(stationId) ?? []).map((key) => [key.id, key] as const));
      const nextKeys = keyIds.flatMap((id, index) => {
        const key = byId.get(id);
        return key ? [{ ...key, priority: index }] : [];
      });
      memoryKeys.set(stationId, nextKeys);
      return nextKeys;
    }
    throw error;
  }));
}

export function listKeyPoolItems() {
  return invoke<KeyPoolItem[]>("list_key_pool_items").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryKeyPool;
    }
    throw error;
  });
}

export function reorderKeyPool(keyIds: string[]) {
  return withKeyPoolItemsInvalidation(invoke<KeyPoolItem[]>("reorder_key_pool", { keyIds }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const byId = new Map(memoryKeyPool.map((item) => [item.id, item] as const));
      memoryKeyPool = keyIds.flatMap((id, index) => {
        const item = byId.get(id);
        return item ? [{ ...item, priority: index }] : [];
      });
      return memoryKeyPool;
    }
    throw error;
  }));
}

export function testStationKeyConnectivity(stationKeyId: string) {
  return invoke<StationKeyConnectivityTestResult>("test_station_key_connectivity", { stationKeyId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return {
        stationKeyId,
        ok: true,
        statusCode: 200,
        durationMs: 0,
        model: "mock",
        message: "浏览器预览模式：跳过真实连通性测试",
      };
    }
    throw error;
  });
}

export function getStationCredentials(stationId: string) {
  return invoke<StationCredentials>("get_station_credentials", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryCredentials.get(stationId) ?? emptyCredentials(stationId);
    }
    throw error;
  });
}

export function updateStationCredentials(input: {
  stationId: string;
  loginUsername: string | null;
  loginPassword: string | null;
  rememberPassword: boolean;
}) {
  return invoke<StationCredentials>("update_station_credentials", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const existing = memoryCredentials.get(input.stationId) ?? emptyCredentials(input.stationId);
      const hasNewPassword = Boolean(input.loginPassword?.trim());
      const next = {
        ...existing,
        loginUsername: input.loginUsername,
        passwordPresent: input.rememberPassword ? hasNewPassword || existing.passwordPresent : false,
        rememberPassword: input.rememberPassword,
        loginStatus: "saved",
        updatedAt: new Date().toISOString(),
      };
      memoryCredentials.set(input.stationId, next);
      return next;
    }
    throw error;
  });
}

export function clearStationCredentials(stationId: string) {
  return invoke<StationCredentials>("clear_station_credentials", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const next = emptyCredentials(stationId);
      memoryCredentials.set(stationId, next);
      return next;
    }
    throw error;
  });
}

export function updateStationSession(input: UpdateStationSessionInput) {
  return invoke<StationCredentials>("update_station_session", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const existing = memoryCredentials.get(input.stationId) ?? emptyCredentials(input.stationId);
      const next = {
        ...existing,
        accessTokenPresent: Boolean(input.accessToken),
        refreshTokenPresent: Boolean(input.refreshToken),
        cookiePresent: Boolean(input.cookie),
        newapiUserId: input.newapiUserId?.trim() || null,
        tokenExpiresAt: input.tokenExpiresAt?.trim() || null,
        sessionStatus: input.accessToken || input.refreshToken || input.cookie ? "valid" : "none",
        sessionSource: input.accessToken || input.refreshToken || input.cookie ? "manual" : null,
        updatedAt: new Date().toISOString(),
      };
      memoryCredentials.set(input.stationId, next);
      return next;
    }
    throw error;
  });
}

function emptyCredentials(stationId: string): StationCredentials {
  return {
    stationId,
    loginUsername: null,
    passwordPresent: false,
    rememberPassword: false,
    loginStatus: "unknown",
    loginError: null,
    lastLoginAt: null,
    sessionStatus: "none",
    sessionExpiresAt: null,
    accessTokenPresent: false,
    refreshTokenPresent: false,
    cookiePresent: false,
    sessionSource: null,
    newapiUserId: null,
    tokenExpiresAt: null,
    tokenRefreshedAt: null,
    updatedAt: null,
  };
}

async function memoryRemoteKeyCapability(stationId: string): Promise<RemoteKeyCapability> {
  const station = await listStations()
    .then((items) => items.find((item) => item.id === stationId) ?? null)
    .catch(() => null);

  if (!station) {
    return unsupportedRemoteKeyCapability(
      stationId,
      "unknown",
      "浏览器预览模式：未找到中转站，无法判断远端 Key 能力。",
    );
  }

  if (station.stationType === "sub2api") {
    return {
      stationId,
      stationType: station.stationType,
      canListRemoteKeys: true,
      canCreateRemoteKey: true,
      canReadGroups: true,
      requiresManualSession: true,
      unsupportedReason: null,
    };
  }

  if (station.stationType === "newapi") {
    return {
      stationId,
      stationType: station.stationType,
      canListRemoteKeys: true,
      canCreateRemoteKey: true,
      canReadGroups: true,
      requiresManualSession: true,
      unsupportedReason: null,
    };
  }

  return unsupportedRemoteKeyCapability(
    stationId,
    station.stationType,
    `暂不支持 ${station.stationType} 类型中转站的远端 Key 管理。`,
  );
}

function unsupportedRemoteKeyCapability(
  stationId: string,
  stationType: string,
  unsupportedReason: string,
): RemoteKeyCapability {
  return {
    stationId,
    stationType,
    canListRemoteKeys: false,
    canCreateRemoteKey: false,
    canReadGroups: false,
    requiresManualSession: false,
    unsupportedReason,
  };
}

function memoryRemoteKeyFromStationKey(
  stationKey: StationKey,
  input: CreateRemoteStationKeyInput,
): RemoteStationKey {
  const now = new Date().toISOString();
  return {
    id: nextMemoryId("remote-key"),
    stationId: input.stationId,
    remoteKeyIdHash: `preview-${stationKey.id}`,
    remoteKeyName: input.name,
    apiKeyMasked: stationKey.apiKeyMasked,
    apiKeyFingerprint: null,
    groupIdHash: input.groupIdHash,
    groupName: input.groupName,
    tierLabel: stationKey.tierLabel,
    rateMultiplier: stationKey.rateMultiplier,
    rateSource: stationKey.rateSource,
    createdAt: stationKey.createdAt,
    lastUsedAt: stationKey.lastUsedAt,
    rawSource: "browser-preview",
    matchStatus: "matched",
    matchedStationKeyId: stationKey.id,
    matchConfidence: 1,
    collectedAt: now,
  };
}

function memoryKeyFromInput(input: CreateStationKeyInput): StationKey {
  const now = new Date().toISOString();
  return {
    id: nextMemoryId("key"),
    stationId: input.stationId,
    name: input.name,
    apiKeyMasked: input.apiKey ? "sk-****" : "未设置",
    apiKeyPresent: Boolean(input.apiKey),
    enabled: input.enabled,
    priority: input.priority ?? (memoryKeys.get(input.stationId)?.length ?? 0),
    groupBindingId: input.groupBindingId ?? null,
    groupIdHash: input.groupIdHash ?? null,
    groupName: input.groupName,
    tierLabel: input.tierLabel,
    rateMultiplier: input.rateMultiplier ?? null,
    rateSource: input.rateSource ?? null,
    rateCollectedAt: null,
    balanceScope: input.balanceScope ?? null,
    status: "unchecked",
    lastCheckedAt: null,
    lastUsedAt: null,
    note: input.note,
    createdAt: now,
    updatedAt: now,
  };
}

async function saveStationKeyWithDefaultsInMemory(
  input: SaveStationKeyWithDefaultsInput,
): Promise<SaveStationKeyWithDefaultsResult> {
  validateSaveStationKeyWithDefaultsInput(input);

  const stationKey =
    input.mode === "create"
      ? await createStationKey(memoryCreateInputFromDefaults(input))
      : await updateStationKey(memoryUpdateInputFromDefaults(input));
  const capabilities = await memoryCapabilitiesForSavedKey(input, stationKey.id);

  return {
    stationKey,
    capabilities,
    message: "Browser preview fallback: station key saved with default capabilities.",
  };
}

function validateSaveStationKeyWithDefaultsInput(input: SaveStationKeyWithDefaultsInput) {
  if (input.mode === "create" && input.groupSelection.kind === "keep") {
    throw new Error("Browser preview fallback: create mode cannot keep an existing group selection.");
  }
}

function memoryCreateInputFromDefaults(input: SaveStationKeyWithDefaultsInput): CreateStationKeyInput {
  const groupSelection = input.groupSelection.kind === "set" ? input.groupSelection : null;
  return {
    stationId: input.stationId,
    name: input.name,
    apiKey: input.apiKey ?? "",
    enabled: input.enabled,
    priority: input.priority ?? null,
    groupBindingId: groupSelection?.groupBindingId ?? null,
    groupIdHash: groupSelection?.groupIdHash ?? null,
    groupName: groupSelection?.groupName ?? null,
    tierLabel: input.tierLabel ?? null,
    rateMultiplier: null,
    rateSource: null,
    balanceScope: input.balanceScope ?? null,
    note: input.note ?? null,
  };
}

function memoryUpdateInputFromDefaults(input: SaveStationKeyWithDefaultsInput): UpdateStationKeyInput {
  if (!input.id) {
    throw new Error("Browser preview fallback: station key id is required for update.");
  }

  const existing = (memoryKeys.get(input.stationId) ?? []).find((key) => key.id === input.id);
  const groupFields = memoryGroupFieldsFromSelection(input, existing ?? null);

  const updateInput: UpdateStationKeyInput = {
    id: input.id,
    stationId: input.stationId,
    name: input.name,
    apiKey: input.apiKey ?? null,
    enabled: input.enabled,
    priority: input.priority ?? existing?.priority ?? 0,
    ...groupFields,
    tierLabel: input.tierLabel ?? null,
    rateMultiplier: groupFields.groupBindingId === existing?.groupBindingId ? existing?.rateMultiplier ?? null : null,
    rateSource: groupFields.groupBindingId === existing?.groupBindingId ? existing?.rateSource ?? null : null,
    status: input.status ?? existing?.status ?? "unchecked",
    note: input.note ?? null,
  };

  if (input.balanceScope != null) {
    updateInput.balanceScope = input.balanceScope;
  }

  return updateInput;
}

function memoryGroupFieldsFromSelection(
  input: SaveStationKeyWithDefaultsInput,
  existing: StationKey | null,
): Pick<UpdateStationKeyInput, "groupBindingId" | "groupIdHash" | "groupName"> {
  if (input.groupSelection.kind === "keep") {
    return {
      groupBindingId: existing?.groupBindingId ?? null,
      groupIdHash: existing?.groupIdHash ?? null,
      groupName: existing?.groupName ?? null,
    };
  }

  if (input.groupSelection.kind === "clear") {
    return {
      groupBindingId: null,
      groupIdHash: null,
      groupName: null,
    };
  }

  return {
    groupBindingId: input.groupSelection.groupBindingId,
    groupIdHash: input.groupSelection.groupIdHash ?? null,
    groupName: input.groupSelection.groupName ?? null,
  };
}

function defaultStationKeyCapabilities(stationKeyId: string): StationKeyCapabilities {
  return {
    stationKeyId,
    supportsChatCompletions: true,
    supportsResponses: true,
    supportsEmbeddings: true,
    supportsStream: true,
    supportsTools: true,
    supportsVision: true,
    supportsReasoning: true,
    modelAllowlist: [],
    modelBlocklist: [],
    preferredModels: [],
    onlyUseAsBackup: false,
    routingTags: [],
    updatedAt: new Date().toISOString(),
  };
}

async function memoryCapabilitiesForSavedKey(
  input: SaveStationKeyWithDefaultsInput,
  stationKeyId: string,
): Promise<StationKeyCapabilities> {
  if (input.capabilities) {
    return updateStationKeyCapabilities({
      ...input.capabilities,
      stationKeyId,
    });
  }

  if (input.mode === "update") {
    return getStationKeyCapabilities(stationKeyId);
  }

  return updateStationKeyCapabilities(defaultStationKeyCapabilitiesInput(stationKeyId));
}

function defaultStationKeyCapabilitiesInput(stationKeyId: string): UpdateStationKeyCapabilitiesInput {
  const { updatedAt: _updatedAt, ...input } = defaultStationKeyCapabilities(stationKeyId);
  return input;
}

function nextMemoryId(prefix: string) {
  const randomId = globalThis.crypto?.randomUUID?.();
  if (randomId) {
    return `${prefix}-${randomId}`;
  }
  return `${prefix}-${Date.now()}-${++memoryIdCounter}`;
}

function memoryPoolItemFromKey(key: StationKey, station: Station | null): KeyPoolItem {
  return {
    ...key,
    stationName: station?.name ?? "未命名中转站",
    stationType: station?.stationType ?? "custom",
    stationBaseUrl: station?.baseUrl ?? "",
    stationUpstreamApiFormat: "auto",
    capabilitySummary: [],
    modelScopeSummary: "全部模型",
    onlyUseAsBackup: false,
    cooldownUntil: null,
    successRate: null,
    avgLatencyMs: null,
    consecutiveFailures: 0,
    lastErrorSummary: null,
    endpointPingStatus: "unchecked",
    endpointPingMs: null,
    endpointPingCheckedAt: null,
    endpointPingError: null,
    bindingStatus: null,
    priceState: null,
  };
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
