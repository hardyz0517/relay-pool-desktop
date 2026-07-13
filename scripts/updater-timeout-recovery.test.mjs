import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const updaterApiSource = await readFile("src/lib/api/updater.ts", "utf8");
const workflowSource = await readFile(".github/workflows/release.yml", "utf8");

assert.match(
  updaterApiSource,
  /checkNative:[\s\S]*catch \(error\) \{[\s\S]*abandonNativeUpdateCheck\(\)[\s\S]*throw error/,
  "a timed-out native check must be detached before manifest inspection starts",
);

assert.match(
  updaterApiSource,
  /function abandonNativeUpdateCheck\(\) \{[\s\S]*nativeUpdateCheckInFlight = null[\s\S]*update\?\.close\(\)/,
  "a detached native check must close a late update resource instead of leaking it into a later install",
);

assert.match(
  workflowSource,
  /node scripts\/updater-timeout-recovery\.test\.mjs/,
  "release builds must run the timeout recovery regression check",
);

console.log("updater timeout recovery contract checks passed");
