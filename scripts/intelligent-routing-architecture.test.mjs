import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";

const args = process.argv.slice(2);
const rootIndex = args.indexOf("--root");
const root = path.resolve(rootIndex >= 0 ? args[rootIndex + 1] : process.cwd());
const fixtureMode = args.includes("--fixtures");

if (fixtureMode) {
  runFixtures();
  console.log("intelligent routing architecture fixtures passed");
  process.exit(0);
}

const manifest = readJson("docs/audits/intelligent-routing-boundary-manifest.json");
assert.equal(manifest.schema_version, 1, "boundary manifest schema version must be 1");
assert.deepEqual(
  manifest.temporary_allowed_exceptions,
  [],
  "intelligent-routing cutover must not retain a temporary production boundary",
);
for (const owner of manifest.required_target_owners) {
  assert.equal(typeof owner, "string");
  assert.notEqual(owner.length, 0);
}

checkPlannerContractBoundary();
checkObservationAndHealthOwnership();
checkManifestOwnersAndForbiddenEdges();

console.log("intelligent routing architecture manifest gate passed");

function checkPlannerContractBoundary() {
  const snapshotPlanner = "src-tauri/src/application/routing_engine/intelligent_planner.rs";
  const engineModule = "src-tauri/src/application/routing_engine/mod.rs";
  const admission = "src-tauri/src/application/routing_engine/admission.rs";
  const execution = "src-tauri/src/services/proxy/execution.rs";
  const productionConsumers = [
    "src-tauri/src/application/routing_engine/admission.rs",
    "src-tauri/src/application/routing.rs",
  ];
  const snapshotSource = readSource(snapshotPlanner);
  const moduleSource = readSource(engineModule);
  assert.doesNotMatch(moduleSource, /planner_legacy|planner_contract_gate|\bcontroller\b|\bselector\b|routing_snapshot|routing_types/u);
  assert.match(snapshotSource, /fn\s+plan_snapshot\s*\(/u);
  assert.doesNotMatch(snapshotSource, /weighted_rendezvous/u);
  assert.doesNotMatch(snapshotSource, /exploration_share_basis_points/u);
  assert.match(snapshotSource, /planned\.sort_by[\s\S]*utility\.value\(\)\.cmp/u);
  const admissionSource = readSource(admission);
  const executionSource = readSource(execution);
  const planningSnapshotSource = readSource(
    "src-tauri/src/application/operational_facts/planning_snapshot.rs",
  );
  const operationalQuerySource = readSource(
    "src-tauri/src/persistence/stores/operational_facts/queries.rs",
  );
  const tiersSource = readSource("src-tauri/src/application/routing_engine/tiers.rs");
  const factorsSource = readSource("src-tauri/src/application/routing_engine/factors.rs");
  assert.match(
    admissionSource,
    /pub\s+fn\s+next[\s\S]*?plan_snapshot\s*\(/u,
    "production admission must invoke the canonical intelligent planner",
  );
  for (const count of [
    "configured_key_count",
    "capability_match_count",
    "candidate_cap_count",
  ]) {
    assert.match(
      planningSnapshotSource,
      new RegExp(`\\b${count}\\b`, "u"),
      `planning snapshot must capture ${count}`,
    );
  }
  assert.match(
    planningSnapshotSource,
    /candidate_cap_count\s*>\s*options\.candidate_limit\(\)/u,
    "candidate cap must be checked after request-specific planning evaluation",
  );
  assert.doesNotMatch(
    operationalQuerySource,
    /candidate_query_limit|LIMIT\s+\?1/u,
    "operational facts must not apply the candidate cap before capability filtering",
  );
  assert.match(
    tiersSource,
    /Primary[\s\S]*ConfiguredBackup[\s\S]*DepletedEmergency/u,
    "production planning must preserve all three availability tiers",
  );
  assert.doesNotMatch(
    factorsSource,
    /unwrap_or(?:_else)?\([^)]*5_000/u,
    "missing cost must remain unavailable rather than becoming neutral evidence",
  );
  assert.match(
    admissionSource,
    /attempt_count[\s\S]*max_attempts/u,
    "production admission must enforce one request-global attempt budget",
  );
  assert.match(
    admissionSource,
    /planning_snapshot_required/u,
    "production admission must fail closed without a planning snapshot",
  );
  assert.doesNotMatch(
    executionSource,
    /planner_order|plan_route_from_order|build_route_plan_from_order/u,
    "proxy execution must not bridge the intelligent planner into a legacy order",
  );
  for (const consumer of productionConsumers) {
    const source = readSource(consumer);
    assert.doesNotMatch(source, /planner_legacy|RouteAdmissionController|selector::|routing_snapshot|routing_types/u, `${consumer} must not depend on deleted routing owners`);
  }
}

function checkObservationAndHealthOwnership() {
  const ingestion = readSource("src-tauri/src/application/observation_ingestion.rs");
  const transitions = readSource("src-tauri/src/application/health_transitions.rs");
  const healthStore = readSource("src-tauri/src/persistence/stores/health_observation_store.rs");
  assert.match(ingestion, /RoutingObservationStore/u, "canonical observation ingestion must own the observation store");
  assert.match(ingestion, /producer_sequence/u, "observation ordering must be explicit");
  assert.match(ingestion, /Sha256/u, "observation idempotency must use a payload hash");
  assert.doesNotMatch(transitions, /update_station_key_status/u, "health transitions must not write legacy status");
  assert.doesNotMatch(healthStore, /update_station_key_status/u, "health store must not expose a legacy status writer");
}

function checkManifestOwnersAndForbiddenEdges() {
  const owners = {
    PlanningSnapshotBuilder: "src-tauri/src/application/operational_facts/planning_snapshot.rs",
    RoutingPolicyConfigV1: "src-tauri/src/models/routing_policy.rs",
    RoutingObservation: "src-tauri/src/models/routing_observation.rs",
    DispatchAlgorithmProfile: "src-tauri/src/application/routing_engine/algorithm_profile.rs",
    RouteAdmissionCoordinator: "src-tauri/src/application/routing_engine/admission.rs",
    RoutingWorkspaceReadModel: "src-tauri/src/application/queries/routing_workspace.rs",
    DomainRevisionNotice: "src-tauri/src/application/queries/read_model_revision.rs",
  };
  for (const owner of manifest.required_target_owners) {
    const file = owners[owner];
    assert.ok(file, `manifest owner ${owner} must have an explicit source mapping`);
    assert.ok(readSource(file).length > 0, `${owner} source must exist`);
  }
  const planner = readSource("src-tauri/src/application/routing_engine/intelligent_planner.rs");
  for (const forbidden of manifest.forbidden_production_dependencies.planner) {
    const pattern = new RegExp(`\\b${escapeRegExp(forbidden).replaceAll("\\ ", "\\\\s+")}\\b`, "iu");
    assert.doesNotMatch(planner, pattern, `planner must not depend on ${forbidden}`);
  }
  assert.match(readSource("src-tauri/src/application/routing_engine/admission.rs"), /plan_snapshot\s*\(/u);
  const routingApplication = readSource("src-tauri/src/application/routing.rs");
  assert.match(routingApplication, /load_intelligent_planning_snapshot[\s\S]*plan_snapshot\s*\(/u);
  const simulationBody = routingApplication.split("pub(crate) async fn simulate_route", 1)[1] ?? "";
  assert.doesNotMatch(simulationBody, /load_(?:runtime|workspace_projection)_candidates_with_request_pricing/u, "simulation must not load the read-model candidate projection chain");
  const executionRepository = readSource("src-tauri/src/services/proxy/routing_repository.rs")
    .split("\n#[cfg(test)]\nmod tests", 1)[0]
    .replace(/\s*#\[cfg\(test\)\]\s*pub\(crate\)\s+legacy_candidates\s*:[\s\S]*?,/u, "");
  assert.doesNotMatch(executionRepository, /RouteCandidateProjection|load_runtime_candidates_with_request_pricing/u, "production execution repository must not rebuild legacy candidate projections");
  assert.match(executionRepository, /OperationalRouteSnapshot[\s\S]*Vec<RoutePlanCandidate>/u, "execution snapshot must expose an execution-only candidate index");
  assert.doesNotMatch(readSource("src-tauri/src/application/queries/routing_workspace.rs"), /production_policy/u, "workspace read model must not expose legacy policy enum truth");
  assert.match(readSource("src-tauri/src/application/queries/routing_workspace.rs"), /policy_config: RoutingPolicyConfigV3/u, "workspace read model must expose the active canonical v3 policy config");
  assert.match(readSource("src/features/routing/LocalRoutingSettingsEditor.tsx"), /useRoutingPolicyDraft/u, "routing editor must use the shared draft/CAS owner");
  assert.doesNotMatch(readSource("src/features/routing/LocalRoutingSettingsEditor.tsx"), /schedulerAdvancedSettings|updateSettings/u);
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function runFixtures() {
  const fixtureRoot = path.join(root, "scripts", "fixtures", "intelligent-routing-architecture");
  checkFixture(path.join(fixtureRoot, "pass"), true);
  for (const name of readdirSync(fixtureRoot)) {
    if (!name.startsWith("red-")) continue;
    checkFixture(path.join(fixtureRoot, name), false);
  }
}

function checkFixture(fixtureRoot, shouldPass) {
  const sources = filesUnder(fixtureRoot).map((file) => readFileSync(file, "utf8")).join("\n");
  const failures = [];
  const reject = (pattern, message) => {
    if (pattern.test(sources)) failures.push(message);
  };
  reject(/\b(?:sqlx|reqwest|tauri|SecretManager|ipc::dto|request[_ ]log|monitoring[_ ]dto)\b/u, "planner imports an outer-layer dependency");
  reject(/\bRouteCandidateProjection\b|\bcandidates\s*:\s*&?\[/u, "planner accepts a legacy candidate slice");
  reject(/\b(?:deriveStationGroupDisplayFacts|derivePricingGroupDisplayCandidates|authoritative(?:Pricing|Group|Capability|Health|Score)Reducer)\b/u, "frontend owns routing truth");
  reject(/\b(?:begin_write|begin\s*write)\b/u, "application query opens a write transaction");
  reject(/(?:unwrap_or\(1\)|fallback\s*=\s*1|CAST\(updated_at AS INTEGER\))/u, "timestamp or fallback revision remains");
  reject(/\brequireRegistration\(\s*old_symbol\s*\)|permanent[_ ]temporary/u, "legacy gate contains a permanent compatibility requirement");
  if (shouldPass) {
    assert.deepEqual(failures, [], `${fixtureRoot} should pass: ${failures.join(", ")}`);
  } else {
    assert.notEqual(failures.length, 0, `${fixtureRoot} must be rejected`);
  }
}

function readJson(relativePath) {
  const file = path.join(root, ...relativePath.split("/"));
  assert.ok(existsSync(file), `${relativePath} must exist`);
  return JSON.parse(readFileSync(file, "utf8"));
}

function readSource(relativePath) {
  return readFileSync(path.join(root, ...relativePath.split("/")), "utf8");
}

function filesUnder(directory) {
  const result = [];
  const pending = [directory];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const file = path.join(current, entry.name);
      if (entry.isDirectory()) pending.push(file);
      else if (entry.isFile()) result.push(file);
    }
  }
  return result;
}
