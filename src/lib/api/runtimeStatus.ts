import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { RuntimeStatus } from "@/lib/types/runtimeStatus";

export function getRuntimeStatus(): Promise<RuntimeStatus> {
  return getActiveBackendClient().runtime.getRuntimeStatus();
}
