export type CollectorTaskType = "detect" | "balance" | "groups" | "models" | "full";

export type CollectorRunStatus =
  | "running"
  | "success"
  | "partial"
  | "failed"
  | "manual_required"
  | string;

export type CollectorRun = {
  id: string;
  stationId: string;
  parentRunId: string | null;
  adapter: string;
  taskType: CollectorTaskType | string;
  status: CollectorRunStatus;
  startedAt: string;
  finishedAt: string | null;
  durationMs: number | null;
  endpointCount: number;
  successCount: number;
  failureCount: number;
  manualActionRequired: boolean;
  errorCode: string | null;
  errorMessage: string | null;
  snapshotId: string | null;
  createdAt: string;
};
