import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const manifest = JSON.parse(
  readFileSync("docs/superpowers/audits/routing-operational-boundary-manifest.json", "utf8"),
);
const deletionLedger = readFileSync("docs/superpowers/audits/routing-operational-deletion-ledger.md", "utf8");
const runtimeSource = readFileSync("src-tauri/src/services/proxy/runtime.rs", "utf8");
const responseBodySource = readFileSync("src-tauri/src/services/proxy/response_body.rs", "utf8");

assert.equal(
  manifest.status,
  "task28_debug_legacy_runtime_deleted",
  "boundary manifest must record Task 28 debug legacy runtime deletion",
);

const forbiddenSymbols = new Map(
  (manifest.production_forbidden_symbols ?? []).map((entry) => [entry.symbol, entry]),
);
const requestCoupledFinalizer = forbiddenSymbols.get("default-v2 request-coupled response finalization");
assert.ok(requestCoupledFinalizer, "request-coupled finalization must stay registered as a forbidden production symbol");
assert.equal(requestCoupledFinalizer.delete_by_task, 28, "request-coupled finalization must be owned by Task 28 deletion");
assert.deepEqual(
  new Set(requestCoupledFinalizer.paths),
  new Set(["src-tauri/src/services/proxy/runtime.rs", "src-tauri/src/services/proxy/response_body.rs"]),
  "request-coupled finalization forbidden symbol must cover runtime and response body owners",
);
assert.match(
  requestCoupledFinalizer.reason,
  /dual-terminal finalization path|must not return/u,
  "request-coupled finalization reason must point to the dual-terminal replacement and anti-regression intent",
);

for (const exception of manifest.temporary_allowed_exceptions ?? []) {
  const haystack = JSON.stringify(exception);
  assert.doesNotMatch(
    haystack,
    /RELAY_POOL_PROXY_RUNTIME=legacy|debug legacy runtime|request-coupled finalization/u,
    `temporary exception ${exception.id ?? "<unknown>"} must not preserve the deleted runtime/finalizer`,
  );
}

for (const [sourceName, source, forbidden] of [
  [
    "runtime.rs",
    runtimeSource,
    /RELAY_POOL_PROXY_RUNTIME|LegacyRequestCoupled|with_legacy_request_coupled_finalization/u,
  ],
  [
    "response_body.rs",
    responseBodySource,
    /LifecycleFinalizationLease|SelectedAttemptFinalization|FinalizationTarget::Lifecycle/u,
  ],
]) {
  assert.doesNotMatch(source, forbidden, `${sourceName} must not contain deleted legacy runtime/finalizer symbols`);
}

for (const requiredLedgerText of [
  "Old request-coupled response finalizer",
  "Debug legacy runtime",
  "Deleted;",
  "Supported recovery after deletion: stop admission, reset local data, reimport config, or reconfigure with the current dev binary.",
  "Old binary rollback remains outside the development-phase contract.",
]) {
  assert.ok(
    deletionLedger.includes(requiredLedgerText),
    `deletion ledger must include structured Task 28 evidence: ${requiredLedgerText}`,
  );
}

console.log("routing operational legacy doc consistency ok");
