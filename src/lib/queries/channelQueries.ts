import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  ChannelMonitoringWorkspace,
  ChannelStatusWorkspace,
} from "@/lib/bridge/BackendClient";

export type { ChannelMonitoringWorkspace, ChannelStatusWorkspace } from "@/lib/bridge/BackendClient";

export function loadChannelMonitoringWorkspace(): Promise<ChannelMonitoringWorkspace> {
  return getActiveBackendClient().channels.loadChannelMonitoringWorkspace();
}

export function loadChannelStatusWorkspace(): Promise<ChannelStatusWorkspace> {
  return getActiveBackendClient().channels.loadChannelStatusWorkspace();
}
