type ForwardCompatibleString = string & {};

export type ChannelMonitorTargetType = "station_key" | "station";

export type ChannelMonitorRunStatus =
  | "success"
  | "warning"
  | "failed"
  | "skipped"
  | ForwardCompatibleString;

export type ChannelMonitorRequestTemplate = {
  id: string;
  name: string;
  endpointKind: string;
  method: string;
  path: string;
  requestBodyJson: string;
  enabled: boolean;
  builtIn: boolean;
  note: string | null;
  createdAt: string;
  updatedAt: string;
};

export type CreateChannelMonitorTemplateInput = {
  name: string;
  endpointKind: string;
  method: string;
  path: string;
  requestBodyJson: string;
  enabled: boolean;
  note: string | null;
};

export type UpdateChannelMonitorTemplateInput = CreateChannelMonitorTemplateInput & {
  id: string;
};

export type ChannelMonitor = {
  id: string;
  name: string;
  targetType: ChannelMonitorTargetType;
  stationId: string;
  stationKeyId: string | null;
  templateId: string;
  enabled: boolean;
  intervalSeconds: number;
  jitterSeconds: number;
  timeoutSeconds: number;
  maxConcurrency: number;
  consecutiveFailureThreshold: number;
  fallbackModels: string[];
  note: string | null;
  createdAt: string;
  updatedAt: string;
};

export type CreateChannelMonitorInput = {
  name: string;
  targetType: ChannelMonitorTargetType;
  stationId: string;
  stationKeyId: string | null;
  templateId: string;
  enabled: boolean;
  intervalSeconds: number;
  jitterSeconds: number;
  timeoutSeconds: number;
  maxConcurrency: number;
  consecutiveFailureThreshold: number;
  fallbackModels: string[];
  note: string | null;
};

export type UpdateChannelMonitorInput = CreateChannelMonitorInput & {
  id: string;
};

export type ChannelMonitorRun = {
  id: string;
  monitorId: string;
  templateId: string;
  stationId: string;
  stationKeyId: string | null;
  status: ChannelMonitorRunStatus;
  startedAt: string;
  finishedAt: string | null;
  durationMs: number | null;
  httpStatus: number | null;
  latencyMs: number | null;
  responseModel: string | null;
  fallbackModel: string | null;
  errorMessage: string | null;
  createdAt: string;
};
