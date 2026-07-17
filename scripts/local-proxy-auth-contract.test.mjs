import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const proxyModule = await readFile("src-tauri/src/services/proxy/mod.rs", "utf8");
const runtime = await readFile("src-tauri/src/services/proxy/runtime.rs", "utf8");
const localAuth = await readFile("src-tauri/src/services/proxy/local_auth.rs", "utf8").catch(() => "");
const database = await readFile("src-tauri/src/services/database.rs", "utf8");
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

const getLocalAccessKeyCommand = functionBlock(commands, "pub fn get_local_access_key");
const importCcswitchCommand = functionBlock(commands, "pub fn import_relay_pool_to_ccswitch");
const prepareCcswitchImport = functionBlock(commands, "fn prepare_ccswitch_import");

assert.match(proxyModule, /mod local_auth;/);
assert.match(localAuth, /pub fn authorize_headers/);
assert.match(localAuth, /pub fn allowed_origin/);
assert.match(runtime, /database\.ensure_secure_local_access_key\(\)/);
assert.match(runtime, /local_auth::authorize_headers\(&request\.headers, &local_key\)/);
assert.match(runtime, /invalid_local_api_key/);
assert.match(runtime, /local_auth::allowed_origin/);
assert.doesNotMatch(runtime, /access-control-allow-origin:\s*\*/i);
assert.match(database, /ensure_secure_local_access_key/);
assert.match(database, /OsRng\.fill_bytes/);
assert.match(getLocalAccessKeyCommand, /database\.ensure_secure_local_access_key\(\)/);
assert.match(prepareCcswitchImport, /database\.ensure_secure_local_access_key\(\)/);
assert.match(importCcswitchCommand, /prepare_ccswitch_import\(&database, &proxy_status\)/);
assert.doesNotMatch(importCcswitchCommand, /database\.get_local_access_key\(\)/);

console.log("local proxy authentication contract passed");
