import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pages = [
  "src/features/key-pool/KeyPoolPage.tsx",
  "src/features/routing/RoutingPage.tsx",
  "src/features/pricing/PricingPage.tsx",
  "src/features/channels/ChannelStatusTab.tsx",
  "src/features/channels/ChannelMonitoringTab.tsx",
  "src/features/collectors/CollectorsPage.tsx",
  "src/features/settings/SettingsPage.tsx",
];

for (const path of pages) {
  const source = await readFile(path, "utf8");
  assert.ok(
    source.includes("usePageActivation") ||
      source.includes("usePageRefreshEnabled") ||
      source.includes("useActivityQuery"),
    `${path} should read page activity`,
  );
  assert.ok(
    source.includes("useActivityQuery") || !/\bload[A-Z]\w*/.test(source),
    `${path} should use activity-bound server reads`,
  );
  assert.ok(!source.includes("window.setInterval"), `${path} must not own an unconditional interval`);
}

const keyPoolSource = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");
assert.ok(
  keyPoolSource.includes("useActivityQuery(keyPoolQueryOptions())") &&
    keyPoolSource.includes("useActivityQuery(stationsQueryOptions())") &&
    keyPoolSource.includes("useActivityQuery(channelMonitoringQueryOptions())") &&
    !keyPoolSource.includes("handleKeyPoolItemsUpdated") &&
    !keyPoolSource.includes("KEY_POOL_ITEMS_UPDATED_EVENT") &&
    !keyPoolSource.includes("usePageActivation") &&
    !keyPoolSource.includes("listChannelMonitors") &&
    !keyPoolSource.includes("listChannelMonitorTemplates") &&
    !keyPoolSource.includes("refreshMonitorResources"),
  "KeyPool reads should use activity-bound query owners without DOM update events or monitor activation loaders",
);

console.log("hidden page query boundary contract passed");
