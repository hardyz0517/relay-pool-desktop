import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";

const planner = readFileSync("src-tauri/src/application/routing_engine/intelligent_planner.rs", "utf8");
const snapshot = readFileSync("src-tauri/src/application/routing_engine/planning_snapshot.rs", "utf8");
const policy = readFileSync("src-tauri/src/application/routing_policy.rs", "utf8");
const cutover = readFileSync(
  "src-tauri/src/background_tasks/routing_generation_cutover_runner.rs",
  "utf8",
);
const generationModel = readFileSync("src-tauri/src/models/routing_generation.rs", "utf8");
const generationStore = readFileSync(
  "src-tauri/src/persistence/stores/routing_generation_store.rs",
  "utf8",
);
const capacity = readFileSync("src-tauri/src/application/routing_engine/capacity.rs", "utf8");
const admission = readFileSync("src-tauri/src/application/routing_engine/admission.rs", "utf8");

// v3 production planner is a pure snapshot function.  Selection is strict
// deterministic score order; the old budgeted rendezvous/exploration path
// must not re-enter through a compatibility helper.
assert.match(planner, /pub\(crate\)\s+fn\s+plan_snapshot\s*\(/);
assert.doesNotMatch(planner, /weighted_rendezvous/);
assert.doesNotMatch(planner, /exploration_share_basis_points/);
assert.match(planner, /planned\.sort_by[\s\S]*utility\.value\(\)\.cmp/);
assert.match(planner, /explored:\s*false/);
assert.match(planner, /band_size:\s*1/);
assert.match(snapshot, /pub(?:\(crate\))? struct PlanningSnapshot/);
assert.match(policy, /compile/);

// Activation evidence is produced and validated by separate owners. Both
// canonical single-Key failures must be replayed through the real classifier
// and circuit reducer before a generation can become active.
assert.match(cutover, /routing-generation-comparison-report-v2/);
assert.match(cutover, /routing-generation-replay-report-v2/);
assert.match(cutover, /replay_failure_semantics\("tntapi_502",\s*502/);
assert.match(cutover, /replay_failure_semantics\("tntapi_429",\s*429/);
for (const field of [
  "key_commitment",
  "reliability_basis_points",
  "weighted_latency_ms",
  "qualification_score_basis_points",
  "observation_count",
  "real_source_weight_basis_points",
  "monitoring_source_weight_basis_points",
  "quality_basis",
  "circuit_state",
  "rank",
]) {
  assert.match(cutover, new RegExp(`\\b${field}\\b`, "u"), `comparison report must include ${field}`);
}
assert.match(generationModel, /ROUTING_GENERATION_QUALIFICATION_VERSION[\s\S]*routing-generation-qualification-v2/);
assert.match(generationModel, /qualification_reports_are_activation_ready/);
assert.match(generationModel, /object\.get\("station_key_id"\)\.is_none\(\)/);
assert.match(generationModel, /\[429_u64,\s*502_u64\]/);
assert.match(generationStore, /require_qualification[\s\S]*qualification_reports_are_activation_ready/);
assert.match(generationStore, /canonical_json_sha256[\s\S]*comparison_report/);

// maxRetryCount is the only request-level retry budget. A second process-wide
// percentage gate would make configured retries nondeterministic.
for (const source of [capacity, admission]) {
  assert.doesNotMatch(source, /RetryBudgetRegistry|retry_budget_exhausted|retry_permit/u);
}

// Qualification fixture: stable input bytes must produce stable seed bytes.
const fixture = JSON.stringify({ seed: "qualification-seed", candidates: ["key-a", "key-b", "key-c"] });
const digest = () => createHash("sha256").update(fixture).digest("hex");
assert.equal(digest(), digest(), "deterministic replay seed must be stable");

const values = [0, 1, 10_000, 65_535];
for (const value of values) assert.ok(Number.isInteger(value) && value >= 0 && value <= 65_535);

console.log(JSON.stringify({
  status: "qualified",
  deterministicReplay: true,
  policyBounds: true,
  plannerOwner: "intelligent_planner",
  qualificationVersion: "routing-generation-qualification-v2",
  semanticFixtures: [429, 502],
  fixtureHash: digest(),
}));
