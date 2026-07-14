export type ChangeSeverity = "critical" | "warning" | "info";
export type ChangeEventStatus = "unread" | "read" | "dismissed" | "resolved";

export type ChangeObjectType =
  | "station"
  | "station_key"
  | "pricing_rule"
  | "routing_rule"
  | "request_log"
  | "channel"
  | "collector";

export type ChangeEvent = {
  id: string;
  severity: ChangeSeverity;
  eventType: string;
  status: ChangeEventStatus;
  title: string;
  message: string;
  objectType: ChangeObjectType | string;
  objectId: string | null;
  stationId: string | null;
  stationName?: string | null;
  stationKeyId: string | null;
  pricingRuleId: string | null;
  requestLogId: string | null;
  oldValueJson: string | null;
  newValueJson: string | null;
  impactJson: string | null;
  dedupeKey: string;
  source: string;
  detectedAt: string;
  resolvedAt: string | null;
  createdAt: string;
  updatedAt: string;
};

export type UpsertChangeEventInput = {
  severity: ChangeSeverity;
  eventType: string;
  title: string;
  message: string;
  objectType: ChangeObjectType | string;
  objectId: string | null;
  stationId: string | null;
  stationKeyId: string | null;
  pricingRuleId: string | null;
  requestLogId: string | null;
  oldValueJson: string | null;
  newValueJson: string | null;
  impactJson: string | null;
  dedupeKey: string;
  source: string;
};
