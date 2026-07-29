import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const lib = readFileSync("src-tauri/src/lib.rs", "utf8");
const secrets = readFileSync("src-tauri/src/services/secrets/mod.rs", "utf8");
const keychain = readFileSync("src-tauri/src/services/secrets/keychain.rs", "utf8");

const setupStart = lib.indexOf(".setup(|app|");
assert.notEqual(setupStart, -1, "lib.rs must define the Tauri setup boundary");
const setup = lib.slice(setupStart);

function indexOfRequired(source, pattern, label) {
  const index = source.indexOf(pattern);
  assert.notEqual(index, -1, `${label} not found`);
  return index;
}

const resolveConfig = indexOfRequired(setup, "app_config_dir()", "config dir resolution");
const acquireLease = indexOfRequired(setup, "InstallationLease::try_acquire", "installation lease");
const inspectFacts = indexOfRequired(setup, "inspect_startup(&default_data_dir)", "data-store facts inspection");
const contextKey = indexOfRequired(
  setup,
  "initialize_secret_material_for_startup",
  "context-aware device key load/create",
);
const dataStore = indexOfRequired(setup, "prepare_data_store(", "data-store preparation");

assert(
  resolveConfig < acquireLease,
  "setup must resolve config dir before acquiring the installation lease",
);
assert(
  acquireLease < inspectFacts,
  "setup must acquire the installation lease before data-store inspection",
);
assert(
  inspectFacts < contextKey,
  "setup must inspect recovery facts before any device key load/create",
);
assert(
  contextKey < dataStore,
  "setup must complete context-aware key handling before opening writable data store",
);

assert.doesNotMatch(
  setup,
  /SecretManager::initialize|load_or_create_data_key/,
  "setup must not directly call the legacy load-or-create key path",
);
assert.match(secrets, /create_pending_for_first_run/);
assert.match(secrets, /load_existing/);
assert.match(keychain, /SystemCredentialBackend/);

console.log("device key startup boundary contract passed");
