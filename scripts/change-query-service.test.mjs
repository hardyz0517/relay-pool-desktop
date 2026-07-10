import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const querySource = await readFile("src/lib/queries/changeQueries.ts", "utf8");
const pageSource = await readFile("src/features/changes/ChangeCenterPage.tsx", "utf8");

assert.ok(
  querySource.includes("export type ChangeCenterWorkspace") &&
    querySource.includes("changeEvents: ChangeEvent[]") &&
    querySource.includes("stations: Station[]"),
  "change query service should expose a raw facts workspace shape",
);

assert.ok(
  querySource.includes("export async function loadChangeCenterWorkspace()") &&
    querySource.includes("listChangeEvents()") &&
    querySource.includes("listStations()"),
  "change query service should orchestrate existing raw fact reads",
);

assert.ok(
  !querySource.includes("filterChangeEvents") &&
    !querySource.includes("unreadRiskCount") &&
    !querySource.includes("paginateChangeEvents") &&
    !querySource.includes("markUnreadChangeEventsRead"),
  "change query service must not define change center view-model behavior",
);

assert.ok(
  pageSource.includes('import { loadChangeCenterWorkspace } from "@/lib/queries/changeQueries";') &&
    pageSource.includes("const workspace = await loadChangeCenterWorkspace()") &&
    pageSource.includes("setStationNamesById(new Map(workspace.stations.map((station) => [station.id, station.name])))") &&
    pageSource.includes("markUnreadChangeEventsRead(workspace.changeEvents, markChangeEventRead)") &&
    pageSource.includes("setEvents(readOnEntryResult.events)"),
  "change center page should consume the query service without changing existing state assignments",
);

assert.ok(
  !/Promise\.all\(\[\s*listChangeEvents\(\),\s*listStations\(\),?\s*\]\)/s.test(pageSource),
  "change center page should no longer own raw fact Promise.all orchestration",
);
