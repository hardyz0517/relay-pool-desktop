/** Frontend contract for the alerting settings workspace. */

export type AlertSeverity = "critical" | "warning" | "info";
export type AlertEventType =
  | "group_missing" | "key_group_unresolved" | "balance_low" | "balance_depleted"
  | "price_expired" | "key_invalid" | "collector_failed" | "station_down"
  | "route_impacted" | "group_added" | "rate_changed" | "group_rate_changed"
  | "price_changed" | "model_added" | "model_removed" | "audit_change";
export type AlertScope = "global" | "event_type" | "station" | "station_key";
export type AlertPolicyState = "active" | "disabled" | "orphaned" | "tombstone";
export type AlertTriggerMode = "immediate" | "consecutive_occurrences" | "active_duration";
export type AlertRecoveryMode = "consecutive_healthy" | "healthy_duration";
export type AlertRepeatMode = "never" | "interval" | "severity_escalation" | "interval_and_escalation";
export type AlertQuietHoursPolicy = "inherit" | "respect" | "bypass_for_critical";

export type AlertPolicy = {
  id: string;
  name: string;
  enabled: boolean;
  state: AlertPolicyState;
  scopeKind: AlertScope;
  eventType: AlertEventType | null;
  stationId: string | null;
  stationKeyId: string | null;
  minimumSeverity: AlertSeverity | null;
  severityOffset: -1 | 0 | 1;
  triggerMode: AlertTriggerMode;
  triggerCount: number | null;
  triggerDurationSeconds: number | null;
  recoveryMode: AlertRecoveryMode;
  recoveryCount: number | null;
  recoveryDurationSeconds: number | null;
  inAppEnabled: boolean;
  desktopEnabled: boolean;
  repeatMode: AlertRepeatMode;
  repeatIntervalSeconds: number | null;
  cooldownSeconds: number;
  recoveryNotificationEnabled: boolean;
  quietHoursPolicy: AlertQuietHoursPolicy;
  priority: number;
  revision: number;
  createdAtMs: number;
  updatedAtMs: number;
};

export type AlertPolicyInput = Omit<AlertPolicy, "revision" | "createdAtMs" | "updatedAtMs"> & {
  id?: string;
  expectedRevision?: number;
};

export type AlertingSettings = {
  enabled: boolean;
  inAppEnabled: boolean;
  desktopEnabled: boolean;
  paused: boolean;
  globalPauseUntilMs: number | null;
  quietHoursEnabled: boolean;
  quietHoursStart: string;
  quietHoursEnd: string;
  quietHoursTimezone: string;
  criticalBypassesQuietHours: boolean;
  historyRetentionDays: number;
  deliveryRetentionDays: number;
  revision: number;
  updatedAtMs: number;
};
export type AlertingSettingsInput = Omit<AlertingSettings, "revision" | "updatedAtMs"> & { expectedRevision?: number };
export type AlertingWorkspace = { settings: AlertingSettings; policies: AlertPolicy[] };

export function toAlertingSettingsInput(settings: AlertingSettings): AlertingSettingsInput {
  const { revision, updatedAtMs: _updatedAtMs, ...input } = settings;
  return { ...input, expectedRevision: revision };
}

export function toAlertPolicyInput(policy: AlertPolicy, expectedRevision?: number): AlertPolicyInput {
  const { revision: _revision, createdAtMs: _createdAtMs, updatedAtMs: _updatedAtMs, ...input } = policy;
  return expectedRevision == null ? input : { ...input, expectedRevision };
}

export type AlertingCurrentInput = {
  stationId?: string | null;
  severity?: AlertSeverity | null;
  lifecycleState?: "active" | "unread" | "pending" | "open" | "recovering" | "resolved" | null;
  cursor?: AlertingCursor | null;
  limit?: number;
};

export type AlertingActivityInput = {
  stationId?: string | null;
  severity?: AlertSeverity | null;
  cursor?: AlertingCursor | null;
  limit?: number;
};

export type AlertingCursor = { updatedAtMs: number; id: string };

export type AlertingIncident = {
  id: string;
  conditionKey: string;
  eventType: string;
  lifecycleState: "pending" | "open" | "recovering" | "resolved" | string;
  severity: AlertSeverity;
  stationId: string | null;
  episodeNumber: number;
  occurrenceCount: number;
  lastSeenAtMs: number;
  collectorFailedTaskTypes: string[];
  resolvedAtMs: number | null;
  updatedAtMs: number;
  seenAtMs: number | null;
  snoozedUntilMs: number | null;
};

export type AlertingIncidentPage = {
  items: AlertingIncident[];
  nextCursor: AlertingCursor | null;
  activeCount: number;
  unseenCount: number;
};

type AlertingActivityBase = {
  id: string;
  eventType: string;
  severity: AlertSeverity;
  stationId: string | null;
  objectType: string | null;
  objectId: string | null;
  stationKeyId: string | null;
  source: string | null;
  reasonCode: string | null;
  activityAtMs: number;
  oldValueJson: string | null;
  newValueJson: string | null;
  impactJson: string | null;
};

export type AlertingIncidentActivity = AlertingActivityBase & AlertingIncident & {
  recordType: "incident";
};

export type AlertingChangeActivity = AlertingActivityBase & {
  recordType: "change";
  conditionKey: string | null;
  lifecycleState: null;
  episodeNumber: null;
  occurrenceCount: null;
  collectorFailedTaskTypes: string[];
  resolvedAtMs: null;
  seenAtMs: null;
  snoozedUntilMs: null;
};

export type AlertingActivity = AlertingIncidentActivity | AlertingChangeActivity;

export type AlertingActivityPage = {
  items: AlertingActivity[];
  nextCursor: AlertingCursor | null;
  activeCount: number;
  unseenCount: number;
};

export type AlertingIncidentInput = { incidentId: string; episodeNumber: number };
export type AlertingMarkAllSeenInput = {
  stationId?: string | null;
  severity?: AlertSeverity | null;
};
export type AlertingClearScope = "active" | "unread" | "resolved";
export type AlertingClearInput = AlertingMarkAllSeenInput & {
  lifecycleState?: AlertingClearScope | null;
};
export type AlertingHistoryInput = AlertingIncidentInput & {
  cursor?: AlertingCursor | null;
  limit?: number;
};
export type AlertingOccurrence = {
  id: string;
  sourceObservationKey: string;
  eventType: string;
  observationKind: string;
  severity: string;
  reasonCode: string | null;
  source: string;
  objectType: string;
  objectId: string | null;
  stationId: string | null;
  stationKeyId: string | null;
  observedAtMs: number;
};
export type AlertingOccurrencePage = {
  items: AlertingOccurrence[];
  nextCursor: AlertingCursor | null;
};
export type AlertingDelivery = {
  id: string;
  deliveryKey: string;
  channel: string;
  deliveryKind: string;
  status: string;
  scheduledAtMs: number;
  attemptCount: number;
  deliveredAtMs: number | null;
  suppressedReason: string | null;
  errorCode: string | null;
  createdAtMs: number;
  updatedAtMs: number;
};
export type AlertingDeliveryPage = {
  items: AlertingDelivery[];
  nextCursor: AlertingCursor | null;
};

export type AlertingDomainClient = {
  loadWorkspace(): Promise<AlertingWorkspace>;
  getSettings(): Promise<AlertingSettings>;
  updateSettings(input: AlertingSettingsInput): Promise<AlertingSettings>;
  listPolicies(): Promise<AlertPolicy[]>;
  upsertPolicy(input: AlertPolicyInput): Promise<AlertPolicy>;
  deletePolicy(id: string, expectedRevision?: number): Promise<void>;
  listCurrentIncidents(input?: AlertingCurrentInput): Promise<AlertingIncidentPage>;
  listActivity(input?: AlertingActivityInput): Promise<AlertingActivityPage>;
  getIncident(input: AlertingIncidentInput): Promise<AlertingIncident>;
  listOccurrences(input: AlertingHistoryInput): Promise<AlertingOccurrencePage>;
  listDeliveries(input: AlertingHistoryInput): Promise<AlertingDeliveryPage>;
  markSeen(incidentId: string, episodeNumber: number): Promise<void>;
  markAllSeen(input?: AlertingMarkAllSeenInput): Promise<number>;
  resolveAllActive(input?: AlertingMarkAllSeenInput): Promise<number>;
  clearIncidents(input?: AlertingClearInput): Promise<number>;
  snooze(incidentId: string, episodeNumber: number, untilMs: number): Promise<void>;
  sendTestNotification(channel?: "in_app" | "desktop"): Promise<void>;
  getDesktopNotificationPermission(): Promise<"allowed" | "denied" | "unavailable">;
  requestDesktopNotificationPermission(): Promise<"allowed" | "denied" | "unavailable">;
};

export type AlertingEventOption = {
  value: AlertEventType;
  label: string;
  description: string;
  defaultSeverity: AlertSeverity;
  configurable: boolean;
};

export const ALERT_EVENT_OPTIONS: readonly AlertingEventOption[] = [
  { value: "collector_failed", label: "采集失败", description: "采集任务执行失败", defaultSeverity: "warning", configurable: true },
  { value: "station_down", label: "站点不可用", description: "站点健康检查失败", defaultSeverity: "critical", configurable: true },
  { value: "balance_low", label: "余额偏低", description: "余额低于配置的阈值", defaultSeverity: "warning", configurable: true },
  { value: "balance_depleted", label: "余额耗尽", description: "余额已经耗尽", defaultSeverity: "critical", configurable: true },
  { value: "group_missing", label: "分组缺失", description: "绑定的远程分组不存在", defaultSeverity: "warning", configurable: true },
  { value: "key_group_unresolved", label: "密钥分组无法解析", description: "密钥无法解析到有效分组", defaultSeverity: "warning", configurable: true },
  { value: "price_expired", label: "价格已过期", description: "价格快照已过期", defaultSeverity: "warning", configurable: true },
  { value: "key_invalid", label: "密钥无效", description: "站点密钥校验失败", defaultSeverity: "critical", configurable: true },
  { value: "route_impacted", label: "路由受影响", description: "可用路由候选减少", defaultSeverity: "warning", configurable: true },
  { value: "group_added", label: "新增分组", description: "发现新的远程分组", defaultSeverity: "info", configurable: true },
  { value: "rate_changed", label: "倍率变化", description: "站点倍率发生变化", defaultSeverity: "info", configurable: true },
  { value: "group_rate_changed", label: "分组倍率变化", description: "分组倍率发生变化", defaultSeverity: "info", configurable: true },
  { value: "price_changed", label: "价格变化", description: "模型价格发生变化", defaultSeverity: "info", configurable: true },
  { value: "model_added", label: "新增模型", description: "发现新的模型", defaultSeverity: "info", configurable: true },
  { value: "model_removed", label: "模型移除", description: "模型不再可用", defaultSeverity: "info", configurable: true },
  { value: "audit_change", label: "配置变化", description: "配置或策略发生变化", defaultSeverity: "info", configurable: true },
];

export const AUDIT_ALERT_EVENT_TYPES: readonly AlertEventType[] = [
  "group_added",
  "rate_changed",
  "group_rate_changed",
  "price_changed",
  "model_added",
  "model_removed",
  "audit_change",
];

export function isAuditAlertEvent(eventType: AlertEventType | null | undefined): boolean {
  return eventType != null && AUDIT_ALERT_EVENT_TYPES.includes(eventType);
}

export const DEFAULT_ALERTING_SETTINGS: AlertingSettings = {
  enabled: true, inAppEnabled: true, desktopEnabled: false, paused: false, globalPauseUntilMs: null,
  quietHoursEnabled: false, quietHoursStart: "22:00", quietHoursEnd: "08:00", quietHoursTimezone: "local",
  criticalBypassesQuietHours: true, historyRetentionDays: 90, deliveryRetentionDays: 30,
  revision: 1, updatedAtMs: 0,
};

export function defaultAlertPolicy(eventType: AlertEventType = "collector_failed"): AlertPolicy {
  const option = ALERT_EVENT_OPTIONS.find((item) => item.value === eventType) ?? ALERT_EVENT_OPTIONS[0];
  const audit = isAuditAlertEvent(eventType);
  const immediate = audit || eventType === "key_invalid";
  return {
    id: `policy-${eventType}`, name: option.label, enabled: true, state: "active",
    scopeKind: "event_type", eventType, stationId: null, stationKeyId: null,
    minimumSeverity: null, severityOffset: 0,
    triggerMode: immediate ? "immediate" : "consecutive_occurrences",
    triggerCount: immediate ? null : eventType === "collector_failed" ? 3 : 2, triggerDurationSeconds: null,
    // Audit events do not enter the incident state machine; the count is a
    // schema-compatible placeholder and is not used for recovery.
    recoveryMode: "consecutive_healthy", recoveryCount: 1, recoveryDurationSeconds: null,
    inAppEnabled: true, desktopEnabled: false, repeatMode: "never",
    repeatIntervalSeconds: null, cooldownSeconds: 1_800, recoveryNotificationEnabled: true,
    quietHoursPolicy: "inherit", priority: 100, revision: 1, createdAtMs: 0, updatedAtMs: 0,
  };
}
