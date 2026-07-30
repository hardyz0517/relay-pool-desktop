import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const lib = readFileSync(join(root, "src-tauri/src/lib.rs"), "utf8");
const recovery = readFileSync(
  join(root, "src-tauri/src/services/portable_migration/recovery.rs"),
  "utf8",
);

function assert(condition, message) {
  if (!condition) {
    console.error(message);
    process.exit(1);
  }
}

const setupStart = lib.indexOf(".setup(|app|");
assert(setupStart >= 0, "startup setup body must exist");
const setup = lib.slice(setupStart);

const lease = setup.indexOf("InstallationLease::try_acquire");
const portableRecovery = setup.indexOf("recover_portable_activation_for_startup");
const inspectStartup = setup.indexOf("inspect_startup(&default_data_dir)?");
const prepareDataStore = setup.indexOf("prepare_data_store(");
const proxyRuntime = setup.indexOf("ProxyRuntimeState::default");

assert(lease >= 0, "startup must acquire installation lease");
assert(portableRecovery > lease, "portable recovery must run after installation lease");
assert(
  portableRecovery < inspectStartup,
  "portable recovery must run before generic startup inspection",
);
assert(
  portableRecovery < prepareDataStore,
  "portable recovery must run before persistence preparation",
);
assert(portableRecovery < proxyRuntime, "portable recovery must run before proxy runtime");

assert(
  recovery.includes("load_by_key_id") &&
    recovery.includes("target_device_key_id"),
  "portable recovery must load the exact journal target device key id",
);
assert(
  recovery.includes("ManualRecoveryRequired") &&
    recovery.includes("JournalMalformed"),
  "malformed activation journal must enter manual recovery instead of being ignored",
);
assert(
  recovery.includes("replace_with_rollback") &&
    recovery.includes("AtomicDatabaseReplacePort"),
  "activation must use the single atomic database replace port",
);
