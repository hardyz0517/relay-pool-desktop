import { loadLocalRoutingWorkspaceApi } from "@/lib/api/localRouting";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";

export function loadLocalRoutingWorkspace(): Promise<LocalRoutingWorkspace> {
  return loadLocalRoutingWorkspaceApi();
}
