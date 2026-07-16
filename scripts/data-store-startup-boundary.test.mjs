import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const lib = readFileSync("src-tauri/src/lib.rs", "utf8");
const commands = readFileSync("src-tauri/src/commands/mod.rs", "utf8");
const permission = readFileSync("src-tauri/permissions/main-window.toml", "utf8");

assert.match(lib, /inspect_startup/);
assert.match(lib, /DataStoreStartupState/);
assert.doesNotMatch(lib, /AppDatabase::initialize\(app\.handle\(\)\)\?/);
assert.match(commands, /get_data_store_startup_state/);
assert.match(commands, /activate_data_store_candidate/);
assert.match(permission, /get_data_store_startup_state/);
assert.match(permission, /activate_data_store_candidate/);

console.log("data-store startup boundary contract passed");
