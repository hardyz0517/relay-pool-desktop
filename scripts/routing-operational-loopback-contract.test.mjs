import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const libSource = await readFile("src-tauri/src/lib.rs", "utf8");
const runtimeSource = await readFile("src-tauri/src/services/proxy/runtime.rs", "utf8");
const attemptSource = await readFile("src-tauri/src/services/proxy/attempt.rs", "utf8");
const supportSource = await readFile("src-tauri/src/test_support/routing_loopback.rs", "utf8");
const e2eSource = await readFile("src-tauri/tests/routing_loopback_e2e.rs", "utf8");
const catalogSource = await readFile("src-tauri/tests/routing_catalog_loopback.rs", "utf8");
const policySource = await readFile("src-tauri/tests/routing_policy_field_e2e.rs", "utf8");
const contractsRunner = await readFile("scripts/run-contract-tests.mjs", "utf8");

assert.match(
  libSource,
  /#\[cfg\(debug_assertions\)\]\s+pub mod test_support;/,
  "loopback test support must be debug-only and must not be cfg(test)-only",
);
assert.ok(
  !libSource.includes("#[cfg(test)]\npub mod test_support"),
  "test support must not be exposed through a production-invisible cfg(test) facade",
);

assert.match(
  runtimeSource,
  /dual_terminal_(buffered_)?lifecycle_finalizing_stream/,
  "ProxyStartConfig::new_v2 must use the single dual-terminal finalization path after Task 28",
);
assert.ok(
  !/ProxyFinalizationMode|LegacyRequestCoupled|with_legacy_request_coupled_finalization/.test(
    runtimeSource,
  ),
  "legacy request-coupled finalization must not remain as a debug/test opt-in after Task 28 deletion",
);
assert.ok(
  runtimeSource.includes("dual_cost_finalization"),
  "dual-terminal loopback path must persist typed attempt/request cost outcomes instead of stopping at request logs",
);
assert.ok(
  attemptSource.includes("CostFinalizationReservations"),
  "dual-terminal attempt/request finalizer must own bounded cost write reservations",
);

for (const [name, source] of [
  ["routing_loopback_e2e", e2eSource],
  ["routing_catalog_loopback", catalogSource],
  ["routing_policy_field_e2e", policySource],
]) {
  assert.ok(
    source.includes("RoutingLoopbackHarness::new().await"),
    `${name} must use the shared real-composition loopback harness`,
  );
  assert.ok(
    source.includes("attempt_cost_count"),
    `${name} must assert typed outcome persistence, not only HTTP success`,
  );
}

assert.ok(
  supportSource.includes("compose_app_services") &&
    supportSource.includes("RoutingExecutionRepository::new") &&
    supportSource.includes("start_proxy_with_production_startup"),
  "loopback harness must compose real application services, V2 routing repository and a production startup path",
);
assert.ok(
  !supportSource.includes("RELAY_POOL_PROXY_RUNTIME"),
  "loopback harness must not depend on process-wide runtime fallback flags",
);
assert.ok(
  contractsRunner.includes("scripts/routing-operational-loopback-contract.test.mjs"),
  "loopback contract must be wired into pnpm test:contracts",
);

console.log("routing operational loopback contract passed");
