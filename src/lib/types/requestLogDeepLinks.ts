export type RequestLogDeepLink = {
  kind: "request-log";
  requestLogId: string;
  source?: "routing_decision_trace";
};

export type VersionedRequestLogDeepLink = RequestLogDeepLink & {
  sequence: number;
};
