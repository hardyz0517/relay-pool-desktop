import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const runner = readFileSync("scripts/run-routing-operational-local-self-check.ps1", "utf8");
const manifest = JSON.parse(
  readFileSync("docs/superpowers/audits/routing-operational-qualification-manifest.json", "utf8"),
);
const deletionLedger = readFileSync("docs/superpowers/audits/routing-operational-deletion-ledger.md", "utf8");

for (const suite of [
  "operational_fact_reader",
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
  "legacy-doc-anti-regression",
  "scripts/routing-operational-legacy-doc-consistency.test.mjs",
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

assert.equal(manifest.owner_task, 26, "Task 26 manifest remains the aggregate development self-check owner");
assert.ok(
  manifest.required_development_commands.includes(
    "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-soak.ps1 -Smoke",
  ),
  "Task 26 manifest must require only the single-pass deterministic soak by default",
);
assert.ok(
  !manifest.required_development_commands.some((command) => command.includes("DurationMinutes 60")),
  "Task 26 required commands must not make the optional long soak a development blocker",
);
assert.ok(
  manifest.optional_confidence_commands.some((command) => command.includes("DurationMinutes 60")),
  "optional long soak should remain documented as confidence evidence",
);
assert.ok(
  deletionLedger.includes("Supported recovery after deletion: stop admission, reset local data, reimport config, or reconfigure with the current dev binary."),
  "Task 27/28 recovery boundary must be recorded in the deletion ledger as reset/reimport/reconfigure",
);
assert.ok(
  deletionLedger.includes("Old binary rollback remains outside the development-phase contract."),
  "Task 27/28 deletion ledger must not promise old binary rollback",
);
