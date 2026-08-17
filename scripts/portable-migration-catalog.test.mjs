import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";

const repoRoot = process.cwd();
const migrationsDir = path.join(repoRoot, "src-tauri", "src", "persistence", "migrations");
const catalogPath = path.join(
  repoRoot,
  "src-tauri",
  "src",
  "services",
  "portable_migration",
  "catalog.rs",
);

const catalogSource = fs.readFileSync(catalogPath, "utf8");

const actualTables = extractMigrationTables(runMigrations());
const declaredTables = extractCatalogTables(catalogSource);

assert.equal(
  actualTables.size,
  66,
  "portable migration v1 expects the current 66 persisted user tables, including station-published status facts",
);
assert.deepEqual(
  [...declaredTables.keys()].sort(),
  [...actualTables.keys()].sort(),
  "MigrationDataCatalog must declare every migrated SQLite user table exactly once",
);

for (const [table, columns] of actualTables) {
  assert.deepEqual(
    [...(declaredTables.get(table) ?? [])].sort(),
    [...columns].sort(),
    `MigrationDataCatalog column list drifted for table ${table}`,
  );
}

assert.match(
  catalogSource,
  /carries configuration only, never runtime fields/,
  "catalog must keep existing channel monitor table classification limited to configuration data",
);

console.log(`portable migration catalog covers ${actualTables.size} tables`);

function runMigrations() {
  const db = new DatabaseSync(":memory:");
  db.exec("PRAGMA foreign_keys = ON;");
  for (const migration of fs.readdirSync(migrationsDir).filter((name) => name.endsWith(".sql")).sort()) {
    db.exec(fs.readFileSync(path.join(migrationsDir, migration), "utf8"));
  }
  return db;
}

function extractMigrationTables(db) {
  const tables = new Map();
  const ignoredDerivedTables = new Set([
    "dashboard_request_metric_rollups",
    "dashboard_request_cost_rollups",
    "dashboard_request_cost_totals_rollups",
    "station_endpoint_health",
    "station_key_health",
  ]);
  const tableNames = db
    .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
    .all()
    .map((row) => row.name)
    .filter((table) => !ignoredDerivedTables.has(table));
  for (const table of tableNames) {
    assert.match(table, /^[A-Za-z_][A-Za-z0-9_]*$/, `unsafe schema table name ${table}`);
    const columns = db
      .prepare(`PRAGMA table_info("${table}")`)
      .all()
      .map((column) => column.name);
    tables.set(table, columns);
  }
  return tables;
}

function extractCatalogTables(source) {
  const arrays = new Map();
  const arrayRe = /const\s+([A-Z0-9_]+_COLUMNS)\s*:\s*&\s*\[\s*&str\s*\]\s*=\s*&\s*\[([\s\S]*?)\];/g;
  let match;
  while ((match = arrayRe.exec(source))) {
    arrays.set(match[1], [...match[2].matchAll(/"([^"]+)"/g)].map((item) => item[1]));
  }

  const tables = new Map();
  const tableRe = /table\(\s*"([^"]+)"/g;
  while ((match = tableRe.exec(source))) {
    const table = match[1];
    const end = source.indexOf("),", match.index);
    assert.notEqual(end, -1, `could not find end of catalog entry for ${table}`);
    const segment = source.slice(match.index, end);
    const columnConst = segment.match(/\b([A-Z0-9_]+_COLUMNS)\b/)?.[1];
    assert.ok(columnConst, `could not find column constant for ${table}`);
    const columns = arrays.get(columnConst);
    assert.ok(columns, `missing column array ${columnConst}`);
    assert.ok(!tables.has(table), `duplicate catalog entry for ${table}`);
    tables.set(table, columns);
  }

  return new Map([...tables.entries()].sort(([a], [b]) => a.localeCompare(b)));
}
