import { useCallback, useEffect, useReducer } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { applyRoutingPolicyDocument } from "@/lib/api/routing";
import { BackendError, isBackendError } from "@/lib/bridge/errors";
import { readError } from "@/lib/errors";
import {
  routingPolicyQueryOptions,
  routingPolicyPublicationQueryOptions,
  routingQueryKeys,
} from "@/lib/queries/routingQueries";
import type {
  ApplyRoutingPolicyDocumentInput,
  RoutingPolicyConfigV3,
  RoutingPolicyPublicationStatus,
  RoutingPolicyPublicationStatusInput,
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

export type RoutingPolicyPublicationPollingState =
  | "idle"
  | "polling"
  | "unavailable"
  | "timed_out";

export const ROUTING_POLICY_PUBLICATION_POLL_INTERVAL_MS = 1_000;
export const ROUTING_POLICY_PUBLICATION_POLL_TIMEOUT_MS = 120_000;

export type RoutingPolicyDraftState = {
  config: RoutingPolicyConfigV3 | null;
  initialConfig: RoutingPolicyConfigV3 | null;
  baseRevision: number | null;
  remoteSnapshot: RoutingPolicySnapshot | null;
  publicationStatus: string | null;
  publicationGenerationId: string | null;
  publicationFailureCode: string | null;
  publicationPollingState: RoutingPolicyPublicationPollingState;
  publicationError: string | null;
  publicationStartedAtMs: number | null;
  status: RoutingPolicyDraftStatus;
  error: string | null;
  fieldErrors: Record<string, string>;
  conflictRevision: number | null;
};

export type RoutingPolicyDraftAction =
  | { type: "hydrate"; snapshot: RoutingPolicySnapshot }
  | { type: "edit"; config: RoutingPolicyConfigV3 }
  | { type: "saveStart" }
  | { type: "saveSuccess"; snapshot: RoutingPolicySnapshot; publicationStartedAtMs?: number }
  | { type: "publicationUpdate"; publication: RoutingPolicyPublicationStatus }
  | { type: "publicationUnavailable"; revision: number }
  | { type: "publicationTimeout"; revision: number }
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
  publicationStatus: null,
  publicationGenerationId: null,
  publicationFailureCode: null,
  publicationPollingState: "idle",
  publicationError: null,
  publicationStartedAtMs: null,
  status: "loading",
  error: null,
  fieldErrors: {},
  conflictRevision: null,
};

/** Return a fresh copy of the backend-compatible V3 baseline for draft reset. */
export function createDefaultRoutingPolicyConfig(): RoutingPolicyConfigV3 {
  return {
    version: 3,
    reliabilityWeight: 4_000,
    responsivenessWeight: 2_500,
    costWeight: 2_000,
    preferenceWeight: 1_500,
    allowDepletedFallback: false,
    affinityEnabled: false,
    affinityTtlSeconds: 300,
    maxRateMultiplier: null,
    routingGroupFilter: "all_groups",
    outboundProxyMode: "inherit",
    outboundProxyUrl: null,
    reliabilitySourceWeights: {
      realTrafficPercent: 70,
      monitoringPercent: 30,
    },
    reliabilitySampling: {
      historicalMinimumSamples: 15,
      recentMinimumSamples: 5,
      optimisticReliabilityPercent: 95,
      optimisticLatencyMs: 2_500,
    },
    retry: {
      version: 1,
      maxRetryCount: 3,
      consecutiveFailureThreshold: 3,
    },
    circuitBreaker: {
      version: 1,
      recoverySuccessThreshold: 2,
      recoveryWaitSeconds: 30,
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
  left: RoutingPolicyConfigV3 | null,
  right: RoutingPolicyConfigV3 | null,
): boolean {
  return configFingerprint(left) === configFingerprint(right);
}

export function routingPolicyDraftIsDirty(state: RoutingPolicyDraftState): boolean {
  return !routingPolicyConfigEqual(state.config, state.initialConfig);
}

/** Non-blocking client hints. The backend remains the final validator and no
 * value is silently clamped before the document reaches CAS validation. */
export function routingPolicyDraftFieldHints(
  config: RoutingPolicyConfigV3 | null,
): Record<string, string> {
  if (!config) return {};
  const hints: Record<string, string> = {};
  if (
    config.reliabilitySourceWeights.realTrafficPercent < 0 ||
    config.reliabilitySourceWeights.realTrafficPercent > 100 ||
    !Number.isInteger(config.reliabilitySourceWeights.realTrafficPercent) ||
    config.reliabilitySourceWeights.monitoringPercent < 0 ||
    config.reliabilitySourceWeights.monitoringPercent > 100 ||
    !Number.isInteger(config.reliabilitySourceWeights.monitoringPercent) ||
    config.reliabilitySourceWeights.realTrafficPercent + config.reliabilitySourceWeights.monitoringPercent !== 100
  ) {
    hints["reliabilitySourceWeights"] =
      "真实流量和监控权重必须是 0-100 的整数，且之和为 100%。保存时后端会再次校验。";
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
    publicationStatus: snapshot.status,
    publicationGenerationId: null,
    publicationFailureCode: null,
    publicationPollingState: "idle",
    publicationError: null,
    publicationStartedAtMs: null,
    status: "ready",
    error: null,
    fieldErrors: {},
    conflictRevision: null,
  };
}

function dirtyFields(
  base: RoutingPolicyConfigV3,
  local: RoutingPolicyConfigV3,
): Set<keyof RoutingPolicyConfigV3> {
  const keys = Object.keys(base) as Array<keyof RoutingPolicyConfigV3>;
  return new Set(keys.filter((key) =>
    configFingerprint(base[key] as never) !== configFingerprint(local[key] as never),
  ));
}

function mergeWithRemote(state: RoutingPolicyDraftState): RoutingPolicyConfigV3 | null {
  if (!state.config || !state.initialConfig || !state.remoteSnapshot) return state.config;
  const localChanges = dirtyFields(state.initialConfig, state.config);
  const merged = { ...state.remoteSnapshot.config };
  for (const key of localChanges) {
    if (key === "reliabilitySourceWeights") {
      const baseWeights = state.initialConfig.reliabilitySourceWeights;
      const localWeights = state.config.reliabilitySourceWeights;
      const remoteWeights = state.remoteSnapshot.config.reliabilitySourceWeights;
      const reliabilitySourceWeights = { ...remoteWeights };
      for (const nestedKey of Object.keys(baseWeights) as Array<keyof typeof baseWeights>) {
        if (configFingerprint(baseWeights[nestedKey]) !== configFingerprint(localWeights[nestedKey])) {
          reliabilitySourceWeights[nestedKey] = localWeights[nestedKey];
        }
      }
      merged.reliabilitySourceWeights = reliabilitySourceWeights;
    } else if (key === "reliabilitySampling") {
      const baseSampling = state.initialConfig.reliabilitySampling;
      const localSampling = state.config.reliabilitySampling;
      const remoteSampling = state.remoteSnapshot.config.reliabilitySampling;
      const reliabilitySampling = { ...remoteSampling };
      for (const nestedKey of Object.keys(baseSampling) as Array<keyof typeof baseSampling>) {
        if (configFingerprint(baseSampling[nestedKey]) !== configFingerprint(localSampling[nestedKey])) {
          reliabilitySampling[nestedKey] = localSampling[nestedKey];
        }
      }
      merged.reliabilitySampling = reliabilitySampling;
    } else if (key === "retry") {
      const baseRetry = state.initialConfig.retry;
      const localRetry = state.config.retry;
      const remoteRetry = state.remoteSnapshot.config.retry;
      const retry = { ...remoteRetry };
      for (const nestedKey of Object.keys(baseRetry) as Array<keyof typeof baseRetry>) {
        if (configFingerprint(baseRetry[nestedKey]) !== configFingerprint(localRetry[nestedKey])) {
          retry[nestedKey] = localRetry[nestedKey];
        }
      }
      merged.retry = retry;
    } else if (key === "circuitBreaker") {
      const baseCircuit = state.initialConfig.circuitBreaker;
      const localCircuit = state.config.circuitBreaker;
      const remoteCircuit = state.remoteSnapshot.config.circuitBreaker;
      const circuitBreaker = { ...remoteCircuit };
      for (const nestedKey of Object.keys(baseCircuit) as Array<keyof typeof baseCircuit>) {
        if (configFingerprint(baseCircuit[nestedKey]) !== configFingerprint(localCircuit[nestedKey])) {
          circuitBreaker[nestedKey] = localCircuit[nestedKey];
        }
      }
      merged.circuitBreaker = circuitBreaker;
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
        const publicationPending = shouldPollPublication(action.snapshot.status);
        return configFingerprint(state.remoteSnapshot) === configFingerprint(action.snapshot)
          ? state
          : {
              ...state,
              remoteSnapshot: action.snapshot,
              publicationStatus: action.snapshot.status,
              publicationPollingState: publicationPending
                ? state.publicationPollingState
                : "idle",
              publicationError: publicationPending ? state.publicationError : null,
              publicationStartedAtMs: publicationPending
                ? state.publicationStartedAtMs
                : null,
            };
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
      if (state.status === "saving") return state;
      return {
        ...state,
        config: action.config,
        status: "dirty",
        error: null,
        fieldErrors: {},
      };
    case "saveStart":
      return { ...state, status: "saving", error: null, fieldErrors: {} };
    case "saveSuccess": {
      const publicationPending = shouldPollPublication(action.snapshot.status);
      return {
        config: action.snapshot.config,
        initialConfig: action.snapshot.config,
        baseRevision: action.snapshot.revision,
        remoteSnapshot: action.snapshot,
        publicationStatus: action.snapshot.status,
        publicationGenerationId: null,
        publicationFailureCode: null,
        publicationPollingState: publicationPending ? "polling" : "idle",
        publicationError: null,
        publicationStartedAtMs: publicationPending
          ? action.publicationStartedAtMs ?? null
          : null,
        status: "saved",
        error: null,
        fieldErrors: {},
        conflictRevision: null,
      };
    }
    case "publicationUpdate": {
      if (action.publication.revision !== state.baseRevision) return state;
      const terminal = isTerminalPublicationStatus(action.publication.status);
      return {
        ...state,
        publicationStatus: action.publication.status,
        publicationGenerationId:
          action.publication.policyGenerationId ?? state.publicationGenerationId,
        publicationFailureCode: action.publication.failureCode,
        publicationPollingState: terminal ? "idle" : "polling",
        publicationError: null,
        publicationStartedAtMs: terminal ? null : state.publicationStartedAtMs,
      };
    }
    case "publicationUnavailable":
      if (action.revision !== state.baseRevision || state.publicationStartedAtMs === null) {
        return state;
      }
      return {
        ...state,
        publicationPollingState: "unavailable",
        publicationError: "暂时无法读取策略发布进度，将在超时前继续重试；尚未确认此策略已生效。",
      };
    case "publicationTimeout":
      if (action.revision !== state.baseRevision || state.publicationStartedAtMs === null) {
        return state;
      }
      return {
        ...state,
        publicationStatus: "expired",
        publicationPollingState: "timed_out",
        publicationError: "等待策略发布状态超时，轮询已停止；尚未确认此策略已生效。",
        publicationStartedAtMs: null,
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
  setConfig: (config: RoutingPolicyConfigV3) => void;
  save: () => Promise<RoutingPolicySnapshot | null>;
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

function shouldPollPublication(status: string | null): boolean {
  return status === "staged" || status === "ready";
}

function isTerminalPublicationStatus(status: RoutingPolicyPublicationStatus["status"]): boolean {
  return status === "active" || status === "failed" || status === "expired";
}

type PublicationPollWait = (delayMs: number, signal: AbortSignal) => Promise<void>;

export type RoutingPolicyPublicationPollOutcome =
  | "terminal"
  | "timed_out"
  | "cancelled";

export type RoutingPolicyPublicationPollOptions = {
  revision: number;
  policyGenerationId?: string | null;
  startedAtMs: number;
  signal: AbortSignal;
  fetchStatus: (
    input: RoutingPolicyPublicationStatusInput,
  ) => Promise<RoutingPolicyPublicationStatus>;
  onStatus: (status: RoutingPolicyPublicationStatus) => void;
  onUnavailable: () => void;
  intervalMs?: number;
  timeoutMs?: number;
  now?: () => number;
  wait?: PublicationPollWait;
};

type DeadlineResult<T> =
  | { kind: "value"; value: T }
  | { kind: "error" }
  | { kind: "timed_out" }
  | { kind: "cancelled" };

function withPollDeadline<T>(
  promise: Promise<T>,
  delayMs: number,
  signal: AbortSignal,
): Promise<DeadlineResult<T>> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (result: DeadlineResult<T>) => {
      if (settled) return;
      settled = true;
      globalThis.clearTimeout(timeout);
      signal.removeEventListener("abort", onAbort);
      resolve(result);
    };
    const onAbort = () => finish({ kind: "cancelled" });
    const timeout = globalThis.setTimeout(
      () => finish({ kind: "timed_out" }),
      Math.max(0, delayMs),
    );
    signal.addEventListener("abort", onAbort, { once: true });
    if (signal.aborted) {
      onAbort();
      return;
    }
    promise.then(
      (value) => finish({ kind: "value", value }),
      () => finish({ kind: "error" }),
    );
  });
}

function waitForPublicationPoll(delayMs: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    const onAbort = () => {
      globalThis.clearTimeout(timeout);
      resolve();
    };
    const timeout = globalThis.setTimeout(() => {
      signal.removeEventListener("abort", onAbort);
      resolve();
    }, Math.max(0, delayMs));
    signal.addEventListener("abort", onAbort, { once: true });
    if (signal.aborted) onAbort();
  });
}

export async function pollRoutingPolicyPublication(
  options: RoutingPolicyPublicationPollOptions,
): Promise<RoutingPolicyPublicationPollOutcome> {
  const intervalMs = options.intervalMs ?? ROUTING_POLICY_PUBLICATION_POLL_INTERVAL_MS;
  const timeoutMs = options.timeoutMs ?? ROUTING_POLICY_PUBLICATION_POLL_TIMEOUT_MS;
  const now = options.now ?? Date.now;
  const wait = options.wait ?? waitForPublicationPoll;
  let policyGenerationId = options.policyGenerationId ?? null;

  while (!options.signal.aborted) {
    const remainingMs = timeoutMs - (now() - options.startedAtMs);
    if (remainingMs <= 0) return "timed_out";
    const input: RoutingPolicyPublicationStatusInput = {
      revision: options.revision,
      policyGenerationId,
    };
    const result = await withPollDeadline(
      Promise.resolve().then(() => options.fetchStatus(input)),
      remainingMs,
      options.signal,
    );
    if (result.kind === "cancelled") return "cancelled";
    if (result.kind === "timed_out") return "timed_out";
    if (result.kind === "error") {
      options.onUnavailable();
    } else {
      const publication = result.value;
      options.onStatus(publication);
      policyGenerationId = publication.policyGenerationId ?? policyGenerationId;
      if (isTerminalPublicationStatus(publication.status)) return "terminal";
    }

    const afterRequestRemainingMs = timeoutMs - (now() - options.startedAtMs);
    if (afterRequestRemainingMs <= 0) return "timed_out";
    await wait(Math.min(intervalMs, afterRequestRemainingMs), options.signal);
  }
  return "cancelled";
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

  useEffect(() => {
    if (
      state.baseRevision === null ||
      state.publicationStartedAtMs === null ||
      !shouldPollPublication(state.publicationStatus)
    ) {
      return;
    }
    const revision = state.baseRevision;
    const controller = new AbortController();
    void pollRoutingPolicyPublication({
      revision,
      policyGenerationId: state.publicationGenerationId,
      startedAtMs: state.publicationStartedAtMs,
      signal: controller.signal,
      fetchStatus: (input) =>
        queryClient.fetchQuery(routingPolicyPublicationQueryOptions(input)),
      onStatus: (publication) => {
        dispatch({ type: "publicationUpdate", publication });
        if (publication.status === "active") {
          void queryClient.invalidateQueries({ queryKey: routingQueryKeys.all });
        }
      },
      onUnavailable: () => dispatch({ type: "publicationUnavailable", revision }),
    }).then((outcome) => {
      if (outcome === "timed_out" && !controller.signal.aborted) {
        dispatch({ type: "publicationTimeout", revision });
      }
    });
    return () => controller.abort();
  }, [
    queryClient,
    state.baseRevision,
    state.publicationGenerationId,
    state.publicationStartedAtMs,
    state.publicationStatus,
  ]);

  const setConfig = useCallback((config: RoutingPolicyConfigV3) => {
    dispatch({ type: "edit", config });
  }, []);

  const save = useCallback(async (): Promise<RoutingPolicySnapshot | null> => {
    if (!state.config || state.baseRevision === null || state.status === "saving") {
      return null;
    }
    dispatch({ type: "saveStart" });
    const input: ApplyRoutingPolicyDocumentInput = {
      formatVersion: 1,
      baseRevision: state.baseRevision,
      policy: state.config,
    };
    try {
      const snapshot = await applyRoutingPolicyDocument(input);
      dispatch({
        type: "saveSuccess",
        snapshot,
        publicationStartedAtMs: Date.now(),
      });
      queryClient.setQueryData(routingQueryKeys.policy(), snapshot);
      await queryClient.invalidateQueries({ queryKey: routingQueryKeys.all });
      return snapshot;
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
      return null;
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
