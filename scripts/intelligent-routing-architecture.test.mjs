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

const manifest = readJson("docs/superpowers/audits/intelligent-routing-boundary-manifest.json");
assert.equal(manifest.schema_version, 1, "boundary manifest schema version must be 1");
assert.deepEqual(
  manifest.temporary_allowed_exceptions.map((entry) => entry.id),
  ["intelligent_routing_qualification"],
  "qualification must be the only temporary intelligent-routing boundary",
);
assert.equal(manifest.temporary_allowed_exceptions[0].delete_by_task, 17);
assert.equal(manifest.temporary_allowed_exceptions[0].production_reachable, false);
for (const owner of manifest.required_target_owners) {
  assert.equal(typeof owner, "string");
  assert.notEqual(owner.length, 0);
}

checkPlannerContractBoundary();

console.log("intelligent routing architecture manifest gate passed");

function checkPlannerContractBoundary() {
  const legacyPlanner = "src-tauri/src/application/routing_engine/planner_legacy.rs";
  const removedPlanner = "src-tauri/src/application/routing_engine/planner.rs";
  const snapshotPlanner = "src-tauri/src/application/routing_engine/intelligent_planner.rs";
  const engineModule = "src-tauri/src/application/routing_engine/mod.rs";
  const compileGate = "src-tauri/src/application/routing_engine/planner_contract_gate.rs";
  const productionConsumers = [
    "src-tauri/src/application/routing_engine/controller.rs",
    "src-tauri/src/application/routing.rs",
  ];

  assert.ok(existsSync(path.join(root, ...legacyPlanner.split("/"))), `${legacyPlanner} must own the legacy route contract`);
  assert.ok(!existsSync(path.join(root, ...removedPlanner.split("/"))), `${removedPlanner} must not remain after the legacy rename`);

  const legacySource = readSource(legacyPlanner);
  const snapshotSource = readSource(snapshotPlanner);
  const moduleSource = readSource(engineModule);
  const compileGateSource = readSource(compileGate);

  assert.match(moduleSource, /\bmod\s+planner_legacy\s*;/u, "routing engine must expose the legacy planner under an explicit legacy name");
  assert.doesNotMatch(moduleSource, /\bmod\s+planner\s*;/u, "routing engine must not expose an ambiguous planner module");
  assert.match(legacySource, /\bfn\s+plan_route\s*\(/u, "only the legacy planner may retain the plan_route entrypoint");
  assert.doesNotMatch(snapshotSource, /\bplan_route\b/u, "the snapshot planner must not import or export the legacy plan_route contract");
  assert.doesNotMatch(legacySource, /\b(?:intelligent_planner|planning_snapshot)\b/u, "the legacy planner must not import the snapshot planner contract");
  assert.match(compileGateSource, /type\s+LegacyPlanRouteContract\b/u, "a Rust compile gate must pin the legacy contract");
  assert.match(compileGateSource, /type\s+IntelligentPlanSnapshotContract\b/u, "a Rust compile gate must pin the snapshot contract");
  assert.match(compileGateSource, /planner_legacy::plan_route/u, "the compile gate must type-check the legacy entrypoint");
  assert.match(compileGateSource, /intelligent_planner::plan_snapshot/u, "the compile gate must type-check the snapshot entrypoint");
  for (const consumer of productionConsumers) {
    const source = readSource(consumer);
    assert.match(source, /\bplanner_legacy::/u, `${consumer} must explicitly import the legacy planner during qualification`);
    assert.doesNotMatch(source, /\bplanner::/u, `${consumer} must not import an ambiguous planner module`);
  }
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
  reject(/\b(?:buildCurrentStationGroupFacts|buildPricingGroupCandidates|authoritative(?:Pricing|Group|Capability|Health|Score)Reducer)\b/u, "frontend owns routing truth");
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
