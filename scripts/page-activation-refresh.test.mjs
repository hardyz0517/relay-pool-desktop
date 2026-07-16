import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const hostSource = await readFile("src/app/ShellPageHost.tsx", "utf8");
const activitySource = await readFile("src/components/shell/PageActivity.tsx", "utf8").catch(() => "");
const activityQuerySource = await readFile("src/lib/query/useActivityQuery.ts", "utf8").catch(() => "");
const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const stationsSource = await readFile("src/features/stations/StationsPage.tsx", "utf8");

assert.ok(
  hostSource.includes("PageActivityProvider") &&
    hostSource.includes('const interactive = state === "active" || state === "entering";') &&
    hostSource.includes("routeId === refreshRouteId") &&
    hostSource.includes('(shellPageState === "active" || shellPageState === "leaving")') &&
    /if \(transientActive\) \{\s*return "background";\s*\}[\s\S]*return "active";/.test(
      hostSource,
    ) &&
    hostSource.includes(
      "<PageActivityProvider active={interactive} refreshEnabled={refreshEnabled}>",
    ),
  "the outgoing page should retain its subscription through visual handoff while the entering page defers refresh",
);

assert.ok(
  activitySource.includes("refreshEnabled = active") &&
    activitySource.includes("() => ({ interactive: active, refreshEnabled })"),
  "page interaction and refresh permission should remain independent axes",
);

assert.ok(
  activitySource.includes("const PageRefreshContext = createContext(true)") &&
    activitySource.includes("export function usePageRefreshEnabled()") &&
    dashboardSource.includes("const refreshEnabled = usePageRefreshEnabled();") &&
    stationsSource.includes("const refreshEnabled = usePageRefreshEnabled();") &&
    !dashboardSource.includes("usePageActivity") &&
    !stationsSource.includes("usePageActivity"),
  "query-heavy pages should not rerender when only shell interaction state changes",
);

assert.ok(
  activitySource.includes("wasActiveRef") &&
    activitySource.includes("active && !wasActiveRef.current") &&
    activitySource.includes("isInitial"),
  "page activation should fire once on first entry and again only after an inactive-to-active transition",
);

assert.ok(
  activitySource.includes("interactive: boolean") &&
    activitySource.includes("refreshEnabled: boolean") &&
    activitySource.includes("export function usePageActivity"),
  "page activity should expose separate interaction and refresh axes",
);

assert.ok(
  activityQuerySource.includes("enabled: queryEnabled") &&
    activityQuerySource.includes("subscribed: active"),
  "inactive query consumers should disable both query execution and subscription",
);

const pages = [
  "src/features/dashboard/DashboardPage.tsx",
  "src/features/stations/StationsPage.tsx",
  "src/features/key-pool/KeyPoolPage.tsx",
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
  "src/features/stations/StationsPage.tsx",
  "src/features/key-pool/KeyPoolPage.tsx",
  "src/features/routing/RoutingPage.tsx",
  "src/features/pricing/PricingPage.tsx",
  "src/features/channels/ChannelStatusTab.tsx",
  "src/features/channels/ChannelMonitoringTab.tsx",
  "src/features/changes/ChangeCenterPage.tsx",
  "src/features/logs/LogsPage.tsx",
];
const activationOnlyPages = [
  "src/features/collectors/CollectorsPage.tsx",
  "src/features/settings/SettingsPage.tsx",
];

for (const page of refreshOnlyPages) {
  const source = await readFile(page, "utf8");
  assert.ok(
    source.includes("usePageRefreshEnabled") && !source.includes("usePageActivity"),
    `${page} should subscribe only to refresh permission, not the combined activity object`,
  );
}

for (const page of activationOnlyPages) {
  const source = await readFile(page, "utf8");
  assert.ok(
    source.includes("usePageActivation") && !source.includes("usePageActivity"),
    `${page} should rely on its activation hook without a redundant activity subscription`,
  );
}

for (const page of pages) {
  const source = await readFile(page, "utf8");
  assert.ok(
    source.includes("usePageActivation") || source.includes("useActivityQuery"),
    `${page} should refresh or subscribe to persisted data only when the page becomes active`,
  );
}

const monitoringSource = await readFile("src/features/channels/ChannelMonitoringTab.tsx", "utf8");
assert.ok(
  /usePageActivation\(\(\{ isInitial \}\) => \{[\s\S]*refresh\(false, isInitial\)/.test(monitoringSource),
  "monitoring should refresh silently when revisited while preserving first-load feedback",
);

const changeCenterSource = await readFile("src/features/changes/ChangeCenterPage.tsx", "utf8");
assert.ok(
  changeCenterSource.includes("useActivityQuery") &&
    !changeCenterSource.includes("usePageActivation") &&
    !changeCenterSource.includes("markUnreadChangeEventsReadLocally"),
  "change center should subscribe to cached data without owning a duplicate entry read path",
);
