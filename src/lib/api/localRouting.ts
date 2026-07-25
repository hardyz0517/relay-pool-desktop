import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { LocalRoutingWorkspace, ReorderLocalRoutingKeysInput } from "@/lib/types/localRouting";

export function loadLocalRoutingWorkspaceApi(): Promise<LocalRoutingWorkspace> {
  return getActiveBackendClient().localRouting.loadLocalRoutingWorkspace();
}

export function reorderLocalRoutingKeys(input: ReorderLocalRoutingKeysInput): Promise<LocalRoutingWorkspace> {
  return getActiveBackendClient().localRouting.reorderLocalRoutingKeys(input);
}
