import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { DownloadProgress } from "@/lib/types/updater";

export type {
  AppUpdateCheckResult,
  AvailableAppUpdate,
  DownloadProgress,
} from "@/lib/types/updater";

export function currentAppVersion() {
  return getActiveBackendClient().updater.currentAppVersion();
}

export function checkForAppUpdate() {
  return getActiveBackendClient().updater.checkForAppUpdate();
}

export function downloadPendingUpdate(onProgress: (progress: DownloadProgress) => void) {
  return getActiveBackendClient().updater.downloadPendingUpdate(onProgress);
}

export function installPendingUpdateAndRelaunch() {
  return getActiveBackendClient().updater.installPendingUpdateAndRelaunch();
}

export function closePendingUpdate() {
  return getActiveBackendClient().updater.closePendingUpdate();
}
