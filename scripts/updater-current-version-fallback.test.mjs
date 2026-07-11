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
    updaterApiSource.includes("return { kind: \"current\", currentVersion }") &&
    /catch \(updateError\)[\s\S]*fetchLatestManifestVersion[\s\S]*throw updateError/.test(updaterApiSource),
  "updater should fall back to latest.json and report current when the published version matches the installed version",
);

assert.ok(
  tauriCommandsSource.includes("pub fn latest_update_manifest_version") &&
    tauriCommandsSource.includes("UPDATE_MANIFEST_URL") &&
    tauriCommandsSource.includes('get("version")') &&
    tauriLibSource.includes("commands::latest_update_manifest_version"),
  "desktop backend should expose a latest_update_manifest_version command for the updater fallback",
);
