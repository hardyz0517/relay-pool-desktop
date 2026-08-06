import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const [pageSource, querySource, resourceSource, syncSource] = await Promise.all([
  readFile("src/features/routing/RoutingPage.tsx", "utf8"),
  readFile("src/lib/queries/routingQueries.ts", "utf8"),
  readFile("src/lib/query/resourceQueries.ts", "utf8"),
  readFile("src/lib/query/routingQuerySynchronization.ts", "utf8"),
]);

assert.match(querySource, /loadRoutingWorkspaceSnapshotQuery/u);
assert.match(querySource, /loadRoutingRuntimeOverlayQuery/u);
assert.match(pageSource, /loadRoutingWorkspaceSnapshotQuery/u);
assert.match(pageSource, /loadRoutingRuntimeOverlayQuery/u);
assert.match(pageSource, /proxyStatusQueryOptions/u);
assert.doesNotMatch(pageSource, /localRoutingWorkspaceQueryOptions|loadLocalRoutingWorkspace/u);
assert.doesNotMatch(resourceSource, /localRoutingWorkspace|loadLocalRoutingWorkspace/u);
assert.doesNotMatch(syncSource, /localRoutingWorkspace|localWorkspace/u);
assert.doesNotMatch(pageSource, /listLocalRoutingCandidates\(/u);

console.log("routing query service architecture gate passed");
