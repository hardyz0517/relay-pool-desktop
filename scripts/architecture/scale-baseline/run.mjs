import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { assert, currentRevision, repoRoot, runMain } from "../lib.mjs";

function invoke(command, args, env = {}) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8", windowsHide: true, env: { ...process.env, ...env } });
  assert(result.status === 0, `${command} ${args.join(" ")} failed:\n${result.stdout}\n${result.stderr}`);
}

runMain(() => {
  const root = "output/architecture-scale/baseline";
  const first = `${root}/fixtures-first.json`;
  const second = `${root}/fixtures-second.json`;
  const report = `${root}/frontend-report.json`;
  invoke(process.execPath, ["scripts/architecture/scale-baseline/generate.mjs", "--output", first]);
  invoke(process.execPath, ["scripts/architecture/scale-baseline/generate.mjs", "--output", second]);
  assert(fs.readFileSync(path.join(repoRoot, first), "utf8") === fs.readFileSync(path.join(repoRoot, second), "utf8"), "fixed-seed fixtures differ across consecutive generation");
  invoke(process.execPath, [
    "node_modules/vitest/vitest.mjs",
    "run",
    "--config",
    "scripts/architecture/scale-baseline/vitest.config.ts",
  ], {
    ARCHITECTURE_SCALE_REPORT: path.join(repoRoot, report),
    ARCHITECTURE_SOURCE_REVISION: currentRevision(),
  });
  invoke(process.execPath, ["scripts/architecture/scale-baseline/validate-report.mjs", "--fixtures", first, "--report", report]);
  console.log(`Frontend scale baseline written to ${report}`);
});
