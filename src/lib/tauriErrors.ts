export function tauriErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function isTauriInvokeUnavailable(error: unknown) {
  return /invoke|__TAURI(?:_INTERNALS__)?/i.test(tauriErrorMessage(error));
}

export function isTauriCommandNotFound(error: unknown) {
  const message = tauriErrorMessage(error);
  return /command\s+.*not found|not allowed\.\s*command not found/i.test(message);
}
