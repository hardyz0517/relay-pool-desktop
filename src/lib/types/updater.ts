export type AvailableAppUpdate = {
  currentVersion: string;
  version: string;
  notes: string | null;
};

export type AppUpdateCheckResult =
  | { kind: "unsupported"; currentVersion: string }
  | { kind: "current"; currentVersion: string }
  | { kind: "available"; update: AvailableAppUpdate };

export type DownloadProgress = {
  downloadedBytes: number;
  totalBytes: number | null;
};
