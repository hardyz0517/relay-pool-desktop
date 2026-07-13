import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const updaterApiSource = await readFile("src/lib/api/updater.ts", "utf8");
const tauriCommandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const tauriLibSource = await readFile("src-tauri/src/lib.rs", "utf8");

assert.ok(
  updaterApiSource.includes("fetchLatestManifestVersion") &&
    updaterApiSource.includes("fetchLatestManifestVersionFromBrowser") &&
    updaterApiSource.includes("UPDATE_MANIFEST_URL") &&
    updaterApiSource.includes('invoke<string | null>("latest_update_manifest_version")') &&
    updaterApiSource.includes("withTimeout") &&
    updaterApiSource.includes("更新检查超时") &&
    updaterApiSource.includes("versionsMatch") &&
    updaterApiSource.includes("isVersionNewer") &&
    updaterApiSource.includes("nativeUpdateCheckInFlight") &&
    updaterApiSource.includes("startNativeUpdateCheck") &&
    updaterApiSource.includes("ensurePendingUpdateForInstall") &&
    updaterApiSource.includes('return { kind: "available", update: manifestUpdate }') &&
    updaterApiSource.includes("return { kind: \"current\", currentVersion }") &&
    /catch \(updateError\)[\s\S]*fetchLatestManifestVersion[\s\S]*throw updateError/.test(updaterApiSource),
  "updater should fall back to latest.json and report current or available when the published version can be compared",
);

assert.ok(
  /export async function downloadPendingUpdate[\s\S]*ensurePendingUpdateForInstall\(\)[\s\S]*没有可下载的应用更新/.test(updaterApiSource),
  "download should prepare a native updater resource before failing when manifest fallback found an update",
);

assert.ok(
  tauriCommandsSource.includes("pub fn latest_update_manifest_version") &&
    tauriCommandsSource.includes("UPDATE_MANIFEST_URL") &&
    tauriCommandsSource.includes('get("version")') &&
    tauriLibSource.includes("commands::latest_update_manifest_version"),
  "desktop backend should expose a latest_update_manifest_version command for the updater fallback",
);
