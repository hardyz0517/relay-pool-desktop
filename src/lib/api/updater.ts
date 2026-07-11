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
const UPDATE_MANIFEST_URL = "https://github.com/hardyz0517/relay-pool-desktop/releases/latest/download/latest.json";

export async function currentAppVersion() {
  return isTauri() ? getVersion() : "0.0.0";
}

export async function checkForAppUpdate(): Promise<AppUpdateCheckResult> {
  const currentVersion = await currentAppVersion();
  if (!isTauri()) return { kind: "unsupported", currentVersion };

  await closePendingUpdate();
  try {
    pendingUpdate = await withTimeout(
      check({ timeout: 10_000 }),
      12_000,
      "更新检查超时",
    );
  } catch (updateError) {
    const latestVersion = await withTimeout(
      fetchLatestManifestVersion(),
      6_000,
      "更新清单检查超时",
    ).catch(() => null);
    if (latestVersion && versionsMatch(latestVersion, currentVersion)) {
      return { kind: "current", currentVersion };
    }
    throw updateError;
  }
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

async function fetchLatestManifestVersion() {
  const desktopVersion = await invoke<string | null>("latest_update_manifest_version").catch(() => null);
  if (desktopVersion) return desktopVersion;
  return fetchLatestManifestVersionFromBrowser();
}

async function fetchLatestManifestVersionFromBrowser() {
  try {
    const response = await fetch(UPDATE_MANIFEST_URL, { cache: "no-store" });
    if (!response.ok) return null;
    const manifest = (await response.json()) as { version?: unknown };
    return typeof manifest.version === "string" ? manifest.version : null;
  } catch {
    return null;
  }
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string) {
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error(message)), timeoutMs);
    promise.then(
      (value) => {
        window.clearTimeout(timer);
        resolve(value);
      },
      (error) => {
        window.clearTimeout(timer);
        reject(error);
      },
    );
  });
}

function versionsMatch(left: string, right: string) {
  return normalizeVersion(left) === normalizeVersion(right);
}

function normalizeVersion(version: string) {
  return version.trim().replace(/^v/i, "");
}
