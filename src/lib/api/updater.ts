import { getVersion } from "@tauri-apps/api/app";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import type { ProxyStatus } from "@/lib/types/proxy";

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

let pendingUpdate: Update | null = null;

export async function currentAppVersion() {
  return isTauri() ? getVersion() : "0.0.0";
}

export async function checkForAppUpdate(): Promise<AppUpdateCheckResult> {
  const currentVersion = await currentAppVersion();
  if (!isTauri()) return { kind: "unsupported", currentVersion };

  await closePendingUpdate();
  pendingUpdate = await check({ timeout: 10_000 });
  if (!pendingUpdate) return { kind: "current", currentVersion };

  return {
    kind: "available",
    update: {
      currentVersion: pendingUpdate.currentVersion,
      version: pendingUpdate.version,
      notes: pendingUpdate.body ?? null,
    },
  };
}

export async function downloadPendingUpdate(
  onProgress: (progress: DownloadProgress) => void,
) {
  if (!pendingUpdate) throw new Error("没有可下载的应用更新");
  let downloadedBytes = 0;
  let totalBytes: number | null = null;
  await pendingUpdate.download((event: DownloadEvent) => {
    if (event.event === "Started") {
      totalBytes = event.data.contentLength ?? null;
      onProgress({ downloadedBytes, totalBytes });
    } else if (event.event === "Progress") {
      downloadedBytes += event.data.chunkLength;
      onProgress({ downloadedBytes, totalBytes });
    } else {
      onProgress({ downloadedBytes, totalBytes });
    }
  });
}

export function cleanupBeforeUpdate() {
  return invoke<ProxyStatus>("cleanup_before_update");
}

export async function installPendingUpdateAndRelaunch() {
  if (!pendingUpdate) throw new Error("没有已下载的应用更新");
  await pendingUpdate.install();
  await relaunch();
}

export async function closePendingUpdate() {
  const update = pendingUpdate;
  pendingUpdate = null;
  await update?.close();
}
