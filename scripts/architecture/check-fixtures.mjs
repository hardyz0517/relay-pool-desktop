import { spawnSync } from "node:child_process";
import path from "node:path";
import { assert, repoRoot, runMain } from "./lib.mjs";

function run(script, args, shouldPass) {
  const result = spawnSync(process.execPath, [path.join(repoRoot, "scripts/architecture", script), ...args], {
    cwd: repoRoot,
    encoding: "utf8",
    windowsHide: true,
  });
  if (shouldPass) {
    assert(result.status === 0, `${script} fixture should pass:\n${result.stderr}`);
  } else {
    assert(result.status !== 0, `${script} bypass fixture was not rejected`);
  }
}

runMain(() => {
  run("check-typescript-boundaries.mjs", ["--fixtures"], true);
  run("check-command-registry.mjs", ["--root", "scripts/architecture/fixtures/gates/red-command"], false);
  run("check-tauri-security.mjs", ["--root", "scripts/architecture/fixtures/gates/red-security-csp"], false);
  run("check-tauri-security.mjs", ["--root", "scripts/architecture/fixtures/gates/red-security-main"], false);
  run("check-build-entries.mjs", ["--root", "scripts/architecture/fixtures/gates/red-build"], false);
  run("check-artifact-policy.mjs", ["--fixtures"], true);
  console.log("Architecture bypass fixtures passed");
});
