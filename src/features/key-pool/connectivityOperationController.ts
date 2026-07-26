import {
  cancelOperation,
  getOperationStatus,
  startStationKeyConnectivityOperation,
  type OperationProgressDto,
  type OperationSnapshotDto,
  type OperationTerminalDto,
} from "@/lib/bridge/generated";
import type {
  StationKeyConnectivityTestEvent,
  StationKeyConnectivityTestResult,
} from "@/lib/types/stationKeys";

const RESULT_PREFIX = "station_key_connectivity.result ";
const DEFAULT_POLL_INTERVAL_MS = 600;
const DEFAULT_CANCEL_WAIT_MS = 1_000;

export type ConnectivityOperationInput = {
  stationKeyId: string;
  model: string;
};

export type ConnectivityOperationRunOptions = {
  pollIntervalMs?: number;
  signal?: AbortSignal;
  onEvent?: (event: StationKeyConnectivityTestEvent) => void;
  onOperationId?: (operationId: string) => void;
};

export class ConnectivityOperationCancelledError extends Error {
  constructor() {
    super("Connectivity operation was cancelled");
    this.name = "ConnectivityOperationCancelledError";
  }
}

export async function runStationKeyConnectivityOperation(
  input: ConnectivityOperationInput,
  options: ConnectivityOperationRunOptions = {},
): Promise<StationKeyConnectivityTestResult> {
  throwIfAborted(options.signal);
  const started = await startStationKeyConnectivityOperation(input);
  const operationId = started.operationId;
  options.onOperationId?.(operationId);
  let projectedResult: StationKeyConnectivityTestResult | null = null;
  let lastProgressSequence = 0;

  for (;;) {
    if (options.signal?.aborted) {
      await cancelConnectivityOperation(operationId);
      throw new ConnectivityOperationCancelledError();
    }

    const snapshot = await getOperationStatus({ operationId });
    for (const progress of newProgress(snapshot, lastProgressSequence)) {
      lastProgressSequence = Math.max(lastProgressSequence, progress.sequence);
      projectedResult = handleProgress(progress, projectedResult, options.onEvent);
    }

    const terminal = snapshot.terminal ?? terminalFromState(snapshot);
    if (terminal) {
      return resolveTerminal(terminal, projectedResult, options.onEvent);
    }

    try {
      await sleep(options.pollIntervalMs ?? DEFAULT_POLL_INTERVAL_MS, options.signal);
    } catch (error) {
      if (options.signal?.aborted) {
        await cancelConnectivityOperation(operationId);
        throw new ConnectivityOperationCancelledError();
      }
      throw error;
    }
  }
}

export async function cancelConnectivityOperation(operationId: string): Promise<void> {
  await cancelOperation({ operationId, waitMs: DEFAULT_CANCEL_WAIT_MS });
}

function newProgress(snapshot: OperationSnapshotDto, afterSequence: number) {
  return snapshot.progress
    .filter((progress) => progress.sequence > afterSequence)
    .sort((left, right) => left.sequence - right.sequence);
}

function handleProgress(
  progress: OperationProgressDto,
  currentResult: StationKeyConnectivityTestResult | null,
  onEvent: ((event: StationKeyConnectivityTestEvent) => void) | undefined,
) {
  const result = parseConnectivityResult(progress.message);
  if (result) {
    return result;
  }

  const attempt = /^attempt_started protocol=(\S+) model=(.+)$/.exec(progress.message);
  if (attempt) {
    onEvent?.({ type: "attemptStarted", protocol: attempt[1], model: attempt[2] });
    return currentResult;
  }

  const fallback = /^fallback reason=(.*)$/.exec(progress.message);
  if (fallback) {
    onEvent?.({ type: "fallback", reason: fallback[1] });
  }
  return currentResult;
}

function parseConnectivityResult(message: string): StationKeyConnectivityTestResult | null {
  if (!message.startsWith(RESULT_PREFIX)) {
    return null;
  }
  try {
    const raw = JSON.parse(message.slice(RESULT_PREFIX.length)) as Partial<StationKeyConnectivityTestResult>;
    if (
      typeof raw.stationKeyId === "string" &&
      typeof raw.ok === "boolean" &&
      typeof raw.statusCode === "number" &&
      typeof raw.durationMs === "number" &&
      typeof raw.model === "string" &&
      typeof raw.message === "string" &&
      (raw.responseMode === "stream" || raw.responseMode === "non_stream_fallback") &&
      (raw.streamFallbackReason === null || typeof raw.streamFallbackReason === "string")
    ) {
      return {
        stationKeyId: raw.stationKeyId,
        ok: raw.ok,
        statusCode: raw.statusCode,
        durationMs: raw.durationMs,
        model: raw.model,
        message: raw.message,
        responseMode: raw.responseMode,
        streamFallbackReason: raw.streamFallbackReason ?? null,
      };
    }
  } catch {
    return null;
  }
  return null;
}

function terminalFromState(snapshot: OperationSnapshotDto): OperationTerminalDto | null {
  return snapshot.state.state === "terminal" ? snapshot.state.terminal : null;
}

function resolveTerminal(
  terminal: OperationTerminalDto,
  result: StationKeyConnectivityTestResult | null,
  onEvent: ((event: StationKeyConnectivityTestEvent) => void) | undefined,
) {
  if (terminal.terminal === "completed") {
    if (!result) {
      throw new Error("Connectivity operation completed without a result projection");
    }
    onEvent?.({ type: "completed", ok: result.ok });
    return result;
  }
  if (terminal.terminal === "cancelled") {
    throw new ConnectivityOperationCancelledError();
  }
  const message = terminalMessage(terminal);
  onEvent?.({ type: "failed", message });
  throw new Error(message);
}

function terminalMessage(terminal: OperationTerminalDto) {
  switch (terminal.terminal) {
    case "failed":
      return `Connectivity operation failed: ${terminal.code}`;
    case "timed_out":
      return "Connectivity operation timed out";
    case "result_unknown":
      return "Connectivity operation result is unknown";
    case "cancelled":
      return "Connectivity operation was cancelled";
    case "completed":
      return "Connectivity operation completed without a result projection";
  }
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
      reject(new ConnectivityOperationCancelledError());
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
    throw new ConnectivityOperationCancelledError();
  }
}
