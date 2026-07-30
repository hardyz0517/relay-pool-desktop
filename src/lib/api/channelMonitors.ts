import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { ChannelMonitorSummaryOptions } from "@/lib/bridge/BackendClient";
import type {
  CreateChannelMonitorInput,
  CreateChannelMonitorTemplateInput,
  ChannelMonitorAttemptHistoryInput,
  ChannelMonitorExecutionListInput,
  ChannelStatusWorkspaceInput,
  UpdateChannelMonitorInput,
  UpdateChannelMonitorTemplateInput,
} from "@/lib/types/channelMonitors";

export type { ChannelMonitorSummaryOptions } from "@/lib/bridge/BackendClient";

export function listChannelMonitors() {
  return getActiveBackendClient().channels.listChannelMonitors();
}

export function listChannelMonitorSummaries(options: ChannelMonitorSummaryOptions = {}) {
  return getActiveBackendClient().channels.listChannelMonitorSummaries(options);
}

export function listChannelStatusSummaries() {
  return getActiveBackendClient().channels.listChannelStatusSummaries();
}

export function createChannelMonitor(input: CreateChannelMonitorInput) {
  return getActiveBackendClient().channels.createChannelMonitor(input);
}

export function updateChannelMonitor(input: UpdateChannelMonitorInput) {
  return getActiveBackendClient().channels.updateChannelMonitor(input);
}

export function deleteChannelMonitor(id: string) {
  return getActiveBackendClient().channels.deleteChannelMonitor(id);
}

export function runChannelMonitorNow(monitorId: string) {
  return getActiveBackendClient().channels.runChannelMonitorNow(monitorId);
}

export function runChannelMonitorNowWithTrigger(monitorId: string, triggerRequestId: string) {
  return getActiveBackendClient().channels.runChannelMonitorNow(monitorId, triggerRequestId);
}

export function cancelChannelMonitorExecution(executionId: string) {
  return getActiveBackendClient().channels.cancelChannelMonitorExecution(executionId);
}

export function listChannelMonitorExecutions(input: ChannelMonitorExecutionListInput = {}) {
  return getActiveBackendClient().channels.listChannelMonitorExecutions(input);
}

export function getChannelMonitorExecution(executionId: string) {
  return getActiveBackendClient().channels.getChannelMonitorExecution(executionId);
}

export function listChannelMonitorAttempts(input: ChannelMonitorAttemptHistoryInput) {
  return getActiveBackendClient().channels.listChannelMonitorAttempts(input);
}

export function listMonitoringCapabilities() {
  return getActiveBackendClient().channels.listMonitoringCapabilities();
}

export function listChannelMonitorRuns(monitorId: string) {
  return getActiveBackendClient().channels.listChannelMonitorRuns(monitorId);
}

export function listChannelMonitorTemplates() {
  return getActiveBackendClient().channels.listChannelMonitorTemplates();
}

export function createChannelMonitorTemplate(input: CreateChannelMonitorTemplateInput) {
  return getActiveBackendClient().channels.createChannelMonitorTemplate(input);
}

export function updateChannelMonitorTemplate(input: UpdateChannelMonitorTemplateInput) {
  return getActiveBackendClient().channels.updateChannelMonitorTemplate(input);
}

export function duplicateChannelMonitorTemplate(id: string) {
  return getActiveBackendClient().channels.duplicateChannelMonitorTemplate(id);
}

export function deleteChannelMonitorTemplate(id: string) {
  return getActiveBackendClient().channels.deleteChannelMonitorTemplate(id);
}

export function loadChannelStatusWorkspace(input: ChannelStatusWorkspaceInput = {}) {
  return getActiveBackendClient().channels.loadChannelStatusWorkspace(input);
}
