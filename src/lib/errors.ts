import { isBackendError } from "@/lib/bridge/errors";

export function readError(error: unknown) {
  if (isBackendError(error) && error.details?.kind === "validation") {
    const validation = error.details.fields
      .map(({ field, message }) => `${field}: ${message}`)
      .join("; ");
    return `${error.message} (${validation})`;
  }
  return error instanceof Error ? error.message : String(error);
}
