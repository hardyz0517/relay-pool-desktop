import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync("scripts/write-routing-operational-manual-observation.ps1", "utf8");
const contracts = readFileSync("scripts/run-contract-tests.mjs", "utf8");
const task27Template = readFileSync(
  "docs/superpowers/audits/routing-operational-local-self-check-template.md",
  "utf8",
);
const deletionLedger = readFileSync(
  "docs/superpowers/audits/routing-operational-deletion-ledger.md",
  "utf8",
);

for (const scenario of [
  "real_local_client_smoke",
  "real_provider_semantic",
  "ccswitch_fixed_local_entry",
  "windows_sleep_resume",
  "ui_timeline_sqlite_reconciliation",
  "default_v2_no_p0_p1",
]) {
  assert.ok(source.includes(`"${scenario}"`), `manual observation writer must support ${scenario}`);
}

assert.ok(
  source.includes("[switch]$AuthorizeManualObservation") &&
    source.includes("Manual routing operational observation recording is disabled by default"),
  "manual observation writer must fail closed without explicit authorization",
);

assert.ok(
  source.includes('$Status -eq "passed" -and $EvidenceIndex.Count -eq 0'),
  "manual observation writer must reject pass records without evidence references",
);

assert.ok(
  source.includes("output\\routing-operational\\qualification\\manual-observation\\routing-operational-manual-observation-latest.json"),
  "manual observation writer must default to ignored routing operational output",
);

for (const boundary of [
  "recordOnly = $true",
  "copiesEvidenceFiles = $false",
  "storesRawSecrets = $false",
  "storesRawProviderUrl = $false",
  "storesLocalDatabase = $false",
]) {
  assert.ok(source.includes(boundary), `manual observation record must declare ${boundary}`);
}

assert.ok(
  contracts.includes('["node", ["scripts/routing-operational-manual-observation-boundary.test.mjs"]]'),
  "manual observation boundary contract must be part of pnpm test:contracts",
);

assert.ok(
  task27Template.includes("scripts/write-routing-operational-manual-observation.ps1") &&
    deletionLedger.includes("write-routing-operational-manual-observation.ps1"),
  "Task 27 template and deletion ledger must point manual checks to the manual observation writer",
);
