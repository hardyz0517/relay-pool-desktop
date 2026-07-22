import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pkg = JSON.parse(await readFile("package.json", "utf8"));
const release = await readFile(".github/workflows/release.yml", "utf8");

assert.equal(pkg.scripts["test:contracts"], "node scripts/run-contract-tests.mjs");
assert.equal(
  pkg.scripts["verify:release-version"],
  "node scripts/verify-release-version.mjs",
);
assert.equal(
  pkg.scripts["verify:persistence-artifacts"],
  "node scripts/verify-persistence-v2-artifacts.mjs --tracked",
);
assert.match(pkg.scripts["verify:release-bundle"], /verify-persistence-v2-artifacts\.mjs --artifact/);
assert.match(pkg.scripts["verify:release"], /pnpm verify:persistence-artifacts/);
assert.ok(
  pkg.scripts["verify:release"].startsWith("pnpm verify:release-version &&"),
  "release metadata must fail before the expensive verification suite",
);
assert.match(pkg.scripts["verify:release"], /pnpm test:contracts/);
assert.match(pkg.scripts["verify:release"], /pnpm test/);
assert.match(pkg.scripts["verify:release"], /pnpm build/);
assert.match(pkg.scripts["verify:release"], /cargo check/);
assert.match(release, /run: pnpm verify:release/);
assert.match(release, /RELAY_POOL_RELEASE_TAG: \$\{\{ github\.ref_name \}\}/);
assert.match(release, /run: pnpm verify:release-version -- --require-tag/);
assert.match(release, /actions\/setup-python@[0-9a-f]{40}/);
assert.match(release, /run: pnpm verify:release-bundle/);
assert.match(release, /tagName: \$\{\{ github\.ref_name \}\}/);
assert.match(release, /releaseName: Relay Pool Desktop \$\{\{ github\.ref_name \}\}/);
assert.match(release, /releaseBody: \$\{\{ steps\.release_notes\.outputs\.body \}\}/);
assert.ok(
  release.indexOf("run: pnpm verify:release-version -- --require-tag") <
    release.indexOf("uses: tauri-apps/tauri-action@"),
  "tag and source versions must match before packaging or signing",
);
assert.ok(
  release.indexOf("id: release_notes") < release.indexOf("uses: tauri-apps/tauri-action@"),
  "release notes must be loaded before the draft release is created",
);
assert.ok(
  release.indexOf("uses: tauri-apps/tauri-action@") <
    release.indexOf("run: pnpm verify:release-bundle"),
  "the final bundle scan must run after Tauri creates the release artifacts",
);
assert.doesNotMatch(release, /run: node scripts\/updater-/);
