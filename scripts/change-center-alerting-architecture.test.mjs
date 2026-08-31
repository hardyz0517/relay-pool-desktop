import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";

// This gate intentionally has two phases. During the migration phase it makes
// legacy usage visible and verifies that it is accounted for in the ledger. At
// cutover the same inventory becomes a zero-hit production graph check.
const args = process.argv.slice(2);
const rootIndex = args.indexOf("--root");
const root = path.resolve(rootIndex >= 0 ? args[rootIndex + 1] : process.cwd());
const verbose = args.includes("--verbose");
const manifestPath = "docs/audits/change-center-alerting-boundary-manifest.json";
const ledgerPath = "docs/audits/change-center-alerting-deletion-ledger.md";
const manifest = readJson(manifestPath);
const ledger = readSource(ledgerPath);

assert.equal(manifest.schema_version, 1, "alerting boundary manifest schema_version must be 1");
assert.ok(["open", "cutover", "observing", "closed"].includes(manifest.status), "alerting boundary manifest has an invalid status");
assert.ok(Array.isArray(manifest.new_domain), "new_domain must be an array");
assert.ok(Array.isArray(manifest.legacy_read_allowlist), "legacy_read_allowlist must be an array");
assert.ok(Array.isArray(manifest.legacy_write_allowlist), "legacy_write_allowlist must be an array");
assert.ok(Array.isArray(manifest.legacy_production_paths), "legacy_production_paths must be an array");
assert.ok(Array.isArray(manifest.historical_paths), "historical_paths must be an array");
assert.equal(new Set(manifest.legacy_production_paths.map(normalize)).size, manifest.legacy_production_paths.length, "legacy_production_paths must not contain duplicates");
for (const relative of [...manifest.legacy_production_paths, ...manifest.historical_paths]) {
  assert.ok(existsSync(resolve(relative)), `boundary manifest path ${relative} must exist until its deletion revision`);
}
assert.match(ledger, /变更中心旧栈删除账本|Change Center legacy deletion ledger/iu, "deletion ledger must be the alerting ledger");
assert.match(ledger, /Current production inventory/u, "deletion ledger must contain a current production inventory");

checkNewDomainBoundary();
checkLegacyReaderBoundary();
const hits = scanLegacyProductionHits();
checkLegacyInventory(hits);

if (["cutover", "observing", "closed"].includes(manifest.status)) {
  const productionHits = hits.filter((hit) => !isHistoricalPath(hit.path));
  assert.deepEqual(
    productionHits,
    [],
    `alerting cutover requires zero legacy production hits:\n${formatHits(productionHits)}`,
  );
}

console.log(`change-center alerting architecture gate passed (${manifest.status}; ${hits.length} legacy hits inventoried)`);

function checkNewDomainBoundary() {
  const required = [
    "src-tauri/src/models/alerting",
    "src-tauri/src/application/alerting",
    "src-tauri/src/persistence/stores/alerting",
  ];
  for (const relative of required) {
    const absolute = resolve(relative);
    assert.ok(existsSync(absolute), `${relative} must exist`);
    assert.ok(filesUnder(absolute).length > 0, `${relative} must contain source files`);
  }

  const forbiddenDomainDependencies = /\b(?:sqlx|reqwest|tauri|tokio|PersistenceHandle|WriteSession|commands?::|persistence::|services?::|models::change_events|ChangeService|ChangeStore)\b/u;
  const domainRelative = "src-tauri/src/models/alerting";
  for (const file of filesUnder(resolve(domainRelative)).filter((candidate) => candidate.endsWith(".rs"))) {
    const source = readFileSync(file, "utf8");
    assert.doesNotMatch(
      source,
      forbiddenDomainDependencies,
      `${domainRelative} domain source must not depend on outer layers (${relativePath(file)})`,
    );
  }

  for (const relative of ["src-tauri/src/application/alerting", "src-tauri/src/persistence/stores/alerting"]) {
    for (const file of filesUnder(resolve(relative)).filter((candidate) => candidate.endsWith(".rs"))) {
      const source = readFileSync(file, "utf8");
      assert.doesNotMatch(source, /\b(?:ChangeService|ChangeStore|models::change_events|change_events\s*::)/u, `${relative} must not bridge to the old change stack (${relativePath(file)})`);
    }
  }
}

function checkLegacyReaderBoundary() {
  for (const relative of manifest.legacy_read_allowlist) {
    const absolute = resolve(relative);
    assert.ok(existsSync(absolute), `legacy read adapter ${relative} must exist while allowlisted`);
    const source = readFileSync(absolute, "utf8");
    const productionSource = source.split("#[cfg(test)]", 1)[0];
    assert.match(productionSource, /FROM\s+change_events/u, `${relative} must be an explicit legacy reader`);
    assert.doesNotMatch(productionSource, /\b(?:INSERT\s+INTO|UPDATE|DELETE\s+FROM)\s+change_events/u, `${relative} must not write the legacy table`);
    assert.doesNotMatch(productionSource, /ChangeService|ChangeStore/u, `${relative} must not depend on deleted service/store owners`);
    assert.doesNotMatch(productionSource, /#\[tauri::command\]|commands?::|ipc::/u, `${relative} must not be reachable through product IPC`);
  }
  assert.deepEqual(manifest.legacy_write_allowlist, [], "new alerting code must never add a legacy write allowlist");
}

function scanLegacyProductionHits() {
  const rules = [
    ["legacy symbol", /\b(?:ChangeService|ChangeStore)\b/u],
    ["legacy writer helper", /\b(?:NewChangeEvent|upsert_change_event)\b/u],
    ["legacy command", /\b(?:upsert_change_event|resolve_change_event|mark_change_event_read|mark_change_events_read|dismiss_change_event|clear_change_events|list_change_events(?:_for_station)?)\b/u],
    ["legacy table", /change_events/u],
    ["legacy frontend API", /(?:changeEvents|change-events|changeEventViewModels|changeQueries)/u],
  ];
  const hits = [];
  for (const relativeRoot of manifest.production_scan_roots ?? ["src-tauri/src", "src"]) {
    const absoluteRoot = resolve(relativeRoot);
    if (!existsSync(absoluteRoot)) continue;
    for (const file of filesUnder(absoluteRoot).filter(isSourceFile)) {
      const relative = relativePath(file);
      const lines = productionSource(file).split(/\r?\n/u);
      lines.forEach((line, index) => {
        for (const [rule, pattern] of rules) {
          if (pattern.test(line)) hits.push({ path: relative, line: index + 1, rule, text: line.trim().slice(0, 180) });
        }
      });
    }
  }
  return dedupeHits(hits);
}

function productionSource(file) {
  const source = readFileSync(file, "utf8");
  if (!file.endsWith(".rs")) return source;

  const testModule = /\r?\n#\[cfg\(test\)\]\r?\nmod tests \{/u;
  const testModuleIndex = source.search(testModule);
  return testModuleIndex >= 0 ? source.slice(0, testModuleIndex) : source;
}

function checkLegacyInventory(hits) {
  const productionPaths = new Set(manifest.legacy_production_paths.map(normalize));
  const unlisted = hits.filter((hit) => !isHistoricalPath(hit.path) && !productionPaths.has(normalize(hit.path)));
  assert.deepEqual(
    unlisted,
    [],
    `legacy production hits must be listed in legacy_production_paths:\n${formatHits(unlisted)}`,
  );
  const allowedHistorical = hits.filter((hit) => isHistoricalPath(hit.path));
  for (const hit of allowedHistorical) {
    assert.ok(manifest.historical_paths.some((entry) => matchesPath(hit.path, entry)), `${hit.path} must be covered by historical_paths`);
  }
  if (manifest.status === "open") {
    console.warn(`legacy production inventory (${hits.length} hits):\n${verbose ? formatHits(hits) : formatPathSummary(hits)}`);
  }
}

function isHistoricalPath(relative) {
  return manifest.historical_paths.some((entry) => matchesPath(relative, entry));
}

function matchesPath(relative, entry) {
  const normalized = normalize(relative);
  const pattern = normalize(entry)
    .split("*")
    .map((part) => part.replace(/[.+^${}()|[\]\\]/gu, "\\$&"))
    .join(".*");
  return new RegExp(`^${pattern}$`, "u").test(normalized);
}

function dedupeHits(hits) {
  const seen = new Set();
  return hits.filter((hit) => {
    const key = `${hit.path}:${hit.line}:${hit.rule}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function formatHits(hits) {
  return hits.length === 0 ? "(none)" : hits.map((hit) => `- ${hit.path}:${hit.line} [${hit.rule}] ${hit.text}`).join("\n");
}

function formatPathSummary(hits) {
  if (hits.length === 0) return "(none)";
  const counts = new Map();
  for (const hit of hits) counts.set(hit.path, (counts.get(hit.path) ?? 0) + 1);
  return [...counts.entries()].sort(([left], [right]) => left.localeCompare(right)).map(([file, count]) => `- ${file} (${count})`).join("\n");
}

function isSourceFile(file) {
  return /\.(?:rs|ts|tsx|js|mjs|json)$/u.test(file) && !file.includes(`${path.sep}node_modules${path.sep}`);
}

function filesUnder(directory) {
  const result = [];
  const pending = [directory];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const file = path.join(current, entry.name);
      if (entry.isDirectory()) pending.push(file);
      else if (entry.isFile()) result.push(file);
    }
  }
  return result;
}

function readJson(relative) {
  const file = resolve(relative);
  assert.ok(existsSync(file), `${relative} must exist`);
  return JSON.parse(readFileSync(file, "utf8"));
}

function readSource(relative) {
  const file = resolve(relative);
  assert.ok(existsSync(file), `${relative} must exist`);
  return readFileSync(file, "utf8");
}

function resolve(relative) {
  return path.join(root, ...relative.split("/"));
}

function relativePath(file) {
  return normalize(path.relative(root, file));
}

function normalize(value) {
  return value.replaceAll("\\", "/");
}
