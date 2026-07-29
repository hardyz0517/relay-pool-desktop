export type RuntimeTaskStatus =
  | "registered"
  | "running"
  | "backing_off"
  | "stopping"
  | "succeeded"
  | "cancelled"
  | "failed"
  | "panicked";

export type RuntimeTaskSummary = {
  id: string;
  kind: string;
  runId: number | null;
  status: RuntimeTaskStatus;
  lastStartedAtMs: number | null;
  lastSucceededAtMs: number | null;
  lastFailureCode: string | null;
  consecutiveFailures: number;
  nextRetryAtMs: number | null;
};

export type RuntimeStatus = {
  tasks: RuntimeTaskSummary[];
};
