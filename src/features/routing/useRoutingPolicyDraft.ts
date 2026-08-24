import { useCallback, useEffect, useReducer } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { applyRoutingPolicyDocument } from "@/lib/api/routing";
import { BackendError, isBackendError } from "@/lib/bridge/errors";
import { readError } from "@/lib/errors";
import {
  routingPolicyQueryOptions,
  routingQueryKeys,
} from "@/lib/queries/routingQueries";
import type {
  ApplyRoutingPolicyDocumentInput,
  RoutingPolicyConfigV2,
  RoutingPolicySnapshot,
} from "@/lib/types/routing";
import { useActivityQuery } from "@/lib/query/useActivityQuery";

export type RoutingPolicyDraftStatus =
  | "loading"
  | "ready"
  | "dirty"
  | "saving"
  | "saved"
  | "error"
  | "conflict";

export type RoutingPolicyDraftState = {
  config: RoutingPolicyConfigV2 | null;
  initialConfig: RoutingPolicyConfigV2 | null;
  baseRevision: number | null;
  remoteSnapshot: RoutingPolicySnapshot | null;
  status: RoutingPolicyDraftStatus;
  error: string | null;
  fieldErrors: Record<string, string>;
  conflictRevision: number | null;
};

export type RoutingPolicyDraftAction =
  | { type: "hydrate"; snapshot: RoutingPolicySnapshot }
  | { type: "edit"; config: RoutingPolicyConfigV2 }
  | { type: "saveStart" }
  | { type: "saveSuccess"; snapshot: RoutingPolicySnapshot }
  | { type: "saveError"; error: string; fieldErrors?: Record<string, string> }
  | { type: "saveConflict"; error: string; currentRevision: number | null }
  | { type: "discard" }
  | { type: "mergeRemote" }
  | { type: "overwriteRemote" };

export const initialRoutingPolicyDraftState: RoutingPolicyDraftState = {
  config: null,
  initialConfig: null,
  baseRevision: null,
  remoteSnapshot: null,
  status: "loading",
  error: null,
  fieldErrors: {},
  conflictRevision: null,
};

/** Return a fresh copy of the backend-compatible V2 baseline for draft reset. */
export function createDefaultRoutingPolicyConfig(): RoutingPolicyConfigV2 {
  return {
    version: 2,
    reliabilityWeight: 4_000,
    responsivenessWeight: 2_500,
    costWeight: 2_000,
    preferenceWeight: 1_500,
    maxCandidates: 64,
    explorationShareBasisPoints: 500,
    allowDepletedFallback: false,
    affinityEnabled: false,
    affinityTtlSeconds: 300,
    maxRateMultiplier: null,
    routingGroupFilter: "all_groups",
    outboundProxyMode: "inherit",
    outboundProxyUrl: null,
    retryFailover: {
      version: 2,
      maxTotalAttempts: 4,
      maxSameTargetCapacityRetries: 2,
      capacityRetryWaitBudgetSeconds: 2,
      allowCrossCapacityDomainFallback: true,
    },
    protectionProfile: {
      version: 2,
      enabled: false,
      windowMaxSamples: 64,
      windowSeconds: 300,
      minSamples: 5,
      failureThresholdPercent: 60,
      halfOpenSuccessesToClose: 2,
    },
    timeoutPolicy: {
      version: 2,
      connectSeconds: 10,
      firstByteSeconds: 30,
      precommitSeconds: 60,
      bufferedExecutionSeconds: 300,
      streamIdleSeconds: 90,
    },
  };
}

function configFingerprint(value: unknown): string {
  return value === null ? "<null>" : JSON.stringify(value);
}

export function routingPolicyConfigEqual(
  left: RoutingPolicyConfigV2 | null,
  right: RoutingPolicyConfigV2 | null,
): boolean {
  return configFingerprint(left) === configFingerprint(right);
}

export function routingPolicyDraftIsDirty(state: RoutingPolicyDraftState): boolean {
  return !routingPolicyConfigEqual(state.config, state.initialConfig);
}

/** Non-blocking client hints. The backend remains the final validator and no
 * value is silently clamped before the document reaches CAS validation. */
export function routingPolicyDraftFieldHints(
  config: RoutingPolicyConfigV2 | null,
): Record<string, string> {
  if (!config) return {};
  const hints: Record<string, string> = {};
  if (config.retryFailover.maxSameTargetCapacityRetries >= config.retryFailover.maxTotalAttempts) {
    hints["retryFailover.maxSameTargetCapacityRetries"] =
      "同目标容量重试次数必须小于单个请求最大尝试次数。保存时后端会再次校验。";
  }
  return hints;
}

function withSnapshot(
  snapshot: RoutingPolicySnapshot,
): RoutingPolicyDraftState {
  return {
    config: snapshot.config,
    initialConfig: snapshot.config,
    baseRevision: snapshot.revision,
    remoteSnapshot: snapshot,
    status: "ready",
    error: null,
    fieldErrors: {},
    conflictRevision: null,
  };
}

function dirtyFields(
  base: RoutingPolicyConfigV2,
  local: RoutingPolicyConfigV2,
): Set<keyof RoutingPolicyConfigV2> {
  const keys = Object.keys(base) as Array<keyof RoutingPolicyConfigV2>;
  return new Set(keys.filter((key) =>
    configFingerprint(base[key] as never) !== configFingerprint(local[key] as never),
  ));
}

function mergeWithRemote(state: RoutingPolicyDraftState): RoutingPolicyConfigV2 | null {
  if (!state.config || !state.initialConfig || !state.remoteSnapshot) return state.config;
  const localChanges = dirtyFields(state.initialConfig, state.config);
  const merged = { ...state.remoteSnapshot.config };
  for (const key of localChanges) {
    if (key === "retryFailover") {
      const baseRetry = state.initialConfig.retryFailover;
      const localRetry = state.config.retryFailover;
      const remoteRetry = state.remoteSnapshot.config.retryFailover;
      const retryFailover = { ...remoteRetry };
      for (const nestedKey of Object.keys(baseRetry) as Array<keyof typeof baseRetry>) {
        if (configFingerprint(baseRetry[nestedKey]) !== configFingerprint(localRetry[nestedKey])) {
          (retryFailover as Record<string, number | boolean>)[nestedKey] = localRetry[nestedKey];
        }
      }
      merged.retryFailover = retryFailover;
    } else if (key === "protectionProfile") {
      const baseProfile = state.initialConfig.protectionProfile;
      const localProfile = state.config.protectionProfile;
      const remoteProfile = state.remoteSnapshot.config.protectionProfile;
      const protectionProfile = { ...remoteProfile };
      for (const nestedKey of Object.keys(baseProfile) as Array<keyof typeof baseProfile>) {
        if (configFingerprint(baseProfile[nestedKey]) !== configFingerprint(localProfile[nestedKey])) {
          (protectionProfile as Record<string, number | boolean>)[nestedKey] = localProfile[nestedKey];
        }
      }
      merged.protectionProfile = protectionProfile;
    } else if (key === "timeoutPolicy") {
      const baseTimeout = state.initialConfig.timeoutPolicy;
      const localTimeout = state.config.timeoutPolicy;
      const remoteTimeout = state.remoteSnapshot.config.timeoutPolicy;
      const timeoutPolicy = { ...remoteTimeout };
      for (const nestedKey of Object.keys(baseTimeout) as Array<keyof typeof baseTimeout>) {
        if (configFingerprint(baseTimeout[nestedKey]) !== configFingerprint(localTimeout[nestedKey])) {
          (timeoutPolicy as Record<string, number>)[nestedKey] = localTimeout[nestedKey];
        }
      }
      merged.timeoutPolicy = timeoutPolicy;
    } else {
      merged[key] = state.config[key] as never;
    }
  }
  return merged;
}

export function routingPolicyDraftReducer(
  state: RoutingPolicyDraftState,
  action: RoutingPolicyDraftAction,
): RoutingPolicyDraftState {
  switch (action.type) {
    case "hydrate": {
      if (state.baseRevision === null) return withSnapshot(action.snapshot);
      // React Query can deliver a stale in-flight response after a successful
      // CAS write. Never roll a draft back to an older authoritative revision.
      if (action.snapshot.revision < state.baseRevision) return state;
      if (action.snapshot.revision === state.baseRevision) {
        return state.remoteSnapshot?.revision === action.snapshot.revision
          ? state
          : { ...state, remoteSnapshot: action.snapshot };
      }
      if (!routingPolicyDraftIsDirty(state) && state.status !== "saving") {
        return withSnapshot(action.snapshot);
      }
      return {
        ...state,
        remoteSnapshot: action.snapshot,
        status: "conflict",
        conflictRevision: action.snapshot.revision,
        error: "策略已被其他操作更新，请选择如何处理本地草稿。",
        fieldErrors: {},
      };
    }
    case "edit":
      return {
        ...state,
        config: action.config,
        status: "dirty",
        error: null,
        fieldErrors: {},
      };
    case "saveStart":
      return { ...state, status: "saving", error: null, fieldErrors: {} };
    case "saveSuccess":
      return {
        config: action.snapshot.config,
        initialConfig: action.snapshot.config,
        baseRevision: action.snapshot.revision,
        remoteSnapshot: action.snapshot,
        status: "saved",
        error: null,
        fieldErrors: {},
        conflictRevision: null,
      };
    case "saveError":
      return { ...state, status: "error", error: action.error, fieldErrors: action.fieldErrors ?? {} };
    case "saveConflict":
      return {
        ...state,
        status: "conflict",
        error: action.error,
        fieldErrors: {},
        conflictRevision: action.currentRevision,
      };
    case "discard":
      return state.remoteSnapshot ? withSnapshot(state.remoteSnapshot) : state;
    case "mergeRemote": {
      if (!state.remoteSnapshot) return state;
      const config = mergeWithRemote(state);
      const next = withSnapshot(state.remoteSnapshot);
      return config && !routingPolicyConfigEqual(config, next.config)
        ? { ...next, config, status: "dirty" }
        : next;
    }
    case "overwriteRemote": {
      if (!state.remoteSnapshot || !state.config) return state;
      return {
        ...state,
        initialConfig: state.remoteSnapshot.config,
        baseRevision: state.remoteSnapshot.revision,
        remoteSnapshot: state.remoteSnapshot,
        status: routingPolicyConfigEqual(state.config, state.remoteSnapshot.config)
          ? "ready"
          : "dirty",
        error: null,
        fieldErrors: {},
        conflictRevision: null,
      };
    }
  }
}

export type UseRoutingPolicyDraftResult = {
  state: RoutingPolicyDraftState;
  query: ReturnType<typeof useActivityQuery<RoutingPolicySnapshot>>;
  setConfig: (config: RoutingPolicyConfigV2) => void;
  save: () => Promise<boolean>;
  reload: () => Promise<void>;
  discard: () => void;
  mergeRemote: () => void;
  overwriteRemote: () => void;
};

function conflictInfoFromError(error: unknown): { isConflict: boolean; revision: number | null } {
  if (!isBackendError(error) || error.code !== "conflict") {
    return { isConflict: false, revision: null };
  }
  if (error.details?.kind !== "conflict") return { isConflict: true, revision: null };
  const revision = Number(error.details.currentRevision);
  return {
    isConflict: true,
    revision: Number.isSafeInteger(revision) ? revision : null,
  };
}

function errorMessage(error: unknown): string {
  if (error instanceof BackendError) return readError(error);
  return readError(error);
}

function validationFieldErrors(error: unknown): Record<string, string> {
  if (!isBackendError(error) || error.details?.kind !== "validation") return {};
  return Object.fromEntries(error.details.fields.map(({ field, message }) => [field, message]));
}

export function useRoutingPolicyDraft(): UseRoutingPolicyDraftResult {
  const queryClient = useQueryClient();
  const query = useActivityQuery(routingPolicyQueryOptions());
  const [state, dispatch] = useReducer(
    routingPolicyDraftReducer,
    initialRoutingPolicyDraftState,
  );

  useEffect(() => {
    if (query.data) dispatch({ type: "hydrate", snapshot: query.data });
    if (query.error && state.baseRevision === null) {
      dispatch({ type: "saveError", error: errorMessage(query.error), fieldErrors: validationFieldErrors(query.error) });
    }
  }, [query.data, query.error, state.baseRevision]);

  const setConfig = useCallback((config: RoutingPolicyConfigV2) => {
    dispatch({ type: "edit", config });
  }, []);

  const save = useCallback(async (): Promise<boolean> => {
    if (!state.config || state.baseRevision === null || state.status === "saving") {
      return false;
    }
    dispatch({ type: "saveStart" });
    const input: ApplyRoutingPolicyDocumentInput = {
      formatVersion: 1,
      baseRevision: state.baseRevision,
      policy: state.config,
    };
    try {
      const snapshot = await applyRoutingPolicyDocument(input);
      dispatch({ type: "saveSuccess", snapshot });
      queryClient.setQueryData(routingQueryKeys.policy(), snapshot);
      await queryClient.invalidateQueries({ queryKey: routingQueryKeys.all });
      return true;
    } catch (error) {
      const conflict = conflictInfoFromError(error);
      dispatch(conflict.isConflict
        ? {
            type: "saveConflict",
            error: "策略保存冲突：服务器已有更新，请选择重新加载、合并或覆盖。",
            currentRevision: conflict.revision,
          }
        : { type: "saveError", error: errorMessage(error), fieldErrors: validationFieldErrors(error) });
      if (conflict.isConflict) {
        await query.refetch();
      }
      return false;
    }
  }, [query, queryClient, state.baseRevision, state.config, state.status]);

  const reload = useCallback(async () => {
    await query.refetch();
  }, [query]);

  return {
    state,
    query,
    setConfig,
    save,
    reload,
    discard: () => dispatch({ type: "discard" }),
    mergeRemote: () => dispatch({ type: "mergeRemote" }),
    overwriteRemote: () => dispatch({ type: "overwriteRemote" }),
  };
}
