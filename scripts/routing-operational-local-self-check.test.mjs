import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const runner = readFileSync("scripts/run-routing-operational-local-self-check.ps1", "utf8");
const plan = readFileSync(
  "docs/superpowers/plans/2026-07-30-routing-operational-unification-upgrade.md",
  "utf8",
);

for (const suite of [
  "persistence_upgrade",
  "persistence_upgrade_recovery",
  "persistence_startup_cutover",
  "routing_url_sanitizer_migration",
  "routing_lifecycle_reconciliation",
  "routing_production_startup_shutdown",
  "routing_policy_field_e2e",
  "routing_catalog_loopback",
]) {
  assert.ok(runner.includes(`"--test", "${suite}"`), `local self-check must run ${suite}`);
}

for (const text of [
  "schemaVersion = 1",
  "sourceRevision",
  "worktreeCleanAtStart",
  "worktreeCleanAtFinish",
  "realProviderStatus",
  "not-run-without-user-authorization",
  "reset/reimport/reconfigure with the current dev binary",
  "trackedRuntimeResultsAllowed = $false",
]) {
  assert.ok(runner.includes(text), `local self-check report contract is missing ${text}`);
}

for (const forbidden of [
  "candidateRevision",
  "TAURI_SIGNING_PRIVATE_KEY",
  "cargo build --release",
  "pnpm.cmd tauri:build",
]) {
  assert.ok(!runner.includes(forbidden), `local self-check must not require ${forbidden}`);
}

assert.ok(
  plan.includes("reset/reimport/重新配置") &&
    plan.includes("不维护公开签名预迁移版本") &&
    plan.includes("本地自检"),
  "Task 27 plan must preserve development reset/reimport boundary",
);
