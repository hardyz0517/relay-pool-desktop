import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const libSource = await readFile("src-tauri/src/lib.rs", "utf8");
const appRuntimeEventsSource = await readFile("src-tauri/src/app_runtime_events.rs", "utf8");
const runtimeCommandSource = await readFile("src-tauri/src/commands/runtime.rs", "utf8");
const registrySource = await readFile("src-tauri/src/ipc/registry.rs", "utf8");
const desktopBackendSource = await readFile("src/lib/bridge/DesktopBackend.ts", "utf8");
const generatedBridgeSource = await readFile("src/lib/bridge/generated.ts", "utf8");

assert.ok(libSource.includes("pub(crate) fn request_application_restart"));
assert.ok(appRuntimeEventsSource.includes('"app.restart.requested"'));
assert.ok(runtimeCommandSource.includes("pub async fn restart_application"));
assert.ok(runtimeCommandSource.includes("crate::request_application_restart(&app)"));
assert.ok(registrySource.includes(
  "restart_application => $crate::commands::runtime::restart_application",
));
assert.ok(registrySource.includes('"restart_application" => migrated_mutation'));
assert.ok(desktopBackendSource.includes(
  "restartApplication as restartApplicationBinding",
));
assert.ok(desktopBackendSource.includes(
  "restartApp: () => restartApplicationBinding()",
));
assert.ok(desktopBackendSource.includes("await restartApplicationBinding()"));
assert.ok(!desktopBackendSource.includes("@tauri-apps/plugin-process"));
assert.ok(!desktopBackendSource.includes("relaunch()"));
assert.ok(generatedBridgeSource.includes("export function restartApplication("));
assert.ok(generatedBridgeSource.includes('"restart_application"'));

console.log("runtime restart lifecycle contract passed");
