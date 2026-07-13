import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

const repoRoot = process.cwd();
const capabilitiesDir = path.join(repoRoot, "src-tauri", "capabilities");
const permissionsDir = path.join(repoRoot, "src-tauri", "permissions");
const capabilityFiles = fs
  .readdirSync(capabilitiesDir)
  .filter((name) => name.endsWith(".json"))
  .map((name) => ({
    name,
    json: JSON.parse(fs.readFileSync(path.join(capabilitiesDir, name), "utf8")),
  }));

function readTomlArray(text, key) {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = text.match(new RegExp(`${escapedKey}\\s*=\\s*\\[([\\s\\S]*?)\\]`));
  assert.ok(match, `expected TOML array ${key}`);
  return [...match[1].matchAll(/"([^"]+)"/g)].map((entry) => entry[1]);
}

function extractInvokeHandlerCommands(text) {
  const match = text.match(/generate_handler!\s*\\?\[\s*([\s\S]*?)\s*\]\)/);
  assert.ok(match, "expected lib.rs to register Tauri invoke handler commands");
  return [...match[1].matchAll(/commands::([A-Za-z0-9_]+)/g)].map((entry) => entry[1]);
}

const defaultCapability = capabilityFiles.find(({ json }) => json.identifier === "default");
assert.ok(defaultCapability, "main window must keep a default IPC capability");
assert.deepEqual(defaultCapability.json.windows, ["main"]);
assert.ok(
  defaultCapability.json.permissions.includes("main-window"),
  "main window needs an app command permission when the Tauri app manifest is enabled",
);

const mainPermissionPath = path.join(permissionsDir, "main-window.toml");
assert.ok(fs.existsSync(mainPermissionPath), "main window app commands must be declared");

const mainPermissionText = fs.readFileSync(mainPermissionPath, "utf8");
assert.match(mainPermissionText, /\[\[permission\]\]/);
assert.match(mainPermissionText, /identifier\s*=\s*"main-window"/);
const mainAllowedCommands = readTomlArray(mainPermissionText, "commands.allow");
assert.ok(
  mainAllowedCommands.includes("list_stations"),
  "main window must be allowed to invoke list_stations",
);
assert.ok(
  !mainAllowedCommands.includes("record_capture_event"),
  "capture event recording should stay restricted to capture windows",
);

const libText = fs.readFileSync(path.join(repoRoot, "src-tauri", "src", "lib.rs"), "utf8");
const mainWindowCommands = extractInvokeHandlerCommands(libText).filter(
  (command) => command !== "record_capture_event",
);
for (const command of mainWindowCommands) {
  assert.ok(
    mainAllowedCommands.includes(command),
    `main-window permission must allow registered command ${command}`,
  );
}

const captureCapability = capabilityFiles.find(({ json }) => {
  const windows = Array.isArray(json.windows) ? json.windows : [];
  return windows.includes("capture-*");
});

assert.ok(captureCapability, "manual authorization capture windows must have an IPC capability");
assert.deepEqual(
  captureCapability.json.permissions,
  ["record-capture-event"],
  "capture windows should only be able to invoke the capture authorization command group",
);
assert.deepEqual(
  captureCapability.json.remote?.urls,
  ["http://*", "https://*"],
  "capture windows load station management pages from remote HTTP(S) origins",
);
assert.equal(captureCapability.json.local, false, "capture capability should be remote-only");

const permissionPath = path.join(permissionsDir, "record-capture-event.toml");
assert.ok(
  fs.existsSync(permissionPath),
  "record_capture_event must be declared as an app permission for capture windows",
);

const permissionText = fs.readFileSync(permissionPath, "utf8");
assert.match(permissionText, /\[\[permission\]\]/);
assert.match(permissionText, /identifier\s*=\s*"record-capture-event"/);
const captureAllowedCommands = readTomlArray(permissionText, "commands.allow");
assert.deepEqual(
  captureAllowedCommands,
  ["record_capture_event", "finish_web_authorization_session"],
  "capture windows should only be able to record sanitized events and finish verified web authorization",
);

const buildScript = fs.readFileSync(path.join(repoRoot, "src-tauri", "build.rs"), "utf8");
assert.match(
  buildScript,
  /AppManifest::new\(\)/,
  "build.rs must register the app permission manifest so custom command permissions are available",
);
