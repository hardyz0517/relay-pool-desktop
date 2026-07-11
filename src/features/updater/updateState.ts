export type UpdaterPhase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "cleaning"
  | "installing"
  | "failed";

export type UpdaterState = {
  phase: UpdaterPhase;
  currentVersion: string;
  version: string | null;
  notes: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  error: string | null;
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
  | { type: "FAILED"; message: string };

export const initialUpdaterState: UpdaterState = {
  phase: "idle",
  currentVersion: "0.0.0",
  version: null,
  notes: null,
  downloadedBytes: 0,
  totalBytes: null,
  error: null,
  lastCheckedAt: null,
};

export function reduceUpdaterState(state: UpdaterState, event: UpdaterEvent): UpdaterState {
  switch (event.type) {
    case "CURRENT_VERSION":
      return { ...state, currentVersion: event.version };
    case "CHECK_STARTED":
      return { ...state, phase: "checking", error: null };
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
      return { ...state, phase: "idle", version: null, notes: null, error: null };
    case "FAILED":
      return { ...state, phase: "failed", error: event.message };
  }
}
