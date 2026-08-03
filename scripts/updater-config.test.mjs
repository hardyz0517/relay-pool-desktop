import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { parse as parseYaml } from "yaml";

const read = (path) => readFile(path, "utf8").catch(() => "");

const packageJson = JSON.parse(await read("package.json"));
const tauriConfig = JSON.parse(await read("src-tauri/tauri.conf.json"));
const cargoToml = await read("src-tauri/Cargo.toml");
const tauriLib = await read("src-tauri/src/lib.rs");
const capabilitySource = await read("src-tauri/capabilities/default.json");
const workflow = parseYaml(await read(".github/workflows/release.yml"));
const verifier = await read("scripts/verify.ps1");
const contractRunner = await read("scripts/run-contract-tests.mjs");
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

const releaseJob = workflow.jobs.release;
const releaseSteps = releaseJob.steps;
const tauriAction = releaseSteps.find((step) => String(step.uses ?? "").startsWith("tauri-apps/tauri-action@"));
assert.deepEqual(workflow.on.push.tags, ["v*"] , "release workflow must run on version tags");
assert.equal(releaseJob["runs-on"], "windows-latest", "release workflow must build on Windows");
assert.equal(tauriAction.with.releaseDraft, true, "release must start as a Draft");
assert.ok(tauriAction.env.TAURI_SIGNING_PRIVATE_KEY, "release workflow must use updater signing key");
assert.equal(tauriAction.with.args, "--target x86_64-pc-windows-msvc", "release must target Windows x86_64");
assert.ok(releaseSteps.some((step) => String(step.run ?? "").includes("pnpm verify:release:prebundle")), "release workflow must use the shared prebundle verification gate");
assert.ok(releaseSteps.some((step) => String(step.run ?? "").includes("pnpm verify:release:postbundle")), "release workflow must use the shared postbundle verification gate");
assert.match(
  contractRunner,
  /"scripts\/updater-current-version-fallback\.test\.mjs"/,
  "shared release verification must guard updater manifest fallback behavior",
);
assert.match(
  contractRunner,
  /"--experimental-strip-types", "--test", "scripts\/updater-check-coordinator\.test\.ts"/,
  "shared release verification must run behavioral updater coordinator tests",
);
assert.match(
  verifier,
  /Invoke-Checked "Rust tests" cargo @\("test", "--locked"/,
  "shared release verification must run the locked Rust suite containing updater tests",
);
assert.match(
  contractRunner,
  /"scripts\/dashboard-update-action\.test\.mjs"/,
  "shared release verification must guard the dashboard update prompt action",
);

console.log("updater configuration contract checks passed");
