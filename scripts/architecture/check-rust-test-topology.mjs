import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const testsRoot = path.join(root, "src-tauri", "tests");

// Ratchet only: every entry is existing debt and the set may only shrink.
// Scenario tests must use relay_pool_desktop_lib::test_support instead.
const legacySourceAssembly = new Set([
  "capability_evidence.rs",
  "execution_target_resolver.rs",
  "monitoring_adapter_contracts.rs",
  "monitoring_buckets_retention.rs",
  "monitoring_concurrency.rs",
  "monitoring_domain.rs",
  "monitoring_execution_integration.rs",
  "monitoring_faults.rs",
  "monitoring_orchestrator.rs",
  "monitoring_persistence.rs",
  "monitoring_profile_golden.rs",
  "monitoring_read_model.rs",
  "monitoring_scheduler.rs",
  "monitoring_transport.rs",
  "monitoring_write_path.rs",
  "observability_contract.rs",
  "operational_domain.rs",
  "operational_economics_projectors.rs",
  "operational_fact_reader.rs",
  "operational_health_projection.rs",
  "operational_pricing_projection.rs",
  "operational_projector_contract.rs",
  "persistence_runtime.rs",
  "persistence_sessions.rs",
  "pricing_group_monitor_status.rs",
  "provider_conformance.rs",
  "proxy_lifecycle_concurrency.rs",
  "proxy_lifecycle_faults.rs",
  "proxy_protocol_contracts.rs",
  "route_candidate_projection.rs",
  "routing_capacity.rs",
  "routing_capacity_faults.rs",
  "routing_decision_store.rs",
  "routing_dual_terminal_lifecycle.rs",
  "routing_health_verdict_persistence.rs",
  "routing_lifecycle_reconciliation.rs",
  "routing_outcome_domain.rs",
  "routing_outcome_persistence.rs",
  "routing_runtime_state.rs",
  "routing_url_sanitizer_migration.rs",
  "station_key_health_transitions.rs",
]);

async function findRustTests(directory, relativeDirectory = "") {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const relativePath = path.posix.join(relativeDirectory, entry.name);
      if (entry.isDirectory()) {
        return findRustTests(path.join(directory, entry.name), relativePath);
      }
      return entry.isFile() && entry.name.endsWith(".rs") ? [relativePath] : [];
    }),
  );
  return nested.flat();
}

const rustTests = await findRustTests(testsRoot);
const offenders = [];

for (const relativePath of rustTests) {
  const source = await readFile(path.join(testsRoot, relativePath), "utf8");
  const assemblesProductionSource =
    /#\s*\[\s*path\s*=\s*"[^"]*(?:\.\.\/|\.\.\\)src[\\/]/u.test(source) ||
    /include!\s*\([\s\S]{0,300}(?:CARGO_MANIFEST_DIR|\.\.\/src|\.\.\\src)/u.test(source);
  if (assemblesProductionSource) offenders.push(relativePath);
}

const actual = new Set(offenders);
const additions = offenders.filter((file) => !legacySourceAssembly.has(file));
const removed = [...legacySourceAssembly].filter((file) => !actual.has(file));

assert.deepEqual(
  additions,
  [],
  `new Rust integration tests must not assemble production .rs files with #[path]/include!: ${additions.join(", ")}`,
);
assert.deepEqual(
  removed,
  [],
  `source-assembly debt was removed; delete these files from the ratchet allowlist: ${removed.join(", ")}`,
);

for (const migrated of [
  "proxy_lifecycle_domain.rs",
  "routing_failure_contract.rs",
  "routing_stream_finalization_faults.rs",
]) {
  const source = await readFile(path.join(testsRoot, migrated), "utf8");
  assert.match(source, /relay_pool_desktop_lib::test_support/u, `${migrated} must use the real crate test boundary`);
}

const runtimeComposition = await readFile(
  path.join(root, "src-tauri", "src", "runtime_composition.rs"),
  "utf8",
);
assert.doesNotMatch(
  runtimeComposition,
  /\bReadyServiceBundle\b|\bregister_ready_services(?:_in)?\b/u,
  "runtime_composition.rs must not restore the deleted five-slot registration path",
);

console.log(`Rust test topology gate passed (${offenders.length} ratcheted legacy files)`);
