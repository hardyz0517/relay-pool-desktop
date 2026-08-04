import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { parse as parseYaml } from "yaml";

const desktopBackendSource = await readFile("src/lib/bridge/DesktopBackend.ts", "utf8");
const workflow = parseYaml(await readFile(".github/workflows/release.yml", "utf8"));
const contractRunnerSource = await readFile("scripts/run-contract-tests.mjs", "utf8");

assert.match(
  desktopBackendSource,
  /checkNative:[\s\S]*catch \(error\) \{[\s\S]*abandonNativeUpdateCheck\(\)[\s\S]*throw error/,
  "a timed-out native check must be detached before manifest inspection starts",
);

assert.match(
  desktopBackendSource,
  /private abandonNativeUpdateCheck\(\) \{[\s\S]*nativeUpdateCheckInFlight = null[\s\S]*update\?\.close\(\)/,
  "a detached native check must close a late update resource instead of leaking it into a later install",
);

assert.ok(
  workflow.jobs.release.steps.some((step) =>
    String(step.run ?? "").includes("node scripts/verify-release-preflight.mjs --require-ci"),
  ),
  "release builds must verify successful CI qualification before publishing",
);

assert.match(
  contractRunnerSource,
  /node["'], \["scripts\/updater-timeout-recovery\.test\.mjs"\]/,
  "shared release verification must run the timeout recovery regression check",
);

console.log("updater timeout recovery contract checks passed");
