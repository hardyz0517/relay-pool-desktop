import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const updaterApiSource = await readFile("src/lib/api/updater.ts", "utf8");
const desktopBackendSource = await readFile("src/lib/bridge/DesktopBackend.ts", "utf8");
const tauriUpdaterCommandsSource = await readFile("src-tauri/src/commands/updater.rs", "utf8");
const tauriRegistrySource = await readFile("src-tauri/src/ipc/registry.rs", "utf8");
const mainWindowPermissions = await readFile("src-tauri/permissions/main-window.toml", "utf8");

assert.ok(
  updaterApiSource.includes("getActiveBackendClient().updater.checkForAppUpdate()") &&
    !updaterApiSource.includes("coordinateUpdateCheck") &&
    !updaterApiSource.includes("updaterNetworkConfig") &&
    !updaterApiSource.includes("inspectLatestUpdateManifest") &&
    desktopBackendSource.includes("coordinateUpdateCheck") &&
    desktopBackendSource.includes("updaterNetworkConfigBinding()") &&
    desktopBackendSource.includes("inspectLatestUpdateManifestBinding({ currentVersion: version })") &&
    desktopBackendSource.includes("withTimeout") &&
    desktopBackendSource.includes("更新检查超时") &&
    desktopBackendSource.includes("nativeUpdateCheckInFlight") &&
    desktopBackendSource.includes("startNativeUpdateCheck") &&
    desktopBackendSource.includes("check(") &&
    /proxyUrl \? \{ timeout: 10_000, proxy: proxyUrl \}/.test(desktopBackendSource),
  "updater should share system proxy configuration with the authoritative native check",
);

assert.ok(
  !updaterApiSource.includes("fetchLatestManifestVersionFromBrowser") &&
    !updaterApiSource.includes("UPDATE_MANIFEST_URL") &&
    !updaterApiSource.includes("isVersionNewer") &&
    !updaterApiSource.includes("versionParts") &&
    !updaterApiSource.includes("ensurePendingUpdateForInstall") &&
    !desktopBackendSource.includes("fetchLatestManifestVersionFromBrowser") &&
    !desktopBackendSource.includes("UPDATE_MANIFEST_URL") &&
    !desktopBackendSource.includes("isVersionNewer") &&
    !desktopBackendSource.includes("versionParts") &&
    !desktopBackendSource.includes("ensurePendingUpdateForInstall"),
  "updater must not use a CORS browser fallback or expose manifest-only updates as installable",
);

assert.ok(
  tauriUpdaterCommandsSource.includes("pub async fn updater_network_config") &&
    tauriUpdaterCommandsSource.includes("EmptyInputDto::parse(input)?") &&
    tauriUpdaterCommandsSource.includes("pub async fn inspect_latest_update_manifest") &&
    tauriUpdaterCommandsSource.includes("PublishedUpdateInspectionInputDto::parse(input)?") &&
    tauriRegistrySource.includes("updater_network_config => $crate::commands::updater::updater_network_config") &&
    tauriRegistrySource.includes(
      "inspect_latest_update_manifest => $crate::commands::updater::inspect_latest_update_manifest",
    ),
  "desktop backend should expose shared updater network and manifest inspection commands",
);

assert.ok(
  mainWindowPermissions.includes('"updater_network_config"') &&
    mainWindowPermissions.includes('"inspect_latest_update_manifest"') &&
    !mainWindowPermissions.includes('"latest_update_manifest_version"'),
  "the main window must be allowed to invoke the new updater commands instead of the removed fallback command",
);
