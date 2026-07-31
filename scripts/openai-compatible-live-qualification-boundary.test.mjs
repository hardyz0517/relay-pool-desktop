import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync("scripts/run-openai-compatible-live-qualification.ps1", "utf8");
const contracts = readFileSync("scripts/run-contract-tests.mjs", "utf8");

assert.ok(
  source.includes("[string]$OutputPath = \"output\\architecture-scale\\qualification\\live-provider\\openai-compatible-live-qualification-summary.json\""),
  "live provider summary must default to ignored output/ rather than tracked docs",
);

assert.ok(
  source.includes("RELAY_POOL_LIVE_API_KEY is required"),
  "live provider probe must fail closed without explicit secret authorization",
);

assert.ok(
  source.includes("function Get-EndpointEvidence") &&
    source.includes("raw_url_stored = $false") &&
    source.includes("sha256 = Get-Sha256Hex $normalized") &&
    source.includes("host_class ="),
  "live provider probe must store redacted endpoint evidence instead of raw base URL",
);

assert.ok(
  !source.includes('endpoint = $BaseUrl.TrimEnd("/")'),
  "live provider summary must not persist the raw provider base URL",
);

for (const redaction of [
  "(?i)bearer\\s+",
  "(authorization|cookie|api[-_]?key|token)",
  "https?://",
]) {
  assert.ok(source.includes(redaction), `live provider redaction must cover ${redaction}`);
}

assert.ok(
  contracts.includes('["node", ["scripts/openai-compatible-live-qualification-boundary.test.mjs"]]'),
  "live provider boundary contract must be part of pnpm test:contracts",
);
