import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const updaterApiSource = await readFile("src/lib/api/updater.ts", "utf8");
const tauriCommandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const tauriLibSource = await readFile("src-tauri/src/lib.rs", "utf8");
const mainWindowPermissions = await readFile("src-tauri/permissions/main-window.toml", "utf8");

assert.ok(
  updaterApiSource.includes("coordinateUpdateCheck") &&
    updaterApiSource.includes('invoke<UpdaterNetworkConfig>("updater_network_config")') &&
    updaterApiSource.includes('invoke<PublishedUpdateInspection>("inspect_latest_update_manifest"') &&
    updaterApiSource.includes("withTimeout") &&
    updaterApiSource.includes("更新检查超时") &&
    updaterApiSource.includes("nativeUpdateCheckInFlight") &&
    updaterApiSource.includes("startNativeUpdateCheck") &&
    updaterApiSource.includes("check(") &&
    /proxyUrl \? \{ timeout: 10_000, proxy: proxyUrl \}/.test(updaterApiSource),
  "updater should share system proxy configuration with the authoritative native check",
);

assert.ok(
  !updaterApiSource.includes("fetchLatestManifestVersionFromBrowser") &&
    !updaterApiSource.includes("UPDATE_MANIFEST_URL") &&
    !updaterApiSource.includes("isVersionNewer") &&
    !updaterApiSource.includes("versionParts") &&
    !updaterApiSource.includes("ensurePendingUpdateForInstall"),
  "updater must not use a CORS browser fallback or expose manifest-only updates as installable",
);

assert.ok(
  tauriCommandsSource.includes("pub fn updater_network_config") &&
    tauriCommandsSource.includes("pub async fn inspect_latest_update_manifest") &&
    tauriLibSource.includes("commands::updater_network_config") &&
    tauriLibSource.includes("commands::inspect_latest_update_manifest"),
  "desktop backend should expose shared updater network and manifest inspection commands",
);

assert.ok(
  mainWindowPermissions.includes('"updater_network_config"') &&
    mainWindowPermissions.includes('"inspect_latest_update_manifest"') &&
    !mainWindowPermissions.includes('"latest_update_manifest_version"'),
  "the main window must be allowed to invoke the new updater commands instead of the removed fallback command",
);
