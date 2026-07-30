import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function readText(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function migrationDirectory() {
  const source = readText("src-tauri/src/persistence/migrations.rs");
  const match = source.match(/sqlx::migrate!\("(?<path>[^"]+)"\)/);
  assert(match?.groups?.path, "persistence migrator path must be declared with sqlx::migrate!");
  return path.join(repoRoot, "src-tauri", match.groups.path.replace(/^\.\//, ""));
}

function runMigrations() {
  const db = new DatabaseSync(":memory:");
  db.exec("PRAGMA foreign_keys = ON;");
  const migrations = fs
    .readdirSync(migrationDirectory())
    .filter((name) => /^\d{4}_.+\.sql$/.test(name))
    .sort();
  assert(
    migrations.includes("0009_provider_drafts.sql"),
    "schema 9 provider drafts migration must be present before portable migration work",
  );
  assert(
    migrations.includes("0017_encrypted_secret_baseline.sql"),
    "schema 17 encrypted-secret baseline migration must be present after the current mainline schema",
  );
  for (const migration of migrations) {
    db.exec(fs.readFileSync(path.join(migrationDirectory(), migration), "utf8"));
  }
  return db;
}

function currentBinaryCompatibility() {
  const source = readText("src-tauri/src/persistence/migrations.rs");
  const generation = Number(source.match(/database_generation:\s*(\d+)/)?.[1]);
  const writable = [...source.matchAll(/writable_schema:\s*BTreeSet::from\(\[(\d+)\]\)/g)].map((match) =>
    Number(match[1]),
  );
  return { generation, writableSchema: writable.at(-1) };
}

function currentSchemaTables(db) {
  return db
    .prepare(
      "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .all()
    .map((row) => row.name);
}

function specCatalogTables() {
  const spec = readText("docs/proposals/CROSS_DEVICE_ENCRYPTED_MIGRATION_SPEC.md");
  const matrixStart = spec.indexOf("以下矩阵必须作为首版 `MigrationDataCatalog`");
  assert(matrixStart >= 0, "portable migration spec must contain the MigrationDataCatalog matrix");
  const matrixEnd = spec.indexOf("`settings` 必须采用 key allowlist", matrixStart);
  assert(matrixEnd > matrixStart, "portable migration spec matrix end marker is missing");
  const matrix = spec.slice(matrixStart, matrixEnd);
  return [...matrix.matchAll(/^\|\s*`([^`]+)`(?:（[^|]+）)?\s*\|/gm)].map((match) => match[1]);
}

function assertSecurityPolicyStillBlocksPortableSecretMigration() {
  const policy = readText("docs/SECURITY_EXPORT_IMPORT.md");
  assert(
    /Encrypted secret export is not part of P8\./.test(policy),
    "security policy must still say encrypted secret export is not approved by default",
  );
  assert(
    /Default exports do not include[\s\S]*encrypted ciphertext\./.test(policy),
    "default export policy must still exclude encrypted ciphertext",
  );
}

const db = runMigrations();
const compatibility = db
  .prepare(
    "SELECT database_generation AS generation, schema_version AS schemaVersion FROM persistence_schema_compatibility WHERE singleton_key = 1",
  )
  .get();
const binary = currentBinaryCompatibility();
const actualTables = currentSchemaTables(db);
const catalogTables = specCatalogTables();
const allowedSpecOnly = new Set();
const actualSet = new Set(actualTables);
const catalogSet = new Set(catalogTables);
const missingFromSpec = actualTables.filter((table) => !catalogSet.has(table));
const unexpectedSpecOnly = catalogTables.filter((table) => !actualSet.has(table) && !allowedSpecOnly.has(table));

assert(compatibility.generation === 2, `database generation must be 2, got ${compatibility.generation}`);
assert(
  compatibility.schemaVersion === 16,
  `raw migrations must leave schema compatibility at pre-baseline 16 until the encrypted-secret finalizer runs, got ${compatibility.schemaVersion}`,
);
assert(binary.generation === 2, `binary database generation must be 2, got ${binary.generation}`);
assert(binary.writableSchema === 17, `binary writable schema must be 17, got ${binary.writableSchema}`);
assert(actualTables.length === 37, `expected 37 current schema tables, got ${actualTables.length}: ${actualTables.join(", ")}`);
assert(missingFromSpec.length === 0, `spec catalog matrix is missing current tables: ${missingFromSpec.join(", ")}`);
assert(
  unexpectedSpecOnly.length === 0,
  `spec catalog matrix contains unexpected future/non-schema tables: ${unexpectedSpecOnly.join(", ")}`,
);
assert(
  catalogTables.filter((table) => table === "app_secret_bindings").length === 1,
  "spec catalog matrix must contain exactly one app_secret_bindings entry",
);
assertSecurityPolicyStillBlocksPortableSecretMigration();

console.log(
  `portable migration baseline gate passed: generation=${compatibility.generation}, schema=${compatibility.schemaVersion}, schemaTables=${actualTables.length}`,
);
