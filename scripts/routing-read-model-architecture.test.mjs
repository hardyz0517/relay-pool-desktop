import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";

const root = process.cwd();
const failures = [];

const requiredBackendModules = [
  "src-tauri/src/application/queries/routing_workspace.rs",
  "src-tauri/src/application/queries/routing_runtime.rs",
  "src-tauri/src/application/queries/operational_detail.rs",
  "src-tauri/src/application/queries/request_decision_trace.rs",
];

for (const file of requiredBackendModules) {
  assert.ok(existsSync(path.join(root, file)), `${file} must exist`);
}

checkSingleFrontendRoutingQueryOwner();
checkCommandsAreRegistered();
checkRuntimeOverlayIsLowCardinality();
checkPreviewCapacityCannotLookAcquired();
checkLocalWorkspaceIsCompatibilityOnly();

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log("routing read-model architecture gate passed");

function read(relativePath) {
  return readFileSync(path.join(root, relativePath), "utf8");
}

function filesUnder(relativeDir, extensions) {
  const dir = path.join(root, relativeDir);
  const result = [];
  const pending = [dir];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        pending.push(full);
      } else if (extensions.some((extension) => entry.name.endsWith(extension))) {
        result.push(full);
      }
    }
  }
  return result;
}

function normalize(file) {
  return path.relative(root, file).replaceAll("\\", "/");
}

function checkSingleFrontendRoutingQueryOwner() {
  const queryFiles = filesUnder("src/lib/queries", [".ts", ".tsx"])
    .map(normalize)
    .filter((file) => /routing/i.test(path.basename(file)) && !file.endsWith(".test.ts"));
  assert.ok(
    queryFiles.includes("src/lib/queries/routingQueries.ts"),
    "routingQueries.ts must be the routing query owner",
  );
  assert.ok(
    !queryFiles.includes("src/lib/queries/routingOperationalQueries.ts"),
    "do not introduce a second routingOperationalQueries owner",
  );

  const owner = read("src/lib/queries/routingQueries.ts");
  for (const symbol of [
    "loadRoutingWorkspaceSnapshotQuery",
    "loadRoutingRuntimeOverlayQuery",
    "listRecentRouteDecisionsQuery",
    "getStationKeyOperationalDetailQuery",
    "getRequestDecisionTraceQuery",
    "simulateRouteQuery",
  ]) {
    assert.match(owner, new RegExp(`\\b${symbol}\\b`, "u"), `routingQueries.ts must export ${symbol}`);
  }
}

function checkCommandsAreRegistered() {
  const registry = read("src-tauri/src/ipc/registry.rs");
  for (const command of [
    "load_routing_workspace_snapshot",
    "load_routing_runtime_overlay",
    "list_recent_route_decisions",
    "get_station_key_operational_detail",
    "get_request_decision_trace",
    "simulate_route",
  ]) {
    assert.match(registry, new RegExp(`\\b${command}\\b`, "u"), `${command} must be registered`);
    assert.match(read("src/lib/bridge/generated.ts"), new RegExp(`"${command}"`, "u"), `${command} must have generated binding`);
  }
}

function checkRuntimeOverlayIsLowCardinality() {
  const source = read("src-tauri/src/application/queries/routing_runtime.rs");
  for (const forbidden of ["api_key", "apiKey", "upstream_base_url", "upstreamBaseUrl", "price", "pricing", "history"]) {
    if (source.includes(forbidden)) {
      failures.push(`routing_runtime.rs: runtime overlay must not include ${forbidden}`);
    }
  }
}

function checkPreviewCapacityCannotLookAcquired() {
  const workspace = read("src-tauri/src/application/queries/routing_workspace.rs");
  const routing = read("src-tauri/src/application/routing.rs");
  assert.match(workspace, /RoutingCapacityReadMode::SnapshotOnly/u, "workspace preview must use snapshot-only capacity");
  assert.match(workspace, /acquired:\s*false/u, "workspace read model must not acquire capacity");
  assert.match(routing, /selected_capacity_acquired:\s*false/u, "simulate_route preview metadata must not acquire capacity");
  assert.match(routing, /capacity_mode:\s*"snapshot_only"/u, "simulate_route must expose snapshot-only capacity metadata");
}

function checkLocalWorkspaceIsCompatibilityOnly() {
  const routingQueries = read("src/lib/queries/routingQueries.ts");
  assert.doesNotMatch(
    routingQueries,
    /\bloadLocalRoutingWorkspace\b/u,
    "new routing query owner must not depend on legacy local routing workspace",
  );
}
