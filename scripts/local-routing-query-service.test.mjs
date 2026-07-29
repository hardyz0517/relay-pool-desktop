import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const querySource = readFileSync("src/lib/queries/localRoutingQueries.ts", "utf8");
const apiSource = readFileSync("src/lib/api/localRouting.ts", "utf8");

assert.match(querySource, /loadLocalRoutingWorkspace/);
assert.match(querySource, /loadLocalRoutingWorkspaceApi/);
assert.equal(querySource.includes("@tauri-apps/api/core"), false, "query layer must not invoke Tauri directly");
assert.equal(apiSource.includes("@tauri-apps/api/core"), false, "local routing API must route through the active backend facade");
assert.match(apiSource, /getActiveBackendClient\(\)\.localRouting\.loadLocalRoutingWorkspace\(\)/);
assert.match(apiSource, /getActiveBackendClient\(\)\.localRouting\.reorderLocalRoutingKeys\(input\)/);

console.log("local routing query boundary ok");
