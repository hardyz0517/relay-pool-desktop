import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const querySource = await readFile("src/lib/queries/routingQueries.ts", "utf8");
const pageSource = await readFile("src/features/routing/RoutingPage.tsx", "utf8");

assert.ok(
  querySource.includes("export type RoutingWorkspace") &&
    querySource.includes("settings: AppSettings") &&
    querySource.includes("modelAliases: ModelAlias[]"),
  "routing query service should expose a raw facts workspace shape",
);

assert.ok(
  querySource.includes("export async function loadRoutingWorkspace()") &&
    querySource.includes("getSettings()") &&
    querySource.includes("listModelAliases()"),
  "routing query service should orchestrate existing raw fact reads",
);

assert.ok(
  !querySource.includes("simulateRoute") &&
    !querySource.includes("upsertModelAlias") &&
    !querySource.includes("deleteModelAlias") &&
    !querySource.includes("updateSettings"),
  "routing query service must not define routing decisions or perform write actions",
);

assert.ok(
  pageSource.includes('import { loadRoutingWorkspace } from "@/lib/queries/routingQueries";') &&
    pageSource.includes("const workspace = await loadRoutingWorkspace()") &&
    pageSource.includes("setSettings(workspace.settings)") &&
    pageSource.includes("setAliases(workspace.modelAliases)"),
  "routing page should consume the query service without changing existing state assignments",
);

assert.ok(
  !/Promise\.all\(\[\s*getSettings\(\),\s*listModelAliases\(\),?\s*\]\)/s.test(pageSource),
  "routing page should no longer own the initial raw fact Promise.all orchestration",
);
