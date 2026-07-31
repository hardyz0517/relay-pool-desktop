import { readFileSync } from "node:fs";

const predeletionGate = readFileSync("scripts/run-routing-task24-predeletion-gate.ps1", "utf8");
const soakRunner = readFileSync("scripts/run-routing-operational-soak.ps1", "utf8");
const contractsRunner = readFileSync("scripts/run-contract-tests.mjs", "utf8");

function assertIncludes(source, text, label) {
  if (!source.includes(text)) {
    throw new Error(`${label} must include ${text}`);
  }
}

function assertExcludes(source, text, label) {
  if (source.includes(text)) {
    throw new Error(`${label} must not include ${text}`);
  }
}

for (const [source, label] of [
  [predeletionGate, "Task 24 pre-deletion gate"],
  [soakRunner, "routing operational soak runner"],
]) {
  assertIncludes(source, "git rev-parse HEAD", label);
  assertIncludes(source, "git status --porcelain", label);
  assertIncludes(source, "candidateRevision", label);
  assertIncludes(source, "worktreeCleanAtStart", label);
  assertIncludes(source, "ConvertTo-Json", label);
  assertIncludes(source, "output/routing-operational/qualification", label);
  assertExcludes(source, "Remove-Item", label);
  assertExcludes(source, "rm -", label);
  assertExcludes(source, "git add", label);
}

assertIncludes(predeletionGate, "routing_production_composition", "Task 24 pre-deletion gate");
assertIncludes(predeletionGate, "routing_stream_finalization_faults", "Task 24 pre-deletion gate");
assertIncludes(predeletionGate, "scripts/local-routing-redaction.test.mjs", "Task 24 pre-deletion gate");
assertIncludes(predeletionGate, "scripts/run-routing-operational-soak.ps1", "Task 24 pre-deletion gate");
assertIncludes(predeletionGate, "deletionApproved", "Task 24 pre-deletion gate");
assertIncludes(predeletionGate, "DurationMinutes -ge 60", "Task 24 pre-deletion gate");
assertIncludes(predeletionGate, "-not [bool]$Smoke", "Task 24 pre-deletion gate");

assertIncludes(soakRunner, "routing_loopback_e2e", "routing operational soak runner");
assertIncludes(soakRunner, "routing_catalog_loopback", "routing operational soak runner");
assertIncludes(soakRunner, "routing_policy_field_e2e", "routing operational soak runner");

assertIncludes(
  contractsRunner,
  "scripts/routing-task24-predeletion-gate.test.mjs",
  "contract test registry",
);

console.log("routing Task 24 pre-deletion gate contract passed");
