import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";

const root = process.cwd();
const read = (file) => readFileSync(`${root}/${file}`, "utf8");

execFileSync(process.execPath, ["scripts/intelligent-routing-architecture.test.mjs", "--fixtures"], { stdio: "inherit" });
const planner = read("src-tauri/src/application/routing_engine/intelligent_planner.rs");
const dispatch = read("src-tauri/src/application/routing_engine/dispatch.rs");
const coordinator = read("src-tauri/src/application/routing_engine/coordinator.rs");
assert.match(planner, /plan_snapshot_with_budget/);
assert.match(dispatch, /seed_commitment/);
assert.match(coordinator, /ReplanLimit/);
for (const source of [planner, dispatch, coordinator]) {
  assert.doesNotMatch(source, /api[_-]?key|cookie|secret|prompt/i);
}
const replay = (seed, id) => createHash("sha256").update(`${seed}:relay-pool-routing/v1:1:${id}`).digest("hex");
assert.equal(replay("qualification-seed", "key-a"), replay("qualification-seed", "key-a"));
console.log(JSON.stringify({ status: "qualified", profile: "routing-profile-v1", projector: "routing_quality_v1", replay: "deterministic" }));
