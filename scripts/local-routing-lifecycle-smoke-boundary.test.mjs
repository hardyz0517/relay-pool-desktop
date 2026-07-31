import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync("scripts/verify-local-routing-lifecycle.ps1", "utf8");
const contracts = readFileSync("scripts/run-contract-tests.mjs", "utf8");
const task27Template = readFileSync(
  "docs/superpowers/audits/routing-operational-local-self-check-template.md",
  "utf8",
);

assert.ok(
  source.includes("[switch]$AuthorizeLocalClientSmoke") &&
    source.includes("Local routing lifecycle smoke is disabled by default"),
  "local client smoke must require explicit authorization before sending HTTP",
);

assert.ok(
  source.includes("output\\routing-operational\\qualification\\local-client-smoke\\routing-local-client-smoke-summary.json"),
  "local client smoke summary must default to ignored routing operational output",
);

assert.ok(
  source.includes('kind = "routing-operational-local-client-smoke"') &&
    source.includes("rawBaseUrlStored = $false") &&
    source.includes("localBearerPrinted = $false") &&
    source.includes("realProviderCredentialRead = $false"),
  "local client smoke summary must declare non-secret evidence boundaries",
);

for (const behavior of [
  'path = "/v1/models"',
  'path = "/v1/chat/completions"',
  'path = "/v1/responses"',
  "chat-stream-cancel",
  "verify-request-lifecycle-db.ps1",
]) {
  assert.ok(source.includes(behavior), `local client smoke must cover ${behavior}`);
}

assert.ok(
  contracts.includes('["node", ["scripts/local-routing-lifecycle-smoke-boundary.test.mjs"]]'),
  "local client smoke boundary contract must be part of pnpm test:contracts",
);

assert.ok(
  task27Template.includes("scripts/verify-local-routing-lifecycle.ps1") &&
    task27Template.includes("-AuthorizeLocalClientSmoke"),
  "Task 27 template must route real local client smoke through the authorization-gated harness",
);
