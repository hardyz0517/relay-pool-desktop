import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const hostSource = await readFile("src/app/ShellPageHost.tsx", "utf8");
const activityQuerySource = await readFile("src/lib/query/useActivityQuery.ts", "utf8").catch(() => "");
const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const stationsSource = await readFile("src/features/stations/StationsPage.tsx", "utf8");
const stationsControllerSource = await readFile("src/features/stations/useStationsPageController.ts", "utf8");
const keyPoolControllerSource = await readFile("src/features/key-pool/useKeyPoolPageController.ts", "utf8");

assert.ok(
  hostSource.includes("PageVisibilityProvider") &&
    hostSource.includes("shellPageVisibilityForState(state)") &&
    hostSource.includes("<PageVisibilityProvider visibility={visibility}>") &&
    hostSource.includes("getPageRetentionDecision({") &&
    hostSource.includes('return "background";') &&
    hostSource.includes('return "active";'),
  "shell host should derive page activity from canonical PageVisibility and retention policy",
);

const retentionPolicySource = await readFile("src/app/navigation/pageRetentionPolicy.ts", "utf8");
assert.ok(
  retentionPolicySource.includes("export const MAX_RETAINED_SHELL_PAGES = 2") &&
    retentionPolicySource.includes('reason: "default-unmounted"') &&
    !retentionPolicySource.includes("legacy-allowlist") &&
    !retentionPolicySource.includes("retainedDuringStage3Migration"),
  "page retention should default to active plus transition pages without the Stage 3 legacy allowlist",
);

assert.ok(
  dashboardSource.includes("useActivityQuery") &&
    stationsControllerSource.includes("useActivityQuery") &&
    stationsControllerSource.includes("stationAssetsQueryOptions(stationIds)") &&
    !stationsSource.includes("useQueries({") &&
    !dashboardSource.includes("usePageActivity") &&
    !stationsSource.includes("usePageActivity"),
  "query-heavy pages should use activity-bound queries without the legacy Stations per-row query blocker",
);

assert.ok(
  !(await readFile("src/components/shell/PageActivity.tsx", "utf8").then(
    () => true,
    () => false,
  )),
  "page activity compatibility module should be deleted once page reads move to query owners",
);

assert.ok(
  activityQuerySource.includes("enabled: queryEnabled") &&
    activityQuerySource.includes("subscribed: active"),
  "inactive query consumers should disable both query execution and subscription",
);

const pages = [
  "src/features/dashboard/DashboardPage.tsx",
  "src/features/stations/useStationsPageController.ts",
  "src/features/key-pool/useKeyPoolPageController.ts",
  "src/features/routing/RoutingPage.tsx",
  "src/features/pricing/PricingPage.tsx",
  "src/features/channels/ChannelStatusTab.tsx",
  "src/features/channels/ChannelMonitoringTab.tsx",
  "src/features/collectors/CollectorsPage.tsx",
  "src/features/changes/ChangeCenterPage.tsx",
  "src/features/logs/LogsPage.tsx",
  "src/features/settings/SettingsPage.tsx",
];

const refreshOnlyPages = [
  "src/features/dashboard/DashboardPage.tsx",
  "src/features/stations/useStationsPageController.ts",
  "src/features/key-pool/useKeyPoolPageController.ts",
  "src/features/routing/RoutingPage.tsx",
  "src/features/pricing/PricingPage.tsx",
  "src/features/channels/ChannelStatusTab.tsx",
  "src/features/channels/ChannelMonitoringTab.tsx",
  "src/features/changes/ChangeCenterPage.tsx",
  "src/features/logs/LogsPage.tsx",
];
for (const page of refreshOnlyPages) {
  const source = await readFile(page, "utf8");
  assert.ok(
    source.includes("useActivityQuery") &&
      !source.includes("usePageActivity"),
    `${page} should have an explicit activity-bound query owner without the combined activity object`,
  );
}

const collectorsSource = await readFile("src/features/collectors/CollectorsPage.tsx", "utf8");
assert.ok(
  collectorsSource.includes("useActivityQuery") &&
    !collectorsSource.includes("usePageActivation") &&
    !collectorsSource.includes("usePageActivity"),
  "collectors page should use activity-bound query owners instead of a local activation loader",
);

const settingsSource = await readFile("src/features/settings/SettingsPage.tsx", "utf8");
assert.ok(
  settingsSource.includes("useActivityQuery") &&
    !settingsSource.includes("usePageActivation") &&
    !settingsSource.includes("usePageActivity"),
  "settings page should use activity-bound query owners instead of a local activation loader",
);

assert.ok(
  keyPoolControllerSource.includes("useActivityQuery(keyPoolQueryOptions())") &&
    keyPoolControllerSource.includes("useActivityQuery(stationsQueryOptions())") &&
    keyPoolControllerSource.includes("useActivityQuery(channelMonitoringQueryOptions())") &&
    keyPoolControllerSource.includes("queryClient.invalidateQueries({ queryKey: queryKeys.channelMonitoring })") &&
    !keyPoolControllerSource.includes("usePageActivation") &&
    !keyPoolControllerSource.includes("refreshMonitorResources"),
  "key pool should read monitor resources through canonical query owners instead of an activation loader",
);

for (const page of pages) {
  const source = await readFile(page, "utf8");
  assert.ok(
    source.includes("usePageActivation") || source.includes("useActivityQuery"),
    `${page} should refresh or subscribe to persisted data only when the page becomes active`,
  );
}

const monitoringSource = await readFile("src/features/channels/ChannelMonitoringTab.tsx", "utf8");
assert.ok(
  monitoringSource.includes("useActivityQuery(channelMonitoringQueryOptions())") &&
    monitoringSource.includes("queryClient.invalidateQueries({ queryKey: queryKeys.channelMonitoring })") &&
    !monitoringSource.includes("usePageActivation"),
  "monitoring should use the channel monitoring query owner instead of an activation loader",
);

const channelStatusPageSource = await readFile("src/features/channels/ChannelStatusPage.tsx", "utf8");
const channelStatusSource = await readFile("src/features/channels/ChannelStatusTab.tsx", "utf8");
assert.ok(
  channelStatusPageSource.includes("queryClient.invalidateQueries({ queryKey: queryKeys.channelStatus })") &&
    !channelStatusPageSource.includes("statusRefreshToken") &&
    !channelStatusSource.includes("refreshToken") &&
    !channelStatusSource.includes("useEffect"),
  "channel status refresh should use query invalidation instead of a tab token",
);

const logsSource = await readFile("src/features/logs/LogsPage.tsx", "utf8");
assert.ok(
  /useActivityQuery\(\s*requestLogsQueryOptions\(/.test(logsSource) &&
    logsSource.includes("queryClient.invalidateQueries({ queryKey: queryKeys.requestLogs })") &&
    !logsSource.includes("usePageRefreshEnabled") &&
    !logsSource.includes("Promise.all("),
  "logs page should refresh through the canonical request log query owner without legacy refresh fan-out",
);

const changeCenterSource = await readFile("src/features/changes/ChangeCenterPage.tsx", "utf8");
assert.ok(
  changeCenterSource.includes("useActivityQuery") &&
    !changeCenterSource.includes("usePageActivation") &&
    !changeCenterSource.includes("markUnreadChangeEventsReadLocally"),
  "change center should subscribe to cached data without owning a duplicate entry read path",
);
