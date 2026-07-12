import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const appSource = await readFile("src/app/App.tsx", "utf8");
const activitySource = await readFile("src/components/shell/PageActivity.tsx", "utf8").catch(() => "");
const activityQuerySource = await readFile("src/lib/query/useActivityQuery.ts", "utf8").catch(() => "");

assert.ok(
  appSource.includes("PageActivityProvider") &&
    appSource.includes('const active = shellPageState === "active";') &&
    appSource.includes('isCurrentTransientPage ? "background" : "active"') &&
    appSource.includes("<PageActivityProvider key={routeId} active={active}>"),
  "kept-alive shell pages should refresh only in active state, never while serving as a transient background",
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

for (const page of pages) {
  const source = await readFile(page, "utf8");
  assert.ok(
    source.includes("usePageActivation"),
    `${page} should refresh its persisted data whenever the page becomes active`,
  );
}

const monitoringSource = await readFile("src/features/channels/ChannelMonitoringTab.tsx", "utf8");
assert.ok(
  /usePageActivation\(\(\{ isInitial \}\) => \{[\s\S]*refresh\(false, isInitial\)/.test(monitoringSource),
  "monitoring should refresh silently when revisited while preserving first-load feedback",
);

const changeCenterSource = await readFile("src/features/changes/ChangeCenterPage.tsx", "utf8");
assert.ok(
  /usePageActivation\(\(\{ isInitial \}\) => \{[\s\S]*refresh\(false, isInitial\)/.test(changeCenterSource),
  "change center should rerun entry refresh and mark unread events read whenever revisited",
);
