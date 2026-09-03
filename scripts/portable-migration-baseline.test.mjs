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
  const source = [
    readText("src-tauri/src/persistence/schema_registry.rs"),
    readText("src-tauri/src/persistence/migrations.rs"),
  ].join("\n");
  const match = source.match(/sqlx::migrate!\("(?<path>[^"]+)"\)/);
  assert(match?.groups?.path, "persistence migrator path must be declared with sqlx::migrate!");
  return path.join(repoRoot, "src-tauri", match.groups.path.replace(/^\.\//, ""));
}

function migrationFiles() {
  return fs
    .readdirSync(migrationDirectory())
    .filter((name) => /^\d{4}_.+\.sql$/.test(name))
    .sort();
}

function latestMigrationVersion(migrations) {
  assert(migrations.length > 0, "persistence migration directory must not be empty");
  return Number(migrations.at(-1).slice(0, 4));
}

function runMigrations() {
  const db = new DatabaseSync(":memory:");
  db.exec("PRAGMA foreign_keys = ON;");
  const migrations = migrationFiles();
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
  const source = readText("src-tauri/src/persistence/schema_registry.rs");
  const generation = Number(source.match(/database_generation:\s*(\d+)/)?.[1]);
  assert(
    /writable_schema:\s*BTreeSet::from\(\[latest\]\)/.test(source),
    "binary writable schema must be derived from registry latest schema",
  );
  return { generation, writableSchema: latestMigrationVersion(migrationFiles()) };
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
  // The proposal is a historical design snapshot. The executable catalog is
  // the current portable-migration contract and must be the gate's source of
  // truth as migrations evolve past the original schema-16 baseline.
  const catalog = readText("src-tauri/src/services/portable_migration/catalog.rs");
  const tableSection = catalog.slice(catalog.indexOf("const TABLES:"));
  const tables = [...tableSection.matchAll(/\btable\(\s*"([^"]+)"/g)].map((match) => match[1]);
  assert(tables.length > 0, "portable migration catalog must declare table entries");
  return tables;
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
const migrations = migrationFiles();
const latestSchema = latestMigrationVersion(migrations);
const compatibility = db
  .prepare(
    "SELECT database_generation AS generation, schema_version AS schemaVersion FROM persistence_schema_compatibility WHERE singleton_key = 1",
  )
  .get();
const binary = currentBinaryCompatibility();
const actualTables = currentSchemaTables(db);
const catalogTables = specCatalogTables();
// Materialized dashboard rollups are local derived state and are rebuilt on
// the target device instead of being part of the portable data catalog.
const allowedDerivedOnly = new Set([
  "dashboard_request_cost_rollups",
  "dashboard_request_cost_totals_rollups",
  "dashboard_request_metric_rollups",
]);
const actualSet = new Set(actualTables);
const catalogSet = new Set(catalogTables);
const missingFromCatalog = actualTables.filter(
  (table) => !catalogSet.has(table) && !allowedDerivedOnly.has(table),
);
const unexpectedCatalogOnly = catalogTables.filter((table) => !actualSet.has(table));

assert(compatibility.generation === 2, `database generation must be 2, got ${compatibility.generation}`);
assert(
  compatibility.schemaVersion === latestSchema,
  `raw migrations must leave schema compatibility at the latest migration schema ${latestSchema}, got ${compatibility.schemaVersion}`,
);
assert(binary.generation === 2, `binary database generation must be 2, got ${binary.generation}`);
assert(
  binary.writableSchema === latestSchema,
  `binary writable schema must match the latest migration schema ${latestSchema}, got ${binary.writableSchema}`,
);
assert(
  actualTables.length === catalogTables.length + allowedDerivedOnly.size,
  `current schema table count must match the portable catalog plus known derived-only tables (${catalogTables.length} + ${allowedDerivedOnly.size}), got ${actualTables.length}: ${actualTables.join(", ")}`,
);
assert(missingFromCatalog.length === 0, `portable catalog is missing current tables: ${missingFromCatalog.join(", ")}`);
assert(
  unexpectedCatalogOnly.length === 0,
  `portable catalog contains unexpected future/non-schema tables: ${unexpectedCatalogOnly.join(", ")}`,
);
assert(
  catalogTables.filter((table) => table === "app_secret_bindings").length === 1,
  "spec catalog matrix must contain exactly one app_secret_bindings entry",
);
assertSecurityPolicyStillBlocksPortableSecretMigration();

console.log(
  `portable migration baseline gate passed: generation=${compatibility.generation}, schema=${compatibility.schemaVersion}, schemaTables=${actualTables.length}`,
);
