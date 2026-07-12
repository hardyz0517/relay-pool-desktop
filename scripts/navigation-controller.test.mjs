import assert from "node:assert/strict";
import {
  commitNavigationIntent,
  createInitialNavigationIntent,
  createNavigationIntent,
} from "../src/app/navigationPolicy.ts";

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
};
assert.equal(commitNavigationIntent(committed, stations, 2), committed);
assert.equal(commitNavigationIntent(committed, logs, 2).activeRouteId, "logs");

const detail = createNavigationIntent("stationDetail", "stations", "stations", 3);
const edit = createNavigationIntent("editProvider", "stations", "stations", 4);
assert.equal(detail.transientParentRouteId, "stations");
assert.equal(edit.transientParentRouteId, "stations");
assert.equal(edit.shellRouteId, "stations");

console.log("navigation controller policy contract passed");
