import { queryOptions } from "@tanstack/react-query";
import {
  getAlertingIncident,
  listAlertingDeliveries,
  getAlertingSettings,
  listAlertingActivity,
  listCurrentAlertingIncidents,
  listAlertingOccurrences,
  listAlertPolicies,
  loadAlertingWorkspace,
} from "@/lib/api/alerting";
import type { AlertingActivityInput, AlertingCurrentInput, AlertingHistoryInput, AlertingIncidentInput } from "@/lib/types/alerting";
import { queryKeys } from "@/lib/query/queryKeys";

export const alertingWorkspaceQueryOptions = () =>
  queryOptions({
    queryKey: queryKeys.alertingWorkspace,
    queryFn: loadAlertingWorkspace,
    staleTime: 30_000,
  });

export const alertingSettingsQueryOptions = () =>
  queryOptions({
    queryKey: queryKeys.alertingSettings,
    queryFn: getAlertingSettings,
    staleTime: 30_000,
  });

export const alertPoliciesQueryOptions = () =>
  queryOptions({
    queryKey: queryKeys.alertPolicies,
    queryFn: listAlertPolicies,
    staleTime: 30_000,
  });

export const alertingCurrentQueryOptions = (input: AlertingCurrentInput = {}) =>
  queryOptions({
    queryKey: queryKeys.alertingCurrent(input),
    queryFn: () => listCurrentAlertingIncidents(input),
    staleTime: 2_000,
  });

export const alertingActivityQueryOptions = (input: AlertingActivityInput = {}) =>
  queryOptions({
    queryKey: queryKeys.alertingActivity(input),
    queryFn: () => listAlertingActivity(input),
    staleTime: 2_000,
  });

/**
 * The sidebar badge and Change Center summary intentionally share this exact
 * server-side count. Keep the filter here so their meaning cannot drift.
 */
export const unreadChangeActivityQueryOptions = () =>
  alertingActivityQueryOptions({
    recordType: "change",
    unreadOnly: true,
    limit: 1,
  });

export const alertingIncidentQueryOptions = (input: AlertingIncidentInput) =>
  queryOptions({
    queryKey: queryKeys.alertingIncident(input.incidentId, input.episodeNumber),
    queryFn: () => getAlertingIncident(input),
    staleTime: 2_000,
  });

export const alertingOccurrencesQueryOptions = (input: AlertingHistoryInput) =>
  queryOptions({
    queryKey: queryKeys.alertingOccurrences(input),
    queryFn: () => listAlertingOccurrences(input),
    staleTime: 2_000,
  });

export const alertingDeliveriesQueryOptions = (input: AlertingHistoryInput) =>
  queryOptions({
    queryKey: queryKeys.alertingDeliveries(input),
    queryFn: () => listAlertingDeliveries(input),
    staleTime: 2_000,
  });
