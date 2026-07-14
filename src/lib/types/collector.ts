export type CollectorSnapshot = {
  id: string;
  stationId: string;
  endpointRevision: number;
  source: string;
  status: string;
  fetchedAt: string;
  summaryJson: Record<string, unknown>;
  normalizedJson: Record<string, unknown>;
  rawJsonRedacted: Record<string, unknown> | null;
  errorMessage: string | null;
  createdAt: string;
};

export type CollectorTaskType = "detect" | "balance" | "groups" | "models" | "full";

export type CollectorEndpointResult = {
  path: string;
  result: string;
  detail: string;
  statusCode?: number | null;
};

export type CollectorRecognizedSummary = {
  balanceLabel: unknown;
  groupCount: number;
  rateCount: number;
  keyCount: number;
  matchedFieldCount: number;
};

export type CollectorSummary = {
  adapter?: string;
  detectedType?: string;
  conclusion?: string;
  message?: string;
  loginStatus?: string;
  loginRequired?: boolean;
  nextStep?: string;
  diagnosis?: string;
  endpointResults?: CollectorEndpointResult[];
  recognized?: CollectorRecognizedSummary;
  webviewRequired?: boolean;
  webviewNote?: string;
};

export type CollectorEvent = {
  eventType: string;
  message: string;
  status: string;
};

export type CollectorRunResult = {
  snapshot: CollectorSnapshot;
  events: CollectorEvent[];
};

export type StationLoginTestInput = {
  stationType?: string;
  websiteUrl: string;
  loginUsername: string;
  loginPassword: string;
};

export type StationLoginTestResult = {
  status: "success" | "manual_required" | "missing_base_url" | "missing_credentials" | string;
  message: string;
  diagnosis: string | null;
  tokenPresent: boolean;
};

export type CaptureSessionStatus = {
  stationId: string;
  status: "idle" | "capturing" | "failed" | string;
  captureCount: number;
  recognizedFieldCount: number;
  pendingConfirmationCount: number;
  webAuthorizationCandidate: boolean;
  lastError: string | null;
};
