import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const querySource = readFileSync("src/lib/queries/localRoutingQueries.ts", "utf8");
const apiSource = readFileSync("src/lib/api/localRouting.ts", "utf8");

assert.match(querySource, /loadLocalRoutingWorkspace/);
assert.match(querySource, /loadLocalRoutingWorkspaceApi/);
assert.equal(querySource.includes("@tauri-apps/api/core"), false, "query layer must not invoke Tauri directly");
assert.match(apiSource, /invoke<LocalRoutingWorkspace>\("load_local_routing_workspace"\)/);
assert.match(apiSource, /isInvokeUnavailable/);

console.log("local routing query boundary ok");
