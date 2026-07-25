import type { BackendClient } from "./BackendClient";

let activeBackendClient: BackendClient | null = null;

export class BackendClientUnavailableError extends Error {
  readonly code = "runtime_unavailable" as const;
  readonly retryable = true;

  constructor() {
    super("Backend client is not installed.");
    this.name = "BackendClientUnavailableError";
  }
}

export function setActiveBackendClient(client: BackendClient | null): void {
  activeBackendClient = client;
}

export function getActiveBackendClient(): BackendClient {
  if (!activeBackendClient) {
    throw new BackendClientUnavailableError();
  }
  return activeBackendClient;
}
