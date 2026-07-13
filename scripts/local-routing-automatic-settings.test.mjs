import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

function read(path) {
  return readFileSync(path, "utf8");
}

const localRoutingTypes = read("src/lib/types/localRouting.ts");
const settingsTypes = read("src/lib/types/settings.ts");
const settingsApi = read("src/lib/api/settings.ts");
const settingsPage = read("src/features/settings/SettingsPage.tsx");
const routingPage = read("src/features/routing/RoutingPage.tsx");
const localRoutingApi = read("src/lib/api/localRouting.ts");
const statusTab = read("src/features/routing/LocalRoutingStatusTab.tsx");
const editTab = read("src/features/routing/LocalRoutingEditTab.tsx");
const settingsEditor = read("src/features/routing/LocalRoutingSettingsEditor.tsx");
const settingsFields = read("src/features/routing/LocalRoutingSettingsFields.tsx");
const editSurface = editTab + settingsEditor + settingsFields;
const candidateRow = read("src/features/routing/LocalRoutingCandidateRow.tsx");
const rustRoutingTypes = read("src-tauri/src/services/proxy/routing_types.rs");
const rustSnapshot = read("src-tauri/src/services/proxy/routing_snapshot.rs");

assert.match(settingsTypes, /"automatic_balanced"/);
assert.match(settingsApi, /defaultRoutingStrategy: "automatic_balanced"/);
assert.match(settingsApi, /value === "automatic" \|\| value === "automatic_balanced"/);
assert.match(routingPage, /SETTINGS_UPDATED_EVENT/);
assert.match(routingPage, /addEventListener\(SETTINGS_UPDATED_EVENT/);
assert.match(routingPage, /removeEventListener\(SETTINGS_UPDATED_EVENT/);
assert.match(routingPage, /queryClient\.invalidateQueries/);
assert.match(routingPage, /queryKeys\.localRoutingWorkspace/);
assert.match(routingPage, /useActivityQuery/);
assert.match(localRoutingApi, /getSettings/);
assert.match(localRoutingApi, /settings\.localProxyPort/);
assert.match(localRoutingApi, /port:\s*settings\.localProxyPort/);
assert.match(localRoutingApi, /settings\.maxRateMultiplier/);
assert.match(localRoutingApi, /maxRateMultiplier:\s*settings\.maxRateMultiplier/);
assert.match(localRoutingApi, /settings\.defaultRoutingGroupFilter/);
assert.match(localRoutingApi, /routingGroupFilter:\s*settings\.defaultRoutingGroupFilter/);

assert.match(localRoutingTypes, /maxRateMultiplier: number \| null/);
assert.match(localRoutingTypes, /routingGroupFilter: RoutingGroupFilter/);
assert.match(localRoutingTypes, /eligibleUnderMultiplierLimitCount: number/);
assert.match(localRoutingTypes, /effectiveMultiplier: number \| null/);
assert.match(localRoutingTypes, /effectiveMultiplierSource: string \| null/);
assert.match(localRoutingTypes, /effectiveMultiplierConfidence: number \| null/);
assert.match(localRoutingTypes, /routingGroupScope: RoutingGroupFilter/);
assert.match(localRoutingTypes, /routingGroupMatch: boolean/);
assert.match(localRoutingTypes, /schedulerRejectReason: string \| null/);

assert.match(rustRoutingTypes, /pub max_rate_multiplier: Option<f64>/);
assert.match(rustRoutingTypes, /pub routing_group_filter: RoutingGroupFilter/);
assert.match(rustRoutingTypes, /pub eligible_under_multiplier_limit_count: i64/);
assert.match(rustRoutingTypes, /pub effective_multiplier: Option<f64>/);
assert.match(rustSnapshot, /eligible_under_multiplier_limit_count/);
assert.match(rustSnapshot, /settings\.max_rate_multiplier/);
assert.match(rustSnapshot, /settings\.default_routing_group_filter/);

assert.match(statusTab, /maxRateMultiplier/);
assert.match(statusTab, /eligibleUnderMultiplierLimitCount/);
assert.match(statusTab, /倍率未知或过期不参与路由/);
assert.match(statusTab, /分组筛选/);
assert.match(statusTab, /自动路由/);

assert.match(editSurface, /自动调度/);
assert.match(editSurface, /倍率上限/);
assert.match(editTab, /LocalRoutingSettingsEditor/);
assert.doesNotMatch(editSurface, /低价稳定优先/);
assert.doesNotMatch(editSurface, /策略草稿/);
assert.doesNotMatch(editSurface, /运行时会综合/);

assert.match(candidateRow, /effectiveMultiplier/);
assert.match(candidateRow, /effectiveMultiplierSource/);
assert.match(candidateRow, /schedulerRejectReason/);

assert.doesNotMatch(settingsPage, /routingStrategyLabels/);
assert.doesNotMatch(settingsPage, /handleDefaultRoutingStrategyChange/);
assert.doesNotMatch(settingsPage, /默认路由策略/);
for (const label of ["低余额阈值", "允许余额耗尽兜底"]) {
  assert.doesNotMatch(settingsPage, new RegExp(`label="${label}"`));
}
assert.match(settingsFields, /默认低余额阈值/);
assert.match(settingsFields, /余额耗尽兜底/);

console.log("local routing automatic settings contract ok");
