import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

const repoRoot = process.cwd();

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

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

const scannedFiles = [
  ...listFiles(path.join(repoRoot, "src", "features", "settings", "data-migration"), (file) => /\.(ts|tsx)$/.test(file)),
  ...listFiles(path.join(repoRoot, "src", "lib", "api"), (file) => file.endsWith("dataMigration.ts") || file.endsWith("dataMigration.test.ts")),
  ...listFiles(path.join(repoRoot, "src", "lib", "types"), (file) => file.endsWith("dataMigration.ts")),
  ...listFiles(path.join(repoRoot, "src-tauri", "src", "commands"), (file) => file.endsWith("data_migration.rs")),
  ...listFiles(path.join(repoRoot, "src-tauri", "src", "application", "data_migration"), (file) => file.endsWith(".rs")),
  ...listFiles(path.join(repoRoot, "src-tauri", "src", "services", "portable_migration"), (file) => file.endsWith(".rs")),
];

const forbiddenSecretPatterns = [
  /sk-[A-Za-z0-9_-]{12,}/,
  /Bearer\s+[A-Za-z0-9._-]{12,}/,
  /refresh_token\s*[:=]\s*["'][A-Za-z0-9._-]{16,}["']/i,
  /access_token\s*[:=]\s*["'][A-Za-z0-9._-]{16,}["']/i,
  /cookie\s*[:=]\s*["'][A-Za-z0-9._-]{16,}["']/i,
];

for (const absolutePath of scannedFiles) {
  const relativePath = path.relative(repoRoot, absolutePath).replaceAll("\\", "/");
  const source = fs.readFileSync(absolutePath, "utf8");
  const nonCanaryLines = source
    .split(/\r?\n/)
    .filter((line) => !/RPD_TEST_|canary|p8-test-secret/i.test(line))
    .join("\n");
  for (const pattern of forbiddenSecretPatterns) {
    assert.ok(!pattern.test(nonCanaryLines), `${relativePath} contains a literal secret-shaped value: ${pattern}`);
  }
}

const frontend = scannedFiles
  .filter((file) => file.includes(`${path.sep}src${path.sep}`))
  .map((file) => fs.readFileSync(file, "utf8"))
  .join("\n");
assert.ok(!/localStorage\s*\./.test(frontend), "portable migration UI must not persist passphrases or operation state in localStorage");
assert.ok(!/analytics|telemetry|screenshot/i.test(frontend), "portable migration UI must not send passphrase-adjacent flow data to analytics/screenshots");
assert.ok(!/error\.detail|details\?\.|rawError|stack/i.test(frontend), "portable migration UI must not render backend raw error details");

const generated = read("src/lib/bridge/generated.ts");
const domainTypes = read("src/lib/types/dataMigration.ts");
assert.ok(!domainTypes.includes("generated"), "portable migration domain types must not import generated bridge types");
assert.ok(generated.includes("getPortableImportRecoveryState"), "generated bridge wrapper must expose recovery state without frontend hand-written invoke calls");

const commandSource = read("src-tauri/src/commands/data_migration.rs");
assert.match(commandSource, /passphrase|PortableMigrationCommandFacade/, "command facade must own public migration errors and DTO parsing");
assert.ok(!commandSource.includes("<redacted>") || !commandSource.includes("passphrase_confirmation"), "command layer must not debug-print passphrase DTOs");

const exportService = read("src-tauri/src/application/data_migration/export_service.rs");
assert.match(exportService, /field\("passphrase", &"<redacted>"\)/, "portable export request Debug must redact passphrase");
assert.match(exportService, /field\("passphrase_confirmation", &"<redacted>"\)/, "portable export request Debug must redact passphrase confirmation");

const journal = read("src-tauri/src/services/portable_migration/activation_journal.rs");
assert.match(journal, /!raw\.contains\("password"\)/, "activation journal tests must prove password redaction");
assert.match(journal, /!raw\.contains\("transport"\)/, "activation journal tests must prove transport key id is not journaled");

console.log(`portable migration redaction gate scanned ${scannedFiles.length} files`);
