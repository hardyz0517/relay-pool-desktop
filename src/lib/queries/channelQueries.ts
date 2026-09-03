import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  ChannelMonitorLatestSummary,
  ChannelMonitoringWorkspace,
} from "@/lib/bridge/BackendClient";
import type {
  ChannelMonitorAttemptHistoryInput,
  ChannelMonitorAttemptPage,
  ChannelMonitorExecutionDetail,
  ChannelMonitorExecutionListInput,
  ChannelMonitorExecutionPage,
  ChannelStatusWorkspace,
  ChannelStatusWorkspaceInput,
  MonitoringCapabilityCatalog,
} from "@/lib/types/channelMonitors";
import type { ChannelMonitor } from "@/lib/types/channelMonitors";

export type { ChannelMonitoringWorkspace } from "@/lib/bridge/BackendClient";
export type { ChannelStatusWorkspace, ChannelStatusWorkspaceInput } from "@/lib/types/channelMonitors";

export function loadChannelMonitoringWorkspace(): Promise<ChannelMonitoringWorkspace> {
  return getActiveBackendClient().channels.loadChannelMonitoringWorkspace();
}

export function loadChannelMonitorLatestSummary(): Promise<ChannelMonitorLatestSummary[]> {
  return getActiveBackendClient().channels.loadChannelMonitorLatestSummary();
}

export function listChannelMonitors(): Promise<ChannelMonitor[]> {
  return getActiveBackendClient().channels.listChannelMonitors();
}

export function listChannelMonitorTemplates(): Promise<import("@/lib/types/channelMonitors").ChannelMonitorRequestTemplate[]> {
  return getActiveBackendClient().channels.listChannelMonitorTemplates();
}

export function loadChannelStatusWorkspace(input: ChannelStatusWorkspaceInput = {}): Promise<ChannelStatusWorkspace> {
  return getActiveBackendClient().channels.loadChannelStatusWorkspace(input);
}

export function listChannelMonitorExecutions(
  input: ChannelMonitorExecutionListInput = {},
): Promise<ChannelMonitorExecutionPage> {
  return getActiveBackendClient().channels.listChannelMonitorExecutions(input);
}

export function getChannelMonitorExecution(executionId: string): Promise<ChannelMonitorExecutionDetail> {
  return getActiveBackendClient().channels.getChannelMonitorExecution(executionId);
}

export function listChannelMonitorAttempts(input: ChannelMonitorAttemptHistoryInput): Promise<ChannelMonitorAttemptPage> {
  return getActiveBackendClient().channels.listChannelMonitorAttempts(input);
}

export function listMonitoringCapabilities(): Promise<MonitoringCapabilityCatalog> {
  return getActiveBackendClient().channels.listMonitoringCapabilities();
}
