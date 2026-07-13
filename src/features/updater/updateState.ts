export type UpdaterPhase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "cleaning"
  | "installing"
  | "failed";

export type UpdaterFailureOperation = "check" | "download" | "prepare" | "install";

export type UpdaterState = {
  phase: UpdaterPhase;
  currentVersion: string;
  version: string | null;
  notes: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  error: string | null;
  failedOperation: UpdaterFailureOperation | null;
  lastCheckedAt: string | null;
};

export type UpdaterEvent =
  | { type: "CURRENT_VERSION"; version: string }
  | { type: "CHECK_STARTED" }
  | { type: "UP_TO_DATE"; currentVersion: string; checkedAt: string }
  | {
      type: "UPDATE_AVAILABLE";
      currentVersion: string;
      version: string;
      notes: string | null;
    }
  | { type: "DOWNLOAD_STARTED" }
  | { type: "DOWNLOAD_PROGRESS"; downloadedBytes: number; totalBytes: number | null }
  | { type: "CLEANUP_STARTED" }
  | { type: "INSTALL_STARTED" }
  | { type: "DISMISSED" }
  | { type: "FAILED"; operation: UpdaterFailureOperation; message: string };

export const initialUpdaterState: UpdaterState = {
  phase: "idle",
  currentVersion: "0.0.0",
  version: null,
  notes: null,
  downloadedBytes: 0,
  totalBytes: null,
  error: null,
  failedOperation: null,
  lastCheckedAt: null,
};

export function isUpdaterBusyPhase(phase: UpdaterPhase) {
  return phase === "checking" ||
    phase === "downloading" ||
    phase === "cleaning" ||
    phase === "installing";
}

export function reduceUpdaterState(state: UpdaterState, event: UpdaterEvent): UpdaterState {
  switch (event.type) {
    case "CURRENT_VERSION":
      return { ...state, currentVersion: event.version };
    case "CHECK_STARTED":
      return {
        ...state,
        phase: "checking",
        version: null,
        notes: null,
        downloadedBytes: 0,
        totalBytes: null,
        error: null,
        failedOperation: null,
      };
    case "UP_TO_DATE":
      return {
        ...state,
        phase: "idle",
        currentVersion: event.currentVersion,
        version: null,
        notes: null,
        downloadedBytes: 0,
        totalBytes: null,
        lastCheckedAt: event.checkedAt,
        error: null,
        failedOperation: null,
      };
    case "UPDATE_AVAILABLE":
      return {
        ...state,
        phase: "available",
        currentVersion: event.currentVersion,
        version: event.version,
        notes: event.notes,
        downloadedBytes: 0,
        totalBytes: null,
        error: null,
        failedOperation: null,
        lastCheckedAt: new Date().toISOString(),
      };
    case "DOWNLOAD_STARTED":
      return { ...state, phase: "downloading", downloadedBytes: 0, totalBytes: null, error: null };
    case "DOWNLOAD_PROGRESS":
      return { ...state, downloadedBytes: event.downloadedBytes, totalBytes: event.totalBytes };
    case "CLEANUP_STARTED":
      return { ...state, phase: "cleaning" };
    case "INSTALL_STARTED":
      return { ...state, phase: "installing" };
    case "DISMISSED":
      return {
        ...state,
        phase: "idle",
        version: null,
        notes: null,
        error: null,
        failedOperation: null,
      };
    case "FAILED":
      return {
        ...state,
        phase: "failed",
        version: event.operation === "check" ? null : state.version,
        notes: event.operation === "check" ? null : state.notes,
        downloadedBytes: event.operation === "check" ? 0 : state.downloadedBytes,
        totalBytes: event.operation === "check" ? null : state.totalBytes,
        error: event.message,
        failedOperation: event.operation,
      };
  }
}
