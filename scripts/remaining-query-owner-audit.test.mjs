import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const changeCenterSource = await readFile("src/features/changes/ChangeCenterPage.tsx", "utf8");
const pricingPageSource = await readFile("src/features/pricing/PricingPage.tsx", "utf8");
const pricingQueriesSource = await readFile("src/lib/queries/pricingQueries.ts", "utf8");
const resourceQueriesSource = await readFile("src/lib/query/resourceQueries.ts", "utf8");

assert.ok(
  changeCenterSource.includes("alertingActivityQueryOptions(") &&
    changeCenterSource.includes("useActivityQuery(stationsQueryOptions())") &&
    changeCenterSource.includes("queryClient.refetchQueries({ queryKey: queryKeys.stations, type: \"active\" })") &&
    changeCenterSource.includes("queryClient.refetchQueries({ queryKey: [\"alertingActivity\"], type: \"active\" })") &&
    !changeCenterSource.includes("usePageActivation") &&
    !changeCenterSource.includes("loadChangeCenterWorkspace") &&
    !changeCenterSource.includes("markUnreadChangeEventsReadLocally"),
  "ChangeCenterPage should use activity-bound canonical queries and query-cache mutation owners",
);

assert.ok(
  pricingPageSource.includes("useActivityQuery(") &&
    pricingPageSource.includes("pricingComparisonQueryOptions()") &&
    !pricingPageSource.includes("Promise.all(") &&
    !pricingPageSource.includes("usePageActivation") &&
    !pricingPageSource.includes("listStationGroupBindings") &&
    !pricingPageSource.includes("listGroupRateRecords"),
  "PricingPage should use one activity-bound pricing workspace query and no page-local server-state fan-out",
);

assert.ok(
  pricingQueriesSource.includes("getActiveBackendClient().pricing.loadPricingComparisonWorkspace()") &&
    resourceQueriesSource.includes("queryKey: queryKeys.pricing") &&
    resourceQueriesSource.includes("queryFn: loadPricingComparisonWorkspace"),
  "pricing comparison workspace should be the canonical backend-owned query source",
);

console.log("remaining query owner audit passed");
