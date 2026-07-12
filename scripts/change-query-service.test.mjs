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
  pageSource.includes("changeEventsQueryOptions") &&
    pageSource.includes("stationsQueryOptions") &&
    pageSource.includes("queryClient.setQueryData(queryKeys.changeEvents") &&
    !pageSource.includes("loadChangeCenterWorkspace"),
  "change center page should consume shared resource query options instead of the legacy workspace loader",
);

assert.ok(
  !/Promise\.all\(\[\s*listChangeEvents\(\),\s*listStations\(\),?\s*\]\)/s.test(pageSource),
  "change center page should no longer own raw fact Promise.all orchestration",
);
