import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const proxyModule = await readFile("src-tauri/src/services/proxy/mod.rs", "utf8");
const runtime = await readFile("src-tauri/src/services/proxy/runtime.rs", "utf8");
const legacyRuntime = await readFile("src-tauri/src/services/proxy/legacy_runtime.rs", "utf8").catch(() => "");
const localAuth = await readFile("src-tauri/src/services/proxy/local_auth.rs", "utf8").catch(() => "");
const ingress = await readFile("src-tauri/src/services/proxy/ingress.rs", "utf8");
const proxyStartup = await readFile("src-tauri/src/services/proxy/startup.rs", "utf8");
const settingsService = await readFile("src-tauri/src/application/settings.rs", "utf8");
const commands = await readFile("src-tauri/src/commands/mod.rs", "utf8");

function functionBlock(source, signature) {
  const start = source.indexOf(signature);
  assert.notEqual(start, -1, `${signature} should exist`);
  const braceStart = source.indexOf("{", start);
  assert.notEqual(braceStart, -1, `${signature} should have a body`);
  let depth = 0;
  for (let index = braceStart; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index + 1);
  }
  throw new Error(`${signature} body did not close`);
}

const getLocalAccessKeyCommand = functionBlock(commands, "pub async fn get_local_access_key");
const importCcswitchCommand = functionBlock(commands, "pub async fn import_relay_pool_to_ccswitch");
const prepareCcswitchImport = functionBlock(commands, "fn prepare_ccswitch_import");

assert.match(proxyModule, /mod local_auth;/);
assert.doesNotMatch(proxyModule, /mod legacy_runtime;/);
assert.equal(legacyRuntime, "", "legacy proxy runtime should remain deleted");
assert.match(localAuth, /pub fn authorize_headers/);
assert.match(localAuth, /pub fn allowed_origin/);
assert.match(localAuth, /ConstantTimeEq/);
assert.match(ingress, /local_auth::authorize_headers\(&headers, &state\.local_access_key\)/);
assert.match(ingress, /ProxyFailureCode::LocalAuthInvalid/);
assert.match(ingress, /local_auth::allowed_origin\(origin\)/);
assert.doesNotMatch(ingress, /access-control-allow-origin:\s*\*/i);
assert.match(runtime, /V2ProxyExecutor/);
assert.match(runtime, /RequestLifecycleStore/);
assert.doesNotMatch(runtime, /RequestFinalizationService/);
assert.match(runtime, /LifecycleWriter::start/);
assert.doesNotMatch(runtime, /ProxyRuntimeMode/);
assert.doesNotMatch(runtime, /fn forward_(chat|responses|embeddings)_request/);
assert.match(settingsService, /pub\(crate\) async fn ensure_local_access_key/);
assert.match(settingsService, /OsRng\.fill_bytes/);
assert.match(settingsService, /ensure_local_access_key_replaces_placeholder_once_under_concurrency/);
assert.match(proxyStartup, /services[\s\S]*\.settings[\s\S]*\.ensure_local_access_key\(\)/);
assert.match(getLocalAccessKeyCommand, /services[\s\S]*\.settings[\s\S]*\.ensure_local_access_key\(\)/);
assert.match(importCcswitchCommand, /services[\s\S]*\.settings[\s\S]*\.ensure_local_access_key\(\)/);
assert.match(importCcswitchCommand, /prepare_ccswitch_import\(&local_access_key, &proxy_status\)/);
assert.match(prepareCcswitchImport, /local_access_key/);
assert.doesNotMatch(importCcswitchCommand, /load_local_access_key\(\)/);

console.log("local proxy authentication contract passed");
