import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const manifest = JSON.parse(
  await readFile("docs/superpowers/audits/persistence-v2-released-schema-manifest.json", "utf8"),
);
const generator = await readFile("scripts/build-persistence-v2-fixtures.ps1", "utf8");

assert.equal(manifest.version, 3);
assert.equal(manifest.scope, undefined, "release membership belongs in the releases map");
assert.ok(Object.keys(manifest.profiles).length > 0, "at least one released profile is required");
assert.ok(Object.keys(manifest.releases).length > 0, "at least one released version is required");

for (const [tag, release] of Object.entries(manifest.releases)) {
  const profile = manifest.profiles[release.schema_profile];
  assert.ok(profile, `${tag} must reference an existing profile`);
  assert.ok(profile.releases.includes(tag), `${tag} must be linked back from its profile`);
  assert.match(release.tree, /^[0-9a-f]{40}$/, `${tag} must record its source tree`);
  assert.equal(release.raw_schema_hash, profile.fixture_raw_schema_hash);
  assert.equal(release.semantic_base_schema_hash, profile.semantic_base_schema_hash);
  assert.equal(release.fixture_sha256, profile.fixture_sha256);
  if (release.request_lifecycle_schema_hash) {
    assert.ok(
      profile.accepted_capabilities.request_lifecycle.includes(
        release.request_lifecycle_schema_hash,
      ),
      `${tag} must declare its request lifecycle capability on the profile`,
    );
  }
}

assert.match(generator, /fixture_sha256 = \$release\.Value\.evidence\.fixture_hash/);
assert.doesNotMatch(generator, /generated-task-12/);
