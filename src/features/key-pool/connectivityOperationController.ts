import {
  cancelOperation,
  getOperationStatus,
  getStationKeyConnectivityOperationResult,
  startStationKeyConnectivityOperation,
  type OperationProgressDto,
  type OperationSnapshotDto,
  type OperationTerminalDto,
} from "@/lib/bridge/generated";
import { runUserInteraction } from "@/lib/bridge/runtimeContext";
import type {
  StationKeyConnectivityProgressEvent,
  StationKeyConnectivityTestResult,
  StationKeyConnectivityClientProfile,
} from "@/lib/types/stationKeys";

const DEFAULT_POLL_INTERVAL_MS = 600;
const DEFAULT_CANCEL_WAIT_MS = 1_000;

export type ConnectivityOperationInput = {
  stationKeyId: string;
  model: string;
  clientProfile?: StationKeyConnectivityClientProfile;
};

export type ConnectivityOperationRunOptions = {
  pollIntervalMs?: number;
  signal?: AbortSignal;
  onEvent?: (event: StationKeyConnectivityProgressEvent) => void;
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
  // The initial non-idempotent dispatch belongs to the user's click. Once it
  // returns, polling and cancellation are operation-owned background work and
  // must not retain a mutable interaction id across awaits.
  const started = await runUserInteraction(() => startStationKeyConnectivityOperation(input));
  const operationId = started.operationId;
  options.onOperationId?.(operationId);
  let lastProgressSequence = 0;

  for (;;) {
    if (options.signal?.aborted) {
      await cancelConnectivityOperation(operationId);
      throw new ConnectivityOperationCancelledError();
    }

    const snapshot = await getOperationStatus({ operationId });
    for (const progress of newProgress(snapshot, lastProgressSequence)) {
      lastProgressSequence = Math.max(lastProgressSequence, progress.sequence);
      handleProgress(progress, options.onEvent);
    }

    const terminal = snapshot.terminal ?? terminalFromState(snapshot);
    if (terminal) {
      return resolveTerminal(operationId, terminal, options.onEvent);
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
  onEvent: ((event: StationKeyConnectivityProgressEvent) => void) | undefined,
) {
  const attempt = /^attempt_started protocol=(\S+) model=(.+)$/.exec(progress.message);
  if (attempt) {
    onEvent?.({ type: "attemptStarted", protocol: attempt[1], model: attempt[2] });
    return;
  }

  const fallback = /^fallback reason=(.*)$/.exec(progress.message);
  if (fallback) {
    onEvent?.({ type: "fallback", reason: fallback[1] });
  }
}

function terminalFromState(snapshot: OperationSnapshotDto): OperationTerminalDto | null {
  return snapshot.state.state === "terminal" ? snapshot.state.terminal : null;
}

async function resolveTerminal(
  operationId: string,
  terminal: OperationTerminalDto,
  onEvent: ((event: StationKeyConnectivityProgressEvent) => void) | undefined,
): Promise<StationKeyConnectivityTestResult> {
  if (terminal.terminal === "completed") {
    const result = await getStationKeyConnectivityOperationResult({ operationId });
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
      return "Connectivity operation completed without a result";
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
