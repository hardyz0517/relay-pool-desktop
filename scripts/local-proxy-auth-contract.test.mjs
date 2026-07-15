import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const proxyModule = await readFile("src-tauri/src/services/proxy/mod.rs", "utf8");
const runtime = await readFile("src-tauri/src/services/proxy/runtime.rs", "utf8");
const localAuth = await readFile("src-tauri/src/services/proxy/local_auth.rs", "utf8").catch(() => "");
const database = await readFile("src-tauri/src/services/database.rs", "utf8");

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

console.log("local proxy authentication contract passed");
