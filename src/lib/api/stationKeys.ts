import { invoke } from "@tauri-apps/api/core";
import { listStations } from "@/lib/api/stations";
import type {
  CreateStationKeyInput,
  KeyPoolItem,
  StationKeyConnectivityTestResult,
  StationCredentials,
  StationKey,
  UpdateStationSessionInput,
  UpdateStationKeyInput,
} from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";

const memoryKeys = new Map<string, StationKey[]>();
const memoryCredentials = new Map<string, StationCredentials>();
let memoryKeyPool: KeyPoolItem[] = [];

export function listStationKeys(stationId: string) {
  return invoke<StationKey[]>("list_station_keys", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryKeys.get(stationId) ?? [];
    }
    throw error;
  });
}

export function createStationKey(input: CreateStationKeyInput) {
  return invoke<StationKey>("create_station_key", { input }).catch(async (error) => {
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
  });
}

export function updateStationKey(input: UpdateStationKeyInput) {
  return invoke<StationKey>("update_station_key", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const keys = memoryKeys.get(input.stationId) ?? [];
      const nextKeys = keys.map((key) => key.id === input.id ? { ...key, ...input, apiKeyPresent: input.apiKey ? true : key.apiKeyPresent } : key);
      memoryKeys.set(input.stationId, nextKeys);
      return nextKeys.find((key) => key.id === input.id) ?? keys[0];
    }
    throw error;
  });
}

export function updateStationKeyGroupBinding(stationKeyId: string, groupBindingId: string) {
  return invoke<StationKey>("update_station_key_group_binding", {
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
  });
}

export function deleteStationKey(id: string) {
  return invoke<void>("delete_station_key", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      for (const [stationId, keys] of memoryKeys) {
        memoryKeys.set(stationId, keys.filter((key) => key.id !== id));
      }
      return;
    }
    throw error;
  });
}

export function reorderStationKeys(stationId: string, keyIds: string[]) {
  return invoke<StationKey[]>("reorder_station_keys", { stationId, keyIds }).catch((error) => {
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
  });
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
  return invoke<KeyPoolItem[]>("reorder_key_pool", { keyIds }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const byId = new Map(memoryKeyPool.map((item) => [item.id, item] as const));
      memoryKeyPool = keyIds.flatMap((id, index) => {
        const item = byId.get(id);
        return item ? [{ ...item, priority: index }] : [];
      });
      return memoryKeyPool;
    }
    throw error;
  });
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

function memoryKeyFromInput(input: CreateStationKeyInput): StationKey {
  const now = new Date().toISOString();
  return {
    id: `key-${Date.now()}`,
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

function memoryPoolItemFromKey(key: StationKey, station: Station | null): KeyPoolItem {
  return {
    ...key,
    stationName: station?.name ?? "未命名中转站",
    stationType: station?.stationType ?? "custom",
    stationBaseUrl: station?.baseUrl ?? "",
    capabilitySummary: [],
    modelScopeSummary: "全部模型",
    onlyUseAsBackup: false,
    cooldownUntil: null,
    successRate: null,
    avgLatencyMs: null,
    consecutiveFailures: 0,
    lastErrorSummary: null,
    bindingStatus: null,
    priceState: null,
  };
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
