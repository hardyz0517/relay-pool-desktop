import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const root = process.cwd();
const catalog = JSON.parse(read("src-tauri/tests/fixtures/upstream_errors/catalog.v1.json"));

assert.equal(catalog.schema_version, 1);
assert.equal(catalog.profile_version, "upstream-error-fixtures-v1");
assert.deepEqual(catalog.scope.included_endpoints, [
  "/v1/models",
  "/v1/chat/completions",
  "/v1/responses",
  "/v1/embeddings",
]);
assert.ok(catalog.cases.length >= 30, "catalog must retain a broad baseline matrix");
assert.equal(new Set(catalog.cases.map(({ id }) => id)).size, catalog.cases.length);

for (const required of [
  "provider_capacity", "overload", "credential", "account", "balance",
  "quota", "usage_window", "concurrency", "queue_saturation", "rate_limit",
  "capability", "request", "timeout", "server", "conflicting",
  "malformed_or_oversized", "downstream_cancel", "postcommit_stream_failure",
  "success", "unknown_protocol_event",
]) {
  assert.ok(catalog.cases.some(({ family }) => family === required), `missing family ${required}`);
}
for (const transport of ["http", "sse"]) {
  assert.ok(catalog.cases.some((entry) => entry.transport === transport), `missing ${transport}`);
}
for (const profile of ["sub2api", "generic_openai_gateway"]) {
  assert.ok(catalog.cases.some((entry) => entry.profile === profile), `missing profile ${profile}`);
}
for (const entry of catalog.cases) {
  assert.ok(entry.id && entry.profile && entry.transport && entry.envelope);
  assert.ok(entry.family && entry.scope && entry.retry && entry.confidence);
  assert.ok(Array.isArray(entry.endpoints) && entry.endpoints.length > 0);
  assert.ok(entry.endpoints.every((endpoint) => catalog.scope.included_endpoints.includes(endpoint)));
}

if (process.argv.includes("--catalog-only")) {
  console.log("upstream error fixture catalog baseline passed");
  process.exit(0);
}

// Task 0 RED contract. Each assertion describes the post-cutover production
// topology and intentionally fails against the captured baseline. Keep this
// script outside the ordinary green suite until Task 9 closes every assertion.
const openai = read("src-tauri/src/services/proxy/adapters/openai.rs");
const execution = read("src-tauri/src/services/proxy/execution.rs");
// Test fixtures intentionally retain historical capacity-domain regression
// cases.  Contract assertions describe the production boundary only.
const executionProduction = execution.split("\n#[cfg(test)]\nmod tests {", 1)[0];
const upstream = read("src-tauri/src/services/proxy/upstream.rs");
const proxyError = read("src-tauri/src/services/proxy/error.rs");

const violations = [];
rejectIf(openai, /400\s*\|\s*409\s*\|\s*422\s*=>\s*ProviderErrorSemanticSignal::BadRequest/u,
  "RED-01: HTTP 400 capacity is still collapsed into BadRequest");
rejectIf(execution, /Ok\(Some\(Ok\(bytes\)\)\)\s*=>[\s\S]*?stream::once/u,
  "RED-02: first non-empty TCP chunk is still treated as committed output");
rejectIf(execution, /ProxyFailure::from_public_error\(canonical\.public\)/u,
  "RED-03: CanonicalOutcome effects are still discarded at ProxyFailure projection");
rejectIf(openai, /429\s*=>\s*ProviderErrorSemanticSignal::RateLimited/u,
  "RED-04: all 429 responses still share one semantic signal");
rejectIf(openai, /401\s*=>\s*ProviderErrorSemanticSignal::ConfirmedAuthentication/u,
  "RED-05: every 401 is still attributed to the current credential");
requireMatch(proxyError,
  /if self\.source == FailureSource::Upstream \{[\s\S]*?adapt_proxy_failure\(&self\)[\s\S]*?return \(public\.status, public\.into_json\(\)\);/u,
  "RED-06: upstream failures do not consistently use the OpenAI-compatible adapter");
requireMatch(execution, /SseBootstrapMachine[\s\S]*?PrecommitTerminal[\s\S]*?precommit_protocol_terminal_failure/u,
  "RED-07: production streaming has no semantic bootstrap classifier");
rejectIf(execution, /fn\s+health_effect\s*\(failure:\s*&ProxyFailure\)/u,
  "RED-08/11: execution still reconstructs health from ProxyFailure");
rejectIf(execution, /match\s+failure\.http_status\.as_u16\(\)/u,
  "RED-08: retry/health consumers still classify by HTTP status");
rejectIf(upstream, /response\.bytes\(\)\.await/u,
  "RED-09: upstream error body is still read without an explicit bound");
rejectIf(executionProduction, /ProviderCapacityDomain|CapacityDomainCommitment|capacity_domain/u,
  "RED-10: production execution still carries a provider-capacity-domain retry contract");

assert.deepEqual(violations, [], `upstream cutover remains RED:\n- ${violations.join("\n- ")}`);

console.log("upstream error contract assertions passed");

function read(relativePath) {
  return readFileSync(path.join(root, ...relativePath.split("/")), "utf8");
}

function rejectIf(source, pattern, message) {
  if (pattern.test(source)) violations.push(message);
}

function requireMatch(source, pattern, message) {
  if (!pattern.test(source)) violations.push(message);
}
