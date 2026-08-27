import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";
import type {
  AlertPolicy,
  AlertPolicyInput,
  AlertingActivityInput,
  AlertingActivityPage,
  AlertingCurrentInput,
  AlertingHistoryInput,
  AlertingDomainClient,
  AlertingClearInput,
  AlertingMarkAllSeenInput,
  AlertingIncidentPage,
  AlertingSettings,
  AlertingSettingsInput,
  AlertingWorkspace,
} from "@/lib/types/alerting";
export type { AlertingDomainClient } from "@/lib/types/alerting";

type BackendWithAlerting = BackendClient;

function alertingClient(): AlertingDomainClient {
  const client = getActiveBackendClient() as BackendWithAlerting;
  return client.alerting;
}

export function loadAlertingWorkspace(): Promise<AlertingWorkspace> {
  return alertingClient().loadWorkspace();
}

export function updateAlertingSettings(input: AlertingSettingsInput): Promise<AlertingSettings> {
  return alertingClient().updateSettings(input);
}

export function upsertAlertPolicy(input: AlertPolicyInput): Promise<AlertPolicy> {
  return alertingClient().upsertPolicy(input);
}

export function deleteAlertPolicy(id: string, expectedRevision?: number): Promise<void> {
  return alertingClient().deletePolicy(id, expectedRevision);
}

export function sendTestAlertNotification(channel?: "in_app" | "desktop"): Promise<void> {
  return alertingClient().sendTestNotification(channel);
}

export function getDesktopNotificationPermission(): Promise<"allowed" | "denied" | "unavailable"> {
  return alertingClient().getDesktopNotificationPermission();
}

export function requestDesktopNotificationPermission(): Promise<"allowed" | "denied" | "unavailable"> {
  return alertingClient().requestDesktopNotificationPermission();
}

export function listCurrentAlertingIncidents(input: AlertingCurrentInput = {}): Promise<AlertingIncidentPage> {
  return alertingClient().listCurrentIncidents(input);
}

export function listAlertingActivity(input: AlertingActivityInput = {}): Promise<AlertingActivityPage> {
  return alertingClient().listActivity(input);
}

export function listAlertingOccurrences(input: AlertingHistoryInput) {
  return alertingClient().listOccurrences(input);
}

export function listAlertingDeliveries(input: AlertingHistoryInput) {
  return alertingClient().listDeliveries(input);
}

export function markAlertingSeen(activity: { recordType: "incident"; id: string; episodeNumber: number } | { recordType: "change"; id: string }): Promise<void> {
  return alertingClient().markSeen(activity);
}

export function markAllAlertingSeen(input: AlertingMarkAllSeenInput = {}): Promise<number> {
  return alertingClient().markAllSeen(input);
}

export function clearAlertingActivity(input: AlertingClearInput = {}): Promise<number> {
  return alertingClient().clearActivity(input);
}
