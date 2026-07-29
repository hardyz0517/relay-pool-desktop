import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

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

const migrationSql = fs
  .readdirSync(migrationsDir)
  .filter((name) => name.endsWith(".sql"))
  .sort()
  .map((name) => fs.readFileSync(path.join(migrationsDir, name), "utf8"))
  .join("\n")
  .replace(/--.*$/gm, "");
const catalogSource = fs.readFileSync(catalogPath, "utf8");

const actualTables = extractMigrationTables(migrationSql);
const declaredTables = extractCatalogTables(catalogSource);

assert.equal(
  actualTables.size,
  30,
  "portable migration v1 expects the current 30 user tables, including app_secret_bindings",
);
assert.deepEqual(
  [...declaredTables.keys()].sort(),
  [...actualTables.keys()].sort(),
  "MigrationDataCatalog must declare every migrated SQLite user table exactly once",
);

for (const [table, columns] of actualTables) {
  assert.deepEqual(
    declaredTables.get(table) ?? [],
    columns,
    `MigrationDataCatalog column list drifted for table ${table}`,
  );
}

assert.match(
  catalogSource,
  /carries configuration only, never runtime fields/,
  "catalog must keep existing channel monitor table classification limited to configuration data",
);

console.log(`portable migration catalog covers ${actualTables.size} tables`);

function extractMigrationTables(sql) {
  const tables = new Map();
  const createRe = /\bCREATE\s+(?:TEMP\s+)?TABLE\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/gi;
  let match;
  while ((match = createRe.exec(sql))) {
    const table = match[1];
    if (table === "persistence_v7_schema_guard") continue;
    const openParen = sql.indexOf("(", match.index);
    const closeParen = matchingParen(sql, openParen);
    const body = sql.slice(openParen + 1, closeParen);
    tables.set(table, extractCreateColumns(body));
    createRe.lastIndex = closeParen + 1;
  }

  const alterRe = /\bALTER\s+TABLE\s+([A-Za-z_][A-Za-z0-9_]*)\s+ADD\s+COLUMN\s+([A-Za-z_][A-Za-z0-9_]*)\b/gi;
  while ((match = alterRe.exec(sql))) {
    const [, table, column] = match;
    const columns = tables.get(table);
    assert.ok(columns, `ALTER TABLE referenced unknown table ${table}`);
    columns.push(column);
  }

  return new Map([...tables.entries()].sort(([a], [b]) => a.localeCompare(b)));
}

function extractCreateColumns(body) {
  return splitTopLevel(body)
    .map((part) => part.trim())
    .filter(Boolean)
    .filter((part) => !/^(PRIMARY|FOREIGN|UNIQUE|CHECK|CONSTRAINT)\b/i.test(part))
    .map((part) => {
      const match = part.match(/^["`[]?([A-Za-z_][A-Za-z0-9_]*)/);
      assert.ok(match, `could not parse CREATE TABLE column from ${part}`);
      return match[1];
    });
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

function matchingParen(text, openIndex) {
  let depth = 0;
  let quote = null;
  for (let index = openIndex; index < text.length; index += 1) {
    const char = text[index];
    if (quote) {
      if (char === quote && text[index - 1] !== "\\") quote = null;
      continue;
    }
    if (char === "'" || char === '"' || char === "`") {
      quote = char;
    } else if (char === "(") {
      depth += 1;
    } else if (char === ")") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  throw new Error("unclosed CREATE TABLE parenthesis");
}

function splitTopLevel(text) {
  const parts = [];
  let start = 0;
  let depth = 0;
  let quote = null;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    if (quote) {
      if (char === quote && text[index - 1] !== "\\") quote = null;
      continue;
    }
    if (char === "'" || char === '"' || char === "`") {
      quote = char;
    } else if (char === "(") {
      depth += 1;
    } else if (char === ")") {
      depth -= 1;
    } else if (char === "," && depth === 0) {
      parts.push(text.slice(start, index));
      start = index + 1;
    }
  }
  parts.push(text.slice(start));
  return parts;
}
