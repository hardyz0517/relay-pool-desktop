import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = fileURLToPath(new URL("..", import.meta.url));
const crate = join(repo, "src-tauri");
const metadataDirectory = join(crate, ".sqlx");

const sources = [
  "src/persistence/schema_compatibility.rs",
  "src/persistence/migrations.rs",
  "src/persistence/health_check.rs",
].map((path) => readFileSync(join(crate, path), "utf8"));

for (const source of sources) {
  assert.match(source, /sqlx::query!\s*\(/, "critical persistence query must use SQLx compile-time checking");
}
assert.doesNotMatch(sources[0], /sqlx::query\s*\(/);
assert.doesNotMatch(sources[1], /sqlx::query\s*\(/);

const metadataFiles = readdirSync(metadataDirectory)
  .filter((name) => /^query-[0-9a-f]{64}\.json$/.test(name))
  .sort();
assert.ok(metadataFiles.length >= 3, "expected non-empty SQLx metadata for critical queries");

const metadata = metadataFiles.map((name) =>
  JSON.parse(readFileSync(join(metadataDirectory, name), "utf8")),
);
for (const entry of metadata) {
  assert.equal(entry.db_name, "SQLite");
  assert.equal(typeof entry.query, "string");
  assert.ok(entry.query.trim().length > 0);
  assert.equal(typeof entry.describe, "object");
}

const normalizedQueries = metadata.map((entry) => entry.query.replace(/\s+/g, " ").trim());
for (const required of [
  "FROM persistence_schema_compatibility WHERE singleton_key = 1",
  'SELECT version AS "version!: i64" FROM _sqlx_migrations WHERE success = 1 ORDER BY version DESC LIMIT 1',
  "UPDATE persistence_runtime_health SET write_probe_count = write_probe_count + 1",
]) {
  assert.ok(
    normalizedQueries.some((query) => query.includes(required)),
    `offline metadata missing critical query: ${required}`,
  );
}

const prepareScript = readFileSync(join(repo, "scripts", "prepare-sqlx.ps1"), "utf8");
assert.match(prepareScript, /param\(\[switch\]\$Check\)/);
assert.match(prepareScript, /sqlx-cli 0\\\.8\\\.6/);
assert.match(prepareScript, /"migrate", "run", "--source", "src\/persistence\/migrations"/);
assert.match(prepareScript, /@\("sqlx", "prepare"\)/);
assert.match(prepareScript, /"--check"/);
assert.match(prepareScript, /Invoke-Checked "cargo" \$prepareArguments/);
assert.match(prepareScript, /finally\s*\{/);
assert.match(prepareScript, /Remove-DatabaseArtifacts/);

console.log(`sqlx offline metadata contract passed (${metadataFiles.length} queries)`);
