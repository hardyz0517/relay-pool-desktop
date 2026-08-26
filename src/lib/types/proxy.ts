export type ProxyLifecycle =
  | "stopped"
  | "starting"
  | "running"
  | "draining"
  | "stopping"
  | "failed";

export type ProxyStatus = {
  running: boolean;
  lifecycle: ProxyLifecycle;
  bindAddr: string;
  port: number;
  startedAt: string | null;
  lastError: string | null;
  activeRequests: number;
  requestCount: number;
};

export type RequestLog = {
  id: string;
  requestId: string | null;
  startedAt: string;
  finishedAt: string | null;
  durationMs: number | null;
  method: string;
  path: string;
  model: string | null;
  stream: boolean;
  status: "success" | "fallback" | "failed" | string;
  httpStatus: number | null;
  lifecycleStatus: string | null;
  stationKeyId: string | null;
  stationId: string | null;
  upstreamBaseUrl: string | null;
  fallbackCount: number;
  errorMessage: string | null;
  routePolicy: string | null;
  routeReason: string | null;
  rejectedCandidatesJson: string | null;
  bodyBytes: number | null;
  attemptCount: number | null;
  routeWaitMs: number | null;
  upstreamHeadersMs: number | null;
  failureSource: string | null;
  attemptsJson: string | null;
  completionSource: string | null;
  promptTokens: number | null;
  completionTokens: number | null;
  totalTokens: number | null;
  cacheCreationTokens: number | null;
  cacheReadTokens: number | null;
  reasoningEffort: string | null;
  firstTokenMs: number | null;
  billingMode: string | null;
  estimatedInputCost: number | null;
  estimatedOutputCost: number | null;
  estimatedTotalCost: number | null;
  costCurrency: string | null;
  pricingSource: string | null;
  costStatus: string | null;
  groupBindingId: string | null;
  normalizationStatus: string | null;
  balanceScope: string | null;
  economicContextJson: string | null;
  createdAt: string;
};
