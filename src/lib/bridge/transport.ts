import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  BackendError,
  isCommandErrorEnvelope,
  ResultUnknownError,
  toBackendError,
} from "./errors";
import type { IpcCommand } from "./generated";
import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";

/**
 * The only frontend entry point for ordinary Tauri commands.
 * Preview-only callers still receive the original runtime-unavailable error so
 * their existing demo fallback can make an explicit environment decision.
 */
export function invoke<T>(command: IpcCommand, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(command, args).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      throw error;
    }
    throw toBackendError(error);
  });
}

/**
 * Non-idempotent commands must never be replayed after an ambiguous transport
 * failure. A typed command error proves the backend returned a result; any
 * other post-dispatch rejection leaves the commit state unknown.
 */
export function invokeNonIdempotent<T>(
  command: IpcCommand,
  args?: Record<string, unknown>,
): Promise<T> {
  return tauriInvoke<T>(command, args).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      throw error;
    }
    throw classifyNonIdempotentRejection(command, error);
  });
}

export function classifyNonIdempotentRejection(
  command: IpcCommand,
  error: unknown,
): BackendError | ResultUnknownError {
  return isCommandErrorEnvelope(error)
    ? toBackendError(error)
    : new ResultUnknownError(command, error);
}
