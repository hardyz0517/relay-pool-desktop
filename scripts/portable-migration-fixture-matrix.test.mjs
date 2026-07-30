import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const repoRoot = process.cwd();
const fixtureRoot = path.join(repoRoot, "src-tauri", "tests", "fixtures", "portable-migration");
const manifestPath = path.join(fixtureRoot, "manifest.json");
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));

const requiredIds = new Set([
  "current-valid-manifest",
  "supported-v1-reader-valid",
  "wrong-password-envelope",
  "truncated-frame",
  "unknown-required-feature",
  "too-new-schema",
  "malformed-sqlite",
  "trigger-view-schema-object",
  "foreign-key-broken",
  "resource-overflow",
]);

assert.equal(manifest.schemaVersion, 1, "portable migration fixture manifest schemaVersion must be 1");
assert.equal(manifest.format, "relay-pool-portable-migration", "fixture manifest must target the portable migration format");
assert.equal(manifest.profile, "encrypted-secrets-v1", "fixture manifest must target the frozen v1 profile");
assert.ok(Array.isArray(manifest.fixtures), "fixture manifest must contain fixtures[]");
assert.equal(new Set(manifest.fixtures.map((fixture) => fixture.id)).size, manifest.fixtures.length, "fixture ids must be unique");

for (const requiredId of requiredIds) {
  assert.ok(manifest.fixtures.some((fixture) => fixture.id === requiredId), `missing required portable migration fixture case: ${requiredId}`);
}

for (const fixture of manifest.fixtures) {
  assert.match(fixture.id, /^[a-z0-9-]+$/, `fixture id must be kebab-case: ${fixture.id}`);
  assert.ok(!/monitor/i.test(fixture.id), `portable migration fixture must not add monitor semantics: ${fixture.id}`);
  assert.equal(fixture.formatVersion, 1, `${fixture.id} must declare formatVersion=1`);
  assert.equal(fixture.databaseGeneration, 2, `${fixture.id} must declare databaseGeneration=2`);
  assert.equal(fixture.profile, "encrypted-secrets-v1", `${fixture.id} must declare encrypted-secrets-v1`);
  assert.ok(Array.isArray(fixture.requiredFeatures), `${fixture.id} requiredFeatures must be an array`);
  assert.ok(fixture.expected?.outcome === "valid" || fixture.expected?.outcome === "error", `${fixture.id} must declare valid/error outcome`);
  assert.ok(Array.isArray(fixture.evidence) && fixture.evidence.length > 0, `${fixture.id} must cite automated evidence`);

  if (fixture.path === null) {
    assert.equal(fixture.sha256, null, `${fixture.id} must not fake sha256 for generated runtime fixtures`);
  } else {
    assert.equal(typeof fixture.path, "string", `${fixture.id} path must be a string or null`);
    const absolutePath = path.join(fixtureRoot, fixture.path);
    assert.ok(absolutePath.startsWith(fixtureRoot), `${fixture.id} fixture path must stay under portable-migration fixtures`);
    assert.ok(fs.existsSync(absolutePath), `${fixture.id} fixture file is missing: ${fixture.path}`);
    const digest = crypto.createHash("sha256").update(fs.readFileSync(absolutePath)).digest("hex");
    assert.equal(fixture.sha256, digest, `${fixture.id} sha256 drifted`);
  }

  for (const evidence of fixture.evidence) {
    assert.equal(typeof evidence.path, "string", `${fixture.id} evidence.path is required`);
    assert.equal(typeof evidence.symbol, "string", `${fixture.id} evidence.symbol is required`);
    const evidencePath = path.join(repoRoot, evidence.path);
    assert.ok(fs.existsSync(evidencePath), `${fixture.id} evidence file is missing: ${evidence.path}`);
    const source = fs.readFileSync(evidencePath, "utf8");
    assert.ok(source.includes(evidence.symbol), `${fixture.id} evidence symbol not found: ${evidence.symbol}`);
  }
}

const valid = manifest.fixtures.filter((fixture) => fixture.expected.outcome === "valid");
const errors = manifest.fixtures.filter((fixture) => fixture.expected.outcome === "error");
assert.ok(valid.length >= 2, "fixture matrix must retain current and supported reader valid cases");
assert.ok(errors.length >= 8, "fixture matrix must retain the required failure boundary cases");
assert.ok(!JSON.stringify(manifest).includes("__SHA256__"), "fixture manifest must not contain placeholder hashes");

console.log(`portable migration fixture matrix covers ${manifest.fixtures.length} cases`);
