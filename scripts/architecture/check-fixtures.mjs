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
  run("../routing-operational-architecture.test.mjs", ["--root", "scripts/fixtures/routing-operational-architecture/pass"], true);
  run("../routing-operational-architecture.test.mjs", ["--root", "scripts/fixtures/routing-operational-architecture/red-monitoring-routing-dto"], false);
  run("../routing-operational-architecture.test.mjs", ["--root", "scripts/fixtures/routing-operational-architecture/red-routing-kernel-io"], false);
  run("../routing-operational-architecture.test.mjs", ["--root", "scripts/fixtures/routing-operational-architecture/red-frontend-truth"], false);
  run("../routing-operational-architecture.test.mjs", ["--root", "scripts/fixtures/routing-operational-architecture/red-credential-serialize"], false);
  run("../routing-operational-architecture.test.mjs", ["--root", "scripts/fixtures/routing-operational-architecture/red-test-only-scheduler"], false);
  run("../routing-operational-architecture.test.mjs", ["--root", "scripts/fixtures/routing-operational-architecture/red-hierarchical-weights"], false);
  run("../routing-operational-architecture.test.mjs", ["--root", "scripts/fixtures/routing-operational-architecture/red-unregistered-boundary-symbol"], false);
  run("../intelligent-routing-architecture.test.mjs", ["--fixtures"], true);
  console.log("Architecture bypass fixtures passed");
});
