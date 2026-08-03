export type PricingGroupMonitorStatusInput = {
  schemaVersion: 1;
  groupRefsHash: string;
  groups: Array<{
    stationId: string;
    groupBindingId: string | null;
    groupIdHash: string | null;
    groupKeyHash: string;
  }>;
};

export type PricingGroupMonitorDisplayState =
  | "unresolved"
  | "no_key"
  | "unmonitored"
  | "running"
  | "untested"
  | "available"
  | "degraded"
  | "unavailable"
  | "skipped"
  | "unavailable_data";

export type PricingGroupMonitorSummary = {
  stationId: string;
  groupBindingId: string | null;
  groupIdHash: string | null;
  groupKeyHash: string;
  matchKind:
    | "exact_binding"
    | "parent_binding"
    | "group_id_hash"
    | "group_key_hash"
    | "unresolved";
  resolutionState: "resolved" | "unresolved";
  hasBoundKey: boolean;
  boundKeyCount: number;
  enabledKeyCount: number;
  credentialedKeyCount: number;
  enabledMonitorDefinitionCount: number;
  monitoredKeyCount: number;
  testedKeyCount: number;
  representativeKeyId: string | null;
  representativeMonitorId: string | null;
  latestTargetResultId: string | null;
  latestOutcome: "available" | "degraded" | "unavailable" | "skipped" | "missing";
  latestFailureKind: string | null;
  latestTerminalReason: string | null;
  running: boolean;
  checkedAtMs: number | null;
  latencyMs: number | null;
  generatedAtMs: number;
  displayState: PricingGroupMonitorDisplayState;
};

export type PricingGroupMonitorStatusWorkspace = {
  schemaVersion: 1;
  generatedAtMs: number;
  groupRefsHash: string;
  requestedGroupCount: number;
  returnedGroupCount: number;
  omittedGroupCount: number;
  items: PricingGroupMonitorSummary[];
};
