import type { RuntimeContractInfo } from "./contract";

export type BackendMode = "desktop" | "demo";

export type BackendClient = {
  readonly mode: BackendMode;
  handshake(): Promise<RuntimeContractInfo>;
};

export class DemoBackendUnsupportedError extends Error {
  readonly code = "unsupported" as const;
  readonly retryable = false;
  readonly capability: string;

  constructor(capability: string) {
    super(`Demo backend does not support '${capability}'.`);
    this.name = "DemoBackendUnsupportedError";
    this.capability = capability;
  }
}
