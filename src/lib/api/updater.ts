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

type PublishedUpdateManifest = {
  version: string;
  notes: string | null;
};

let pendingUpdate: Update | null = null;
let nativeUpdateCheckInFlight: Promise<Update | null> | null = null;
const UPDATE_MANIFEST_URL = "https://github.com/hardyz0517/relay-pool-desktop/releases/latest/download/latest.json";

export async function currentAppVersion() {
  return isTauri() ? getVersion() : "0.0.0";
}

export async function checkForAppUpdate(): Promise<AppUpdateCheckResult> {
  const currentVersion = await currentAppVersion();
  if (!isTauri()) return { kind: "unsupported", currentVersion };

  await closePendingUpdateBeforeCheck();
  try {
    pendingUpdate = await withTimeout(
      startNativeUpdateCheck(),
      12_000,
      "更新检查超时",
    );
  } catch (updateError) {
    abandonNativeUpdateCheck();
    const latestManifest = await withTimeout(
      fetchLatestManifestVersion(),
      6_000,
      "更新清单检查超时",
    ).catch(() => null);
    if (latestManifest && versionsMatch(latestManifest.version, currentVersion)) {
      return { kind: "current", currentVersion };
    }
    const manifestUpdate = latestManifest && isVersionNewer(latestManifest.version, currentVersion)
      ? {
          currentVersion,
          version: latestManifest.version,
          notes: latestManifest.notes,
        }
      : null;
    if (manifestUpdate) return { kind: "available", update: manifestUpdate };
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
  await ensurePendingUpdateForInstall();
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

async function closePendingUpdateBeforeCheck() {
  try {
    await withTimeout(closePendingUpdate(), 3_000, "清理旧更新检查超时");
  } catch {
    // A stale resource should not block a fresh check; pendingUpdate is already cleared.
  }
}

function startNativeUpdateCheck() {
  if (!nativeUpdateCheckInFlight) {
    let trackedUpdateCheck: Promise<Update | null>;
    trackedUpdateCheck = check({ timeout: 10_000 }).finally(() => {
      if (nativeUpdateCheckInFlight === trackedUpdateCheck) {
        nativeUpdateCheckInFlight = null;
      }
    });
    nativeUpdateCheckInFlight = trackedUpdateCheck;
  }
  return nativeUpdateCheckInFlight;
}

function abandonNativeUpdateCheck() {
  const abandonedUpdateCheck = nativeUpdateCheckInFlight;
  nativeUpdateCheckInFlight = null;
  if (!abandonedUpdateCheck) return;
  void abandonedUpdateCheck.then(
    (update) => update?.close(),
    () => undefined,
  );
}

async function ensurePendingUpdateForInstall() {
  if (pendingUpdate) return;
  const update = await withTimeout(
    startNativeUpdateCheck(),
    20_000,
    "准备更新下载超时",
  );
  if (update) {
    pendingUpdate = update;
  }
}

async function fetchLatestManifestVersion() {
  const desktopVersion = await invoke<string | null>("latest_update_manifest_version").catch(() => null);
  if (desktopVersion) return { version: desktopVersion, notes: null };
  return fetchLatestManifestVersionFromBrowser();
}

async function fetchLatestManifestVersionFromBrowser() {
  try {
    const response = await fetch(UPDATE_MANIFEST_URL, { cache: "no-store" });
    if (!response.ok) return null;
    const manifest = (await response.json()) as { version?: unknown; notes?: unknown };
    return typeof manifest.version === "string"
      ? {
          version: manifest.version,
          notes: typeof manifest.notes === "string" ? manifest.notes : null,
        }
      : null;
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

function isVersionNewer(candidate: string, current: string) {
  const candidateParts = versionParts(candidate);
  const currentParts = versionParts(current);
  const length = Math.max(candidateParts.length, currentParts.length);
  for (let index = 0; index < length; index += 1) {
    const candidatePart = candidateParts[index] ?? 0;
    const currentPart = currentParts[index] ?? 0;
    if (candidatePart > currentPart) return true;
    if (candidatePart < currentPart) return false;
  }
  return false;
}

function versionParts(version: string) {
  return normalizeVersion(version)
    .split(/[.-]/)
    .map((part) => Number.parseInt(part, 10))
    .filter((part) => Number.isFinite(part));
}

function normalizeVersion(version: string) {
  return version.trim().replace(/^v/i, "");
}
