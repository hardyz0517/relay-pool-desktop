import { validateRuntimeContract } from "@/app/bootstrap/runtimeContract";
import { getRuntimeContractInfo } from "./generated";
import type { BackendClient } from "./BackendClient";
import type { RuntimeContractInfo } from "./contract";
import { RuntimeContractMismatchError } from "./runtimeContractError";

export class DesktopBackend implements BackendClient {
  readonly mode = "desktop" as const;

  async handshake(): Promise<RuntimeContractInfo> {
    const payload = await getRuntimeContractInfo();
    const validation = validateRuntimeContract(payload);
    if (!validation.ok) {
      throw new RuntimeContractMismatchError(validation.reason);
    }
    return validation.contract;
  }
}

export function createDesktopBackendClient(): BackendClient {
  return new DesktopBackend();
}
