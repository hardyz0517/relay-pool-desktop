import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const querySource = await readFile("src/lib/queries/localRoutingQueries.ts", "utf8");
const resourceSource = await readFile("src/lib/query/resourceQueries.ts", "utf8");
const pageSource = await readFile("src/features/routing/RoutingPage.tsx", "utf8");

assert.ok(
  querySource.includes('import { loadLocalRoutingWorkspaceApi } from "@/lib/api/localRouting";') &&
    querySource.includes('import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";'),
  "local routing query service should use the typed local routing API boundary",
);

assert.ok(
  querySource.includes("export function loadLocalRoutingWorkspace(): Promise<LocalRoutingWorkspace>") &&
    querySource.includes("return loadLocalRoutingWorkspaceApi()"),
  "local routing query service should delegate workspace loading to the backend-owned API capability",
);

assert.ok(
  !querySource.includes("startLocalProxy") &&
    !querySource.includes("stopLocalProxy") &&
    !querySource.includes("updateLocalRoutingCandidate"),
  "local routing query service must not perform write actions",
);

assert.ok(
  resourceSource.includes('import { loadLocalRoutingWorkspace } from "@/lib/queries/localRoutingQueries";') &&
    resourceSource.includes("queryFn: loadLocalRoutingWorkspace") &&
    pageSource.includes("useActivityQuery(refreshEnabled, localRoutingWorkspaceQueryOptions())"),
  "routing page should consume the local routing query service through an activity-bound shared query",
);

assert.ok(
  !pageSource.includes("listLocalRoutingCandidates("),
  "routing page should not own local routing raw fact orchestration",
);
