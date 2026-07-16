export type TauriInvokeErrorKind = "acl-denied" | "command-not-found" | "runtime-unavailable" | "other";

export function tauriErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function classifyTauriInvokeError(error: unknown): TauriInvokeErrorKind {
  const message = tauriErrorMessage(error);
  if (/not allowed by ACL/i.test(message)) return "acl-denied";
  if (/^Command\s+[^\s]+\s+not found$/i.test(message.trim())) return "command-not-found";
  if (/invoke|__TAURI(?:_INTERNALS__)?/i.test(message)) {
    return "runtime-unavailable";
  }
  return "other";
}

export function isTauriInvokeUnavailable(error: unknown) {
  return classifyTauriInvokeError(error) === "runtime-unavailable";
}

export function isTauriCommandNotFound(error: unknown) {
  return classifyTauriInvokeError(error) === "command-not-found";
}
