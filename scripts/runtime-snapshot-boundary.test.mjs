import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/lib/projections/runtimeSnapshot.ts", "utf8");

assert.ok(
  source.includes("buildRuntimeRouteSnapshot"),
  "runtime snapshot projection should expose a pure snapshot builder",
);
assert.ok(
  source.includes("buildCurrentStationGroupFacts"),
  "runtime snapshot projection should consume current group facts",
);
assert.ok(
  source.includes("buildCurrentStationBalanceFacts"),
  "runtime snapshot projection should consume current station balance facts",
);
assert.ok(
  source.includes("backend_read_model_required"),
  "runtime snapshot projection should not reconstruct pricing evidence in the frontend routing path",
);
assert.ok(
  !source.includes("buildPricingGroupCandidates"),
  "runtime snapshot projection must not consume the frontend pricing matcher in the routing path",
);
assert.ok(source.includes("secretRef"), "runtime snapshot should expose secret references only");
assert.ok(!source.includes("@/features/"), "runtime snapshot projection must not import UI feature modules");
assert.ok(!source.includes("@/lib/api/"), "runtime snapshot projection must not call API/query modules");
assert.ok(!source.includes("@tauri-apps/api"), "runtime snapshot projection must not call Tauri directly");
assert.ok(!source.includes("apiKey:"), "runtime snapshot candidate shape must not expose plaintext key fields");
assert.ok(!source.includes(".apiKey"), "runtime snapshot projection must not read plaintext key fields");
assert.ok(!source.includes("listStation"), "runtime snapshot projection must not load raw data by itself");
assert.ok(!source.includes("invoke<"), "runtime snapshot projection must stay pure and not invoke Tauri");
