import {
  cancelOperation,
  getOperationStatus,
  getStationKeyModelDiscoveryOperationResult,
  startStationKeyModelDiscoveryOperation,
  type OperationSnapshotDto,
  type OperationTerminalDto,
  type StationKeyModelDiscoveryResultDto,
} from "@/lib/bridge/generated";

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

function sleep(ms: number, signal: AbortSignal | undefined) {
  return new Promise<void>((resolve, reject) => {
    const cleanup = () => signal?.removeEventListener("abort", abort);
    const timer = window.setTimeout(() => {
      cleanup();
      resolve();
    }, ms);
    const abort = () => {
      window.clearTimeout(timer);
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
