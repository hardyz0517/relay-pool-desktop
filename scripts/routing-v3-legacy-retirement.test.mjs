import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";

const root = process.cwd();
const ledgerPath = "docs/audits/routing-v3-legacy-retirement-ledger.json";
const ledger = JSON.parse(read(ledgerPath));

assert.equal(ledger.schemaVersion, 2, "legacy retirement ledger schema must stay versioned");
assert.equal(ledger.readOwnerVersion, "routing-v3-circuit-read-v1");
assert.equal(ledger.rollbackFloor, "v3-read-side-cutover");
assert.ok(Array.isArray(ledger.items) && ledger.items.length >= 25);

const requiredFields = [
  "id",
  "owner",
  "consumer",
  "classification",
  "replacement",
  "earliestRemovalPhase",
  "rollbackFloor",
  "compatibilityClass",
  "status",
  "deletionCondition",
  "verificationCommand",
  "evidence",
];
const allowedStatuses = new Set([
  "blocked",
  "blocked_by_qualification",
  "deferred_schema_cleanup",
  "in_progress",
  "removed",
  "retained",
  "retained_for_capability",
  "retained_for_decoder",
]);
const ids = new Set();
for (const item of ledger.items) {
  for (const field of requiredFields) {
    assert.ok(Object.hasOwn(item, field), `${item.id ?? "ledger item"} is missing ${field}`);
  }
  assert.match(item.id, /^R3LR-\d{3}$/u);
  assert.ok(allowedStatuses.has(item.status), `${item.id} has unsupported status ${item.status}`);
  assert.ok(item.consumer.length > 0, `${item.id} must identify its consumer`);
  assert.ok(item.deletionCondition.length > 0, `${item.id} must define its deletion condition`);
  assert.ok(item.verificationCommand.length > 0, `${item.id} must define its verification command`);
  assert.ok(Array.isArray(item.evidence) && item.evidence.length > 0, `${item.id} needs evidence`);
  assert.ok(!ids.has(item.id), `duplicate legacy retirement id ${item.id}`);
  ids.add(item.id);
}
for (const id of Array.from({ length: 25 }, (_, index) => `R3LR-${String(index + 1).padStart(3, "0")}`)) {
  assert.ok(ids.has(id), `legacy retirement ledger is missing ${id}`);
}

const requestFinalization = read("src-tauri/src/application/request_finalization/mod.rs");
const monitoringWritePath = read("src-tauri/src/application/monitoring/write_path.rs");
const applicationRouting = read("src-tauri/src/application/routing.rs");
const healthProtection = read("src-tauri/src/application/health_protection.rs");
const routingWorkspace = read("src-tauri/src/application/queries/routing_workspace.rs");
const routingProtection = read("src-tauri/src/application/queries/routing_protection.rs");
const credentialStore = read("src-tauri/src/persistence/stores/credential_store.rs");
const keyPoolQuery = read("src-tauri/src/application/queries/key_pool.rs");

for (const [file, source] of [
  ["src-tauri/src/application/request_finalization/mod.rs", requestFinalization],
  ["src-tauri/src/application/monitoring/write_path.rs", monitoringWritePath],
  ["src-tauri/src/application/routing.rs", applicationRouting],
]) {
  assert.doesNotMatch(
    stripRustTests(source),
    /HealthTransitionService|HealthObservationStore|station_key_health_observations|routing_health_snapshot/u,
    `${file} must not write the retired station-key health chain`,
  );
}

assert.doesNotMatch(
  monitoringWritePath,
  /models\s*::\s*health|health\s*::\s*\{|\bHealthObservation(?:Outcome|Source)?\b|routing_observation_from_health/u,
  "monitoring must construct the canonical RoutingObservation directly",
);
assert.match(
  stripRustTests(monitoringWritePath),
  /RoutingObservation\s*\{/u,
  "monitoring must keep its V3 observation write",
);
assert.doesNotMatch(
  read("src-tauri/src/models/mod.rs"),
  /pub\s*(?:\(crate\))?\s+mod\s+health\b/u,
  "the retired transient health model module must stay unregistered",
);

for (const [file, source] of [
  ["src-tauri/src/application/queries/routing_workspace.rs", routingWorkspace],
  ["src-tauri/src/application/queries/routing_protection.rs", routingProtection],
  ["src-tauri/src/persistence/stores/credential_store.rs", credentialStore],
  ["src-tauri/src/application/queries/key_pool.rs", keyPoolQuery],
]) {
  assert.doesNotMatch(
    stripRustTests(source),
    /routing_health_snapshot/u,
    `${file} must read station-key availability from the V3 circuit read model`,
  );
}

for (const retiredPath of [
  "src-tauri/src/application/error_rate_protection.rs",
  "src-tauri/src/application/routing_engine/coordinator.rs",
  "src-tauri/src/application/routing_engine/eligibility.rs",
  "src-tauri/src/application/routing_engine/failure_domains.rs",
  "src-tauri/src/application/routing_engine/hierarchical_preview.rs",
  "src-tauri/src/application/station_capacity_domains.rs",
  "src-tauri/src/models/station_capacity_domains.rs",
  "src-tauri/src/models/health.rs",
  "src-tauri/src/persistence/stores/routing_error_rate_history_store.rs",
  "src-tauri/src/persistence/stores/station_capacity_domain_store.rs",
  "src-tauri/tests/intelligent_routing_coordinator.rs",
]) {
  assert.ok(!existsSync(resolve(retiredPath)), `${retiredPath} must stay deleted`);
}

for (const file of [
  "scripts/run-routing-operational-soak.ps1",
  "docs/audits/routing-operational-qualification-manifest.json",
  "docs/audits/intelligent-routing-acceptance-matrix.md",
]) {
  assert.doesNotMatch(
    read(file),
    /intelligent_routing_coordinator/u,
    `${file} must use the V3 execution ownership suite`,
  );
}

const retiredSurfacePattern =
  /list_error_rate_history|listErrorRateHistory|(?:get|upsert|clear)_station_capacity_domain|(?:get|upsert|clear)StationCapacityDomain|list_station_key_health|listStationKeyHealth|get_station_key_health|getStationKeyHealth|get_station_key_operational_detail|getStationKeyOperationalDetail/u;
for (const file of [
  "src-tauri/src/ipc/registry.rs",
  "src-tauri/permissions/main-window.toml",
  "src-tauri/generated/command-registry.json",
  "src/lib/bridge/generated.ts",
  "src/lib/bridge/BackendClient.ts",
  "src/lib/bridge/DesktopBackend.ts",
  "src/lib/bridge/DemoBackend.ts",
]) {
  assert.doesNotMatch(read(file), retiredSurfacePattern, `${file} exposes a retired routing surface`);
}

const planningSnapshot = read(
  "src-tauri/src/application/operational_facts/planning_snapshot.rs",
);
const verdictStore = read(
  "src-tauri/src/persistence/stores/routing_health_verdict_store.rs",
);
assert.match(planningSnapshot, /load_unsupported_model_batch/u);
assert.match(verdictStore, /apply_unsupported_model/u);
assert.match(verdictStore, /load_unsupported_model_batch/u);

const routingStore = read("src-tauri/src/persistence/stores/routing_store.rs");
assert.match(routingStore, /endpoint_health_snapshot/u, "endpoint health owner must be retained");
const observationStore = read(
  "src-tauri/src/persistence/stores/routing_observation_store.rs",
);
assert.match(observationStore, /probe_state_revision/u, "historical evidence decoder must be retained");
assert.doesNotMatch(
  observationStore,
  /pub\(crate\)?\s+async\s+fn\s+(?:list_after|list_for_scope|list_for_scopes)\s*\(/u,
  "unversioned routing observation readers must stay deleted",
);

const routingPolicyStore = read("src-tauri/src/persistence/stores/routing_policy_store.rs");
assert.doesNotMatch(routingPolicyStore, /save_compare_and_swap|validate_policy_input/u);

assert.doesNotMatch(
  applicationRouting,
  /load_health_protection_statuses|begin_health_protection_probe|cancel_health_protection_probe/u,
  "empty legacy health/probe facades must stay deleted",
);
assert.doesNotMatch(
  applicationRouting,
  /load_intelligent_planning_snapshot_with_probe|HealthProbeAdmissionMode/u,
  "production routing must not carry the retired health-probe planning parameter chain",
);
assert.doesNotMatch(
  healthProtection,
  /HealthProbeAdmissionMode|HealthProtectionProbe|HealthProtectionReducer|HealthProtectionSnapshotV1|HealthProtectionStatus/u,
  "the retired scoped-health reducer and probe tokens must stay deleted",
);
assert.match(healthProtection, /struct HealthProtectionProfileV1/u);
assert.match(healthProtection, /struct HealthProtectionScope/u);

const runtimeRouting = read("src-tauri/src/models/routing.rs");
const runtimeSettings = runtimeRouting.match(/pub struct RuntimeRoutingSettings\s*\{[\s\S]*?\n\}/u);
assert.ok(runtimeSettings, "RuntimeRoutingSettings must remain defined");
assert.doesNotMatch(runtimeSettings[0], /\bpolicy\b|scheduler_config/u);
assert.doesNotMatch(
  runtimeRouting,
  /AutomaticSchedulerSettings|DispatchAlgorithmSettings|SchedulerConfigError/u,
  "test-only scheduler compatibility types must stay deleted",
);

const admission = read("src-tauri/src/application/routing_engine/admission.rs");
const admissionInput = admission.match(/pub struct AdmissionPlanningInput<'a>\s*\{[\s\S]*?\n\}/u);
assert.ok(admissionInput, "AdmissionPlanningInput must remain defined");
assert.match(
  admissionInput[0],
  /planning_snapshot:\s*&'a\s+PlanningSnapshot/u,
  "admission must require the planning snapshot at the type boundary",
);
assert.doesNotMatch(admissionInput[0], /planning_snapshot:\s*Option\s*</u);
assert.doesNotMatch(
  admission,
  /planning_snapshot_required/u,
  "the unreachable missing-planning-snapshot fallback must stay deleted",
);

const candidatePlan = read("src-tauri/src/application/routing_engine/candidate_plan.rs");
assert.doesNotMatch(
  candidatePlan,
  /RoutePlanStratum|HIERARCHICAL_ROUTE_PLANNER_VERSION|MAX_ROUTE_PLAN_CANDIDATES|build_route_plan/u,
  "test-only hierarchical planner must stay deleted",
);

const routingPolicy = read("src-tauri/src/application/routing_policy.rs");
const attemptBudget = routingPolicy.match(/pub\(crate\) struct AttemptBudgetProfileV1\s*\{[\s\S]*?\n\}/u);
assert.ok(attemptBudget, "AttemptBudgetProfileV1 must remain defined");
assert.doesNotMatch(
  attemptBudget[0],
  /allow_cross_capacity_domain_fallback/u,
  "the V3 attempt budget must not retain the capacity-domain runtime shadow",
);
const routingPolicyModel = read("src-tauri/src/models/routing_policy.rs");
const routingMutationDto = read("src-tauri/src/ipc/dto/routing_mutations.rs");
assert.match(
  routingPolicyModel,
  /allow_cross_capacity_domain_fallback/u,
  "the V1/V2 policy compatibility decoder must remain readable",
);
assert.match(
  routingMutationDto,
  /allow_cross_capacity_domain_fallback/u,
  "the V1/V2 policy compatibility DTO must remain readable",
);

const routingEngineProduction = readTree("src-tauri/src/application/routing_engine", ".rs")
  .map(({ relativePath, source }) => [relativePath, stripRustTests(source)]);
for (const [file, source] of routingEngineProduction) {
  assert.doesNotMatch(
    source,
    /FailureTarget::ProviderCapacity|ProviderCapacityDomain|CapacityDomainCommitment/u,
    `${file} must not restore the runtime capacity-domain target shadow`,
  );
}
assert.doesNotMatch(
  stripRustTests(read("src-tauri/src/services/proxy/error.rs")),
  /ProviderCapacityDomain|CapacityDomainCommitment|FailureTarget::ProviderCapacity/u,
  "proxy error handling must classify provider capacity at the current-key V3 boundary",
);

for (const file of [
  "src-tauri/src/application/operational_facts/planning_snapshot.rs",
  "src-tauri/src/application/queries/routing_workspace.rs",
  "src-tauri/src/ipc/dto/routing_health_reads.rs",
  "src-tauri/src/ipc/dto/routing_health_reads.typescript.txt",
  "src/features/routing/LocalRoutingStatusCandidateRow.tsx",
  "src/features/routing/localRoutingStatusViewModel.ts",
  "src/lib/bridge/generated.ts",
]) {
  assert.doesNotMatch(
    read(file),
    /ProbeDiscoveryOnly|ProbeDiscovery|probe_discovery|error_rate_probe_discovery/u,
    `${file} must not expose the retired probe-discovery planning state`,
  );
}

const rawOperationalFacts = read("src-tauri/src/models/operational/raw_facts.rs");
const rawCandidate = rawOperationalFacts.match(/pub\(crate\) struct RawOperationalCandidateRow\s*\{[\s\S]*?\n\}/u);
assert.ok(rawCandidate, "RawOperationalCandidateRow must remain defined");
assert.doesNotMatch(
  rawCandidate[0],
  /success_count|failure_count|consecutive_failures|avg_latency_ms|last_error_summary|cooldown_until/u,
  "operational facts must not carry the retired health snapshot shadow",
);
assert.doesNotMatch(
  stripRustTests(read("src-tauri/src/persistence/stores/operational_facts/queries.rs")),
  /success_count|failure_count|consecutive_failures|avg_latency_ms|last_error_summary|cooldown_until|routing_health_snapshot/u,
  "operational fact queries must not read the retired health snapshot",
);
const operationalAssembler = stripRustTests(
  read("src-tauri/src/application/operational_facts/assembler.rs"),
).replace(
  /#\[cfg\(test\)\]\s*(?:last_error_summary|cooldown_until):[^\n]*\n/gu,
  "",
);
assert.doesNotMatch(
  operationalAssembler,
  /success_count|failure_count|consecutive_failures|avg_latency_ms|last_error_summary|cooldown_until|routing_health_snapshot/u,
  "production operational fact assembly must not restore the retired health shadow",
);
const canonicalCandidate = runtimeRouting.match(
  /pub struct CanonicalRoutingCandidate\s*\{[\s\S]*?\n\}/u,
);
assert.ok(canonicalCandidate, "CanonicalRoutingCandidate must remain defined");
assert.match(
  canonicalCandidate[0],
  /#\[cfg\(test\)\]\s*pub health:\s*Option<StationKeyHealth>/u,
  "the canonical candidate health fixture must stay bounded to tests",
);

assert.match(
  read("src-tauri/src/ipc/registry.rs"),
  /get_routing_protection_status/u,
  "the V3-backed protection compatibility command must remain through P7",
);

const legacySchemaObjects =
  /\b(?:routing_health_snapshot|station_key_health_observations|routing_error_rate_history(?:_meta)?|station_capacity_domains)\b/u;
const allowedLegacySchemaReaders = [
  "src-tauri/src/persistence/legacy_import/",
  "src-tauri/src/persistence/stores/station_catalog.rs",
  "src-tauri/src/services/data_store/alerting_upgrade.rs",
  "src-tauri/src/services/portable_migration/",
];
for (const { relativePath, source } of readTree("src-tauri/src", ".rs")) {
  if (
    relativePath === "src-tauri/src/persistence/differential_tests.rs" ||
    relativePath.startsWith("src-tauri/src/test_support/")
  ) {
    continue;
  }
  const production = stripRustTests(source);
  if (!legacySchemaObjects.test(production)) continue;
  assert.ok(
    allowedLegacySchemaReaders.some((allowed) =>
      allowed.endsWith("/") ? relativePath.startsWith(allowed) : relativePath === allowed,
    ),
    `${relativePath} may reference retired schema only for migration/import/delete-cleanup/schema/portable compatibility`,
  );
}

const migration26 = read(
  "src-tauri/src/persistence/migrations/0026_intelligent_routing_cutover_cleanup.sql",
);
const portableCatalog = read("src-tauri/src/services/portable_migration/catalog.rs");
assert.match(migration26, /routing_health_snapshot/u);
assert.match(portableCatalog, /station_capacity_domains/u);

for (const migration of readdirSync(resolve("src-tauri/src/persistence/migrations"))) {
  if (!migration.endsWith(".sql")) continue;
  const version = Number.parseInt(migration.slice(0, 4), 10);
  if (!Number.isFinite(version) || version <= 71) continue;
  assert.doesNotMatch(
    read(`src-tauri/src/persistence/migrations/${migration}`),
    /DROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?(?:routing_health_snapshot|station_key_health_observations|station_capacity_domains)\b/iu,
    `${migration} cannot drop legacy routing tables before the recorded qualification gate`,
  );
}

console.log("routing V3 legacy retirement contract passed");

function resolve(relativePath) {
  return path.join(root, ...relativePath.split("/"));
}

function read(relativePath) {
  const absolute = resolve(relativePath);
  assert.ok(existsSync(absolute), `${relativePath} must exist`);
  return readFileSync(absolute, "utf8");
}

function readTree(relativeDirectory, extension) {
  const files = [];
  const visit = (relativePath) => {
    const absolute = resolve(relativePath);
    for (const entry of readdirSync(absolute, { withFileTypes: true })) {
      const child = `${relativePath}/${entry.name}`;
      if (entry.isDirectory()) {
        visit(child);
      } else if (entry.isFile() && entry.name.endsWith(extension)) {
        files.push({ relativePath: child, source: read(child) });
      }
    }
  };
  visit(relativeDirectory);
  return files;
}

function stripRustTests(source) {
  let result = source;
  const modulePattern = /#\[cfg\(test\)\]\s*mod\s+\w+\s*\{/gu;
  for (let match = modulePattern.exec(result); match; match = modulePattern.exec(result)) {
    const open = modulePattern.lastIndex - 1;
    const close = matchingBrace(result, open);
    if (close < 0) break;
    result = `${result.slice(0, match.index)}${result.slice(close + 1)}`;
    modulePattern.lastIndex = match.index;
  }
  return result;
}

function matchingBrace(source, open) {
  let depth = 0;
  for (let index = open; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return index;
  }
  return -1;
}
