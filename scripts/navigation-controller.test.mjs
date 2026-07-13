import assert from "node:assert/strict";
import * as navigationPolicy from "../src/app/navigationPolicy.ts";

const {
  commitNavigationIntent,
  createInitialNavigationIntent,
  createNavigationIntent,
} = navigationPolicy;

const initial = createInitialNavigationIntent("dashboard");
const stations = createNavigationIntent("stations", "stations", null, 1);
const logs = createNavigationIntent("logs", "logs", null, 2);

assert.equal(initial.shellRouteId, "dashboard");
assert.equal(stations.shellRouteId, "stations");
assert.equal(logs.sequence, 2);

const committed = {
  activeRouteId: "dashboard",
  previousRouteId: null,
  transientParentRouteId: null,
  sequence: 0,
};
assert.equal(commitNavigationIntent(committed, stations, 2), committed);
const committedLogs = commitNavigationIntent(committed, logs, 2);
assert.equal(committedLogs.activeRouteId, "logs");
assert.equal(committedLogs.sequence, 2);

const detail = createNavigationIntent("stationDetail", "stations", "stations", 3);
const edit = createNavigationIntent("editProvider", "stations", "stations", 4);
assert.equal(detail.transientParentRouteId, "stations");
assert.equal(edit.transientParentRouteId, "stations");
assert.equal(edit.shellRouteId, "stations");

assert.equal(
  typeof navigationPolicy.isLatestShellNavigationCompletion,
  "function",
  "shell completion should have a pure latest-intent guard",
);
assert.equal(
  navigationPolicy.isLatestShellNavigationCompletion(
    "stations",
    1,
    { shellRouteId: "logs", sequence: 2 },
    { sequence: 1 },
  ),
  false,
  "an entering intermediate route must not complete after a newer intent arrives",
);
assert.equal(
  navigationPolicy.isLatestShellNavigationCompletion(
    "logs",
    2,
    { shellRouteId: "logs", sequence: 2 },
    { sequence: 2 },
  ),
  true,
  "only the latest committed intent may complete and acquire refresh ownership",
);
assert.equal(
  typeof navigationPolicy.shouldNavigateToRoute,
  "function",
  "navigation should expose a pure duplicate-intent guard",
);
assert.equal(
  navigationPolicy.shouldNavigateToRoute(stations, "stations"),
  false,
  "reclicking an entering route should be a no-op instead of invalidating its sequence",
);
assert.equal(
  navigationPolicy.shouldNavigateToRoute(detail, "stations"),
  true,
  "clicking a transient page's parent shell should still navigate back",
);

console.log("navigation controller policy contract passed");
