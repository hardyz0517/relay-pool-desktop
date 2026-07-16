import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pkg = JSON.parse(await readFile("package.json", "utf8"));
const release = await readFile(".github/workflows/release.yml", "utf8");

assert.equal(pkg.scripts["test:contracts"], "node scripts/run-contract-tests.mjs");
assert.match(pkg.scripts["verify:release"], /pnpm test:contracts/);
assert.match(pkg.scripts["verify:release"], /pnpm test/);
assert.match(pkg.scripts["verify:release"], /pnpm build/);
assert.match(pkg.scripts["verify:release"], /cargo check/);
assert.match(release, /run: pnpm verify:release/);
assert.doesNotMatch(release, /run: node scripts\/updater-/);
