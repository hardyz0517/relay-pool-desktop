import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

const repoRoot = process.cwd();

function listFiles(root, predicate = () => true) {
  const entries = [];
  if (!fs.existsSync(root)) return entries;
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const absolute = path.join(root, entry.name);
    if (entry.isDirectory()) entries.push(...listFiles(absolute, predicate));
    else if (entry.isFile() && predicate(absolute)) entries.push(absolute);
  }
  return entries.sort();
}

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function stripRustTests(source) {
  const marker = "#[cfg(test)]";
  const index = source.indexOf(marker);
  return index >= 0 ? source.slice(0, index) : source;
}

const frontendFiles = [
  ...listFiles(path.join(repoRoot, "src", "features", "settings", "data-migration"), (file) => /\.(ts|tsx)$/.test(file)),
  ...listFiles(path.join(repoRoot, "src", "lib", "api"), (file) => file.endsWith("dataMigration.ts") || file.endsWith("dataMigration.test.ts")),
  ...listFiles(path.join(repoRoot, "src", "lib", "types"), (file) => file.endsWith("dataMigration.ts")),
  path.join(repoRoot, "src", "lib", "bridge", "BackendClient.ts"),
  path.join(repoRoot, "src", "lib", "bridge", "DemoBackend.ts"),
];
for (const absolutePath of frontendFiles) {
  const relativePath = path.relative(repoRoot, absolutePath).replaceAll("\\", "/");
  const source = fs.readFileSync(absolutePath, "utf8");
  if (relativePath === "src/lib/bridge/DesktopBackend.ts") continue;
  assert.ok(!/invoke\s*\(\s*["'](?:start|get|choose)_portable_/i.test(source), `${relativePath} must use BackendClient/dataMigration API instead of direct portable invoke`);
  assert.ok(!/\b(?:age|sqlx|sqlite3?|keyring)\b/i.test(source), `${relativePath} must not reach crypto, SQLite, or keyring primitives`);
  assert.ok(!/child_process|Command\.create|plugin-process|processCommand|shell/i.test(source), `${relativePath} must not spawn shell or external process for portable migration`);
}

const commandSource = read("src-tauri/src/commands/data_migration.rs");
assert.ok(!/services::portable_migration|sqlx|keyring|age::|std::process|Command::new/.test(commandSource), "portable migration IPC commands must only call the command facade");
assert.ok(commandSource.includes("State<'_, PortableMigrationCommandFacade>"), "portable migration IPC commands must inject only the command facade state");

const facadeSource = read("src-tauri/src/application/data_migration/mod.rs");
const securityPolicy = read("docs/SECURITY_EXPORT_IMPORT.md");
assert.ok(/SECURITY_POLICY_APPROVED:\s*bool\s*=\s*true/.test(facadeSource), "portable migration capability must be enabled only after explicit security approval");
assert.ok(securityPolicy.includes("Security approval: approved by the repository owner on 2026-07-30 for the codex/cross-device-encrypted-migration branch."), "portable migration approval record must be documented before enabling the feature gate");
assert.ok(securityPolicy.includes("Release promotion still requires the two-machine smoke checklist"), "portable migration release qualification must remain separate from approval");

const readerFiles = [
  "src-tauri/src/services/portable_migration/schema_reader.rs",
  "src-tauri/src/services/portable_migration/validate.rs",
  "src-tauri/src/services/portable_migration/staging.rs",
];
for (const relativePath of readerFiles) {
  const productionSource = stripRustTests(read(relativePath));
  assert.ok(!/\b(?:ATTACH|DETACH|CREATE\s+(?:TABLE|VIEW|TRIGGER|INDEX)|ALTER\s+TABLE|DROP\s+|INSERT\s+INTO|UPDATE\s+|DELETE\s+FROM)\b/i.test(productionSource), `${relativePath} portable reader/staging code must not execute migrations, DDL, ATTACH, or mutation SQL`);
  assert.ok(!/sqlx::migrate!|persistence::migrations|Migrator/i.test(productionSource), `${relativePath} portable reader/staging code must not run application migrations`);
}

const recoverySource = read("src-tauri/src/services/portable_migration/recovery.rs");
assert.ok(recoverySource.includes("validate_journal_paths"), "activation recovery must validate journal paths before replacement");
assert.ok(recoverySource.includes("recover_portable_activation_for_startup"), "activation recovery must be startup-owned");
assert.ok(recoverySource.includes("ManualRecoveryRequired"), "ambiguous activation state must fail closed into manual recovery");

const registrySource = read("src-tauri/src/services/portable_migration/inspection_registry.rs");
assert.ok(registrySource.includes("is_import_staging_sqlite"), "inspection cleanup must only delete owned staging sqlite files");
assert.ok(!/remove_dir_all/.test(registrySource), "inspection registry must not recursively delete unverified directories");

console.log("portable migration boundary gate passed");
