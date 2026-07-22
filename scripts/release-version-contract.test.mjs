import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";

import { validateReleaseMetadata } from "./verify-release-version.mjs";

const pkg = JSON.parse(await readFile("package.json", "utf8"));
const script = await readFile("scripts/verify-release-version.mjs", "utf8");
const run = (args = [], env = {}) =>
  spawnSync(process.execPath, ["scripts/verify-release-version.mjs", ...args], {
    cwd: process.cwd(),
    encoding: "utf8",
    env: { ...process.env, ...env },
    shell: false,
  });

assert.equal(run().status, 0, "the checked-in package/Cargo/Tauri versions must agree");
assert.equal(
  run(["--require-tag"], { RELAY_POOL_RELEASE_TAG: `v${pkg.version}` }).status,
  0,
  "the exact source tag must pass",
);
assert.notEqual(run(["--require-tag"], { RELAY_POOL_RELEASE_TAG: "v99.0.0" }).status, 0);
assert.notEqual(run(["--require-tag"], { RELAY_POOL_RELEASE_TAG: "" }).status, 0);
assert.notEqual(run(["--unknown"]).status, 0);

assert.throws(() =>
  validateReleaseMetadata({
    packageVersion: "0.3.2",
    cargoVersion: "0.3.1",
    tauriVersion: "../package.json",
  }),
);
assert.throws(() =>
  validateReleaseMetadata({
    packageVersion: "0.3.2",
    cargoVersion: "0.3.2",
    tauriVersion: "0.3.2",
  }),
);
assert.equal(
  validateReleaseMetadata({
    packageVersion: "0.4.0-rc.1",
    cargoVersion: "0.4.0-rc.1",
    tauriVersion: "../package.json",
    releaseTag: "v0.4.0-rc.1",
    requireTag: true,
  }),
  "v0.4.0-rc.1",
);
assert.match(script, /"--locked"/);
