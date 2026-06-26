export type CollectorSnapshot = {
  id: string;
  stationId: string;
  source: string;
  status: string;
  fetchedAt: string;
  summaryJson: Record<string, unknown>;
  normalizedJson: Record<string, unknown>;
  rawJsonRedacted: Record<string, unknown> | null;
  errorMessage: string | null;
  createdAt: string;
};

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
