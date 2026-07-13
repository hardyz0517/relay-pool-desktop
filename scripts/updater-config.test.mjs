import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const read = (path) => readFile(path, "utf8").catch(() => "");

const packageJson = JSON.parse(await read("package.json"));
const tauriConfig = JSON.parse(await read("src-tauri/tauri.conf.json"));
const cargoToml = await read("src-tauri/Cargo.toml");
const tauriLib = await read("src-tauri/src/lib.rs");
const capabilitySource = await read("src-tauri/capabilities/default.json");
const workflow = await read(".github/workflows/release.yml");
const capability = capabilitySource ? JSON.parse(capabilitySource) : { permissions: [] };

assert.ok(packageJson.dependencies?.["@tauri-apps/plugin-updater"], "updater JS plugin is required");
assert.ok(packageJson.dependencies?.["@tauri-apps/plugin-process"], "process JS plugin is required");
assert.match(cargoToml, /tauri-plugin-updater\s*=/, "updater Rust plugin is required");
assert.match(cargoToml, /tauri-plugin-process\s*=/, "process Rust plugin is required");
assert.match(tauriLib, /tauri_plugin_updater/, "updater Rust plugin must be registered");
assert.match(tauriLib, /tauri_plugin_process/, "process Rust plugin must be registered");

assert.equal(tauriConfig.version, "../package.json");
assert.equal(tauriConfig.bundle?.active, true);
assert.equal(tauriConfig.bundle?.targets, "nsis");
assert.equal(tauriConfig.bundle?.createUpdaterArtifacts, true);
assert.equal(tauriConfig.bundle?.windows?.nsis?.installMode, "currentUser");
assert.equal(tauriConfig.plugins?.updater?.windows?.installMode, "passive");
assert.deepEqual(tauriConfig.plugins?.updater?.endpoints, [
  "https://github.com/hardyz0517/relay-pool-desktop/releases/latest/download/latest.json",
]);
assert.ok(
  typeof tauriConfig.plugins?.updater?.pubkey === "string" &&
    tauriConfig.plugins.updater.pubkey.length > 40,
  "updater public key must be configured",
);
assert.ok(capability.permissions.includes("updater:default"));
assert.ok(capability.permissions.includes("process:allow-restart"));

assert.match(workflow, /tags:\s*\["v\*"\]/, "release workflow must run on version tags");
assert.match(workflow, /windows-latest/, "release workflow must build on Windows");
assert.match(workflow, /releaseDraft:\s*true/, "release must start as a Draft");
assert.match(workflow, /TAURI_SIGNING_PRIVATE_KEY/, "release workflow must use updater signing key");
assert.match(workflow, /--target x86_64-pc-windows-msvc/, "release must target Windows x86_64");
assert.match(
  workflow,
  /node scripts\/updater-current-version-fallback\.test\.mjs/,
  "release workflow must guard updater manifest fallback behavior",
);
assert.match(
  workflow,
  /node --test scripts\/updater-check-coordinator\.test\.ts/,
  "release workflow must run behavioral updater coordinator tests",
);
assert.match(
  workflow,
  /cargo test[^\r\n]*services::updater/,
  "release workflow must run focused Rust updater service tests",
);
assert.match(
  workflow,
  /node scripts\/dashboard-update-action\.test\.mjs/,
  "release workflow must guard the dashboard update prompt action",
);

console.log("updater configuration contract checks passed");
