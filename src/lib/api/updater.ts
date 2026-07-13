import { getVersion } from "@tauri-apps/api/app";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import type { ProxyStatus } from "@/lib/types/proxy";
import {
  coordinateUpdateCheck,
  type PublishedUpdateInspection,
} from "@/lib/api/updaterCheckCoordinator";

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

type UpdaterNetworkConfig = {
  proxyUrl: string | null;
};

let pendingUpdate: Update | null = null;
let nativeUpdateCheckInFlight: Promise<Update | null> | null = null;

export async function currentAppVersion() {
  return isTauri() ? getVersion() : "0.0.0";
}

export async function checkForAppUpdate(): Promise<AppUpdateCheckResult> {
  const currentVersion = await currentAppVersion();
  if (!isTauri()) return { kind: "unsupported", currentVersion };

  await closePendingUpdateBeforeCheck();
  const network = await invoke<UpdaterNetworkConfig>("updater_network_config")
    .catch(() => ({ proxyUrl: null }));
  const result = await coordinateUpdateCheck({
    currentVersion,
    proxyUrl: network.proxyUrl,
    checkNative: async (proxyUrl) => {
      try {
        return await withTimeout(
          startNativeUpdateCheck(proxyUrl),
          12_000,
          "更新检查超时",
        );
      } catch (error) {
        abandonNativeUpdateCheck();
        throw error;
      }
    },
    inspectPublished: (version) =>
      invoke<PublishedUpdateInspection>("inspect_latest_update_manifest", {
        currentVersion: version,
      }),
  });
  if (result.kind === "current") return result;

  pendingUpdate = result.update;

  return {
    kind: "available",
    update: {
      currentVersion: result.update.currentVersion,
      version: result.update.version,
      notes: result.update.body ?? null,
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

async function closePendingUpdateBeforeCheck() {
  try {
    await withTimeout(closePendingUpdate(), 3_000, "清理旧更新检查超时");
  } catch {
    // A stale resource should not block a fresh check; pendingUpdate is already cleared.
  }
}

function startNativeUpdateCheck(proxyUrl: string | null) {
  if (!nativeUpdateCheckInFlight) {
    let trackedUpdateCheck: Promise<Update | null>;
    trackedUpdateCheck = check(
      proxyUrl ? { timeout: 10_000, proxy: proxyUrl } : { timeout: 10_000 },
    ).finally(() => {
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
