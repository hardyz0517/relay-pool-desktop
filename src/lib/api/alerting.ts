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
  AlertingIncident,
  AlertingIncidentInput,
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

export function getAlertingSettings(): Promise<AlertingSettings> {
  return alertingClient().getSettings();
}

export function updateAlertingSettings(input: AlertingSettingsInput): Promise<AlertingSettings> {
  return alertingClient().updateSettings(input);
}

export function listAlertPolicies(): Promise<AlertPolicy[]> {
  return alertingClient().listPolicies();
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

export function getAlertingIncident(input: AlertingIncidentInput): Promise<AlertingIncident> {
  return alertingClient().getIncident(input);
}

export function listAlertingOccurrences(input: AlertingHistoryInput) {
  return alertingClient().listOccurrences(input);
}

export function listAlertingDeliveries(input: AlertingHistoryInput) {
  return alertingClient().listDeliveries(input);
}

export function markAlertingSeen(incidentId: string, episodeNumber: number): Promise<void> {
  return alertingClient().markSeen(incidentId, episodeNumber);
}

export function markAllAlertingSeen(input: AlertingMarkAllSeenInput = {}): Promise<number> {
  return alertingClient().markAllSeen(input);
}

export function resolveAllAlertingIncidents(input: AlertingMarkAllSeenInput = {}): Promise<number> {
  return alertingClient().resolveAllActive(input);
}

export function clearAlertingIncidents(input: AlertingClearInput = {}): Promise<number> {
  return alertingClient().clearIncidents(input);
}

export function snoozeAlertingIncident(incidentId: string, episodeNumber: number, untilMs: number): Promise<void> {
  return alertingClient().snooze(incidentId, episodeNumber, untilMs);
}
