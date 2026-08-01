import { getStationKeyCapabilities, updateStationKeyCapabilities } from "@/lib/api/routing";
import {
  cancelOperation,
  getOperationStatus,
  getStationKeyModelDiscoveryOperationResult,
  startStationKeyModelDiscoveryOperation,
  type OperationSnapshotDto,
  type OperationTerminalDto,
  type StationKeyModelDiscoveryResultDto,
} from "@/lib/bridge/generated";
import type { StationKeyCapabilities, UpdateStationKeyCapabilitiesInput } from "@/lib/types/routing";

const DEFAULT_POLL_INTERVAL_MS = 400;
const DEFAULT_CANCEL_WAIT_MS = 1_000;

export class ModelDiscoveryOperationCancelledError extends Error {
  constructor() {
    super("Model discovery operation was cancelled");
    this.name = "ModelDiscoveryOperationCancelledError";
  }
}

export async function runStationKeyModelDiscoveryOperation(
  stationKeyId: string,
  options: { pollIntervalMs?: number; signal?: AbortSignal } = {},
): Promise<StationKeyModelDiscoveryResultDto> {
  throwIfAborted(options.signal);
  const started = await startStationKeyModelDiscoveryOperation({ stationKeyId });
  const operationId = started.operationId;

  for (;;) {
    if (options.signal?.aborted) {
      await cancelOperation({ operationId, waitMs: DEFAULT_CANCEL_WAIT_MS });
      throw new ModelDiscoveryOperationCancelledError();
    }

    const snapshot = await getOperationStatus({ operationId });
    const terminal = snapshot.terminal ?? terminalFromState(snapshot);
    if (terminal) {
      return resolveTerminal(operationId, terminal);
    }

    try {
      await sleep(options.pollIntervalMs ?? DEFAULT_POLL_INTERVAL_MS, options.signal);
    } catch (error) {
      if (options.signal?.aborted) {
        await cancelOperation({ operationId, waitMs: DEFAULT_CANCEL_WAIT_MS });
        throw new ModelDiscoveryOperationCancelledError();
      }
      throw error;
    }
  }
}

export type PersistDiscoveredStationKeyModelsDependencies = {
  runDiscovery: (stationKeyId: string) => Promise<StationKeyModelDiscoveryResultDto>;
  getCapabilities: (stationKeyId: string) => Promise<StationKeyCapabilities>;
  updateCapabilities: (input: UpdateStationKeyCapabilitiesInput) => Promise<StationKeyCapabilities>;
};

const defaultPersistDependencies: PersistDiscoveredStationKeyModelsDependencies = {
  runDiscovery: runStationKeyModelDiscoveryOperation,
  getCapabilities: getStationKeyCapabilities,
  updateCapabilities: updateStationKeyCapabilities,
};

export async function discoverAndPersistStationKeyModels(
  stationKeyId: string,
  dependencies: PersistDiscoveredStationKeyModelsDependencies = defaultPersistDependencies,
) {
  const result = await dependencies.runDiscovery(stationKeyId);
  const models = normalizeDiscoveredModels(result.models);
  if (models.length === 0) {
    return { ...result, models };
  }

  const capabilities = await dependencies.getCapabilities(stationKeyId);
  await dependencies.updateCapabilities({
    stationKeyId,
    supportsChatCompletions: capabilities.supportsChatCompletions,
    supportsResponses: capabilities.supportsResponses,
    supportsEmbeddings: capabilities.supportsEmbeddings,
    supportsStream: capabilities.supportsStream,
    supportsTools: capabilities.supportsTools,
    supportsVision: capabilities.supportsVision,
    supportsReasoning: capabilities.supportsReasoning,
    modelAllowlist: models,
    modelBlocklist: capabilities.modelBlocklist,
    preferredModels: capabilities.preferredModels,
    onlyUseAsBackup: capabilities.onlyUseAsBackup,
    routingTags: capabilities.routingTags,
  });
  return { ...result, models };
}

export type CreatedKeyModelDiscoverySummary = {
  requestedCount: number;
  updatedCount: number;
  emptyCount: number;
  modelCount: number;
  failures: Array<{ stationKeyId: string; error: unknown }>;
};

export async function discoverCreatedStationKeyModels(
  stationKeyIds: string[],
  discover: (stationKeyId: string) => Promise<StationKeyModelDiscoveryResultDto> = discoverAndPersistStationKeyModels,
): Promise<CreatedKeyModelDiscoverySummary> {
  const uniqueIds = [...new Set(stationKeyIds.filter(Boolean))];
  const results = await Promise.all(
    uniqueIds.map(async (stationKeyId) => {
      try {
        return { stationKeyId, result: await discover(stationKeyId), error: null };
      } catch (error) {
        return { stationKeyId, result: null, error };
      }
    }),
  );

  return results.reduce<CreatedKeyModelDiscoverySummary>(
    (summary, item) => {
      if (item.error) {
        summary.failures.push({ stationKeyId: item.stationKeyId, error: item.error });
      } else if (item.result && item.result.models.length > 0) {
        summary.updatedCount += 1;
        summary.modelCount += item.result.models.length;
      } else {
        summary.emptyCount += 1;
      }
      return summary;
    },
    {
      requestedCount: uniqueIds.length,
      updatedCount: 0,
      emptyCount: 0,
      modelCount: 0,
      failures: [],
    },
  );
}

function terminalFromState(snapshot: OperationSnapshotDto): OperationTerminalDto | null {
  return snapshot.state.state === "terminal" ? snapshot.state.terminal : null;
}

async function resolveTerminal(
  operationId: string,
  terminal: OperationTerminalDto,
): Promise<StationKeyModelDiscoveryResultDto> {
  if (terminal.terminal === "completed") {
    return getStationKeyModelDiscoveryOperationResult({ operationId });
  }
  if (terminal.terminal === "cancelled") {
    throw new ModelDiscoveryOperationCancelledError();
  }
  if (terminal.terminal === "timed_out") {
    throw new Error("获取模型列表超时");
  }
  if (terminal.terminal === "result_unknown") {
    throw new Error("无法确认模型列表获取结果");
  }
  throw new Error(modelDiscoveryFailureMessage(terminal.code));
}

function modelDiscoveryFailureMessage(code: string) {
  if (code === "model-discovery-http") {
    return "接口拒绝了模型列表请求，请检查密钥权限和 API Base URL";
  }
  if (code === "model-discovery-invalid-response") {
    return "接口返回的模型列表格式无法识别";
  }
  if (code === "model-discovery-request-invalid") {
    return "API Base URL 无法用于获取模型列表";
  }
  return "获取模型列表失败，请检查网络和接口配置";
}

function normalizeDiscoveredModels(models: string[]) {
  const seen = new Set<string>();
  return models
    .flatMap((model) => {
      const value = model.trim();
      const normalized = value.toLowerCase();
      if (!value || seen.has(normalized)) {
        return [];
      }
      seen.add(normalized);
      return [value];
    })
    .sort((left, right) => left.localeCompare(right, undefined, { numeric: true, sensitivity: "base" }));
}

function sleep(ms: number, signal: AbortSignal | undefined) {
  return new Promise<void>((resolve, reject) => {
    const cleanup = () => signal?.removeEventListener("abort", abort);
    const timer = globalThis.setTimeout(() => {
      cleanup();
      resolve();
    }, ms);
    const abort = () => {
      globalThis.clearTimeout(timer);
      cleanup();
      reject(new ModelDiscoveryOperationCancelledError());
    };
    if (signal?.aborted) {
      abort();
      return;
    }
    signal?.addEventListener("abort", abort, { once: true });
  });
}

function throwIfAborted(signal: AbortSignal | undefined) {
  if (signal?.aborted) {
    throw new ModelDiscoveryOperationCancelledError();
  }
}
