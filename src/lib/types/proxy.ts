export type ProxyStatus = {
  running: boolean;
  bindAddr: string;
  port: number;
  startedAt: string | null;
  lastError: string | null;
  activeRequests: number;
  requestCount: number;
};

export type RequestLog = {
  id: string;
  startedAt: string;
  finishedAt: string | null;
  durationMs: number | null;
  method: string;
  path: string;
  model: string | null;
  stream: boolean;
  status: "success" | "fallback" | "failed" | string;
  stationKeyId: string | null;
  stationId: string | null;
  upstreamBaseUrl: string | null;
  fallbackCount: number;
  errorMessage: string | null;
  routePolicy: string | null;
  routeReason: string | null;
  rejectedCandidatesJson: string | null;
  createdAt: string;
};
