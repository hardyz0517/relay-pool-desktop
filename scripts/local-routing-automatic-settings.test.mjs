import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

function read(path) {
  return readFileSync(path, "utf8");
}

const localRoutingTypes = read("src/lib/types/localRouting.ts");
const settingsTypes = read("src/lib/types/settings.ts");
const settingsApi = read("src/lib/api/settings.ts");
const settingsPage = read("src/features/settings/SettingsPage.tsx");
const statusTab = read("src/features/routing/LocalRoutingStatusTab.tsx");
const editTab = read("src/features/routing/LocalRoutingEditTab.tsx");
const candidateRow = read("src/features/routing/LocalRoutingCandidateRow.tsx");
const rustRoutingTypes = read("src-tauri/src/services/proxy/routing_types.rs");
const rustSnapshot = read("src-tauri/src/services/proxy/routing_snapshot.rs");

assert.match(settingsTypes, /"automatic_balanced"/);
assert.match(settingsApi, /defaultRoutingStrategy: "automatic_balanced"/);
assert.match(settingsApi, /value === "automatic" \|\| value === "automatic_balanced"/);

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

assert.match(editTab, /自动调度/);
assert.match(editTab, /倍率上限/);
assert.doesNotMatch(editTab, /低价稳定优先/);
assert.doesNotMatch(editTab, /策略草稿/);

assert.match(candidateRow, /effectiveMultiplier/);
assert.match(candidateRow, /effectiveMultiplierSource/);
assert.match(candidateRow, /schedulerRejectReason/);

assert.doesNotMatch(settingsPage, /routingStrategyLabels/);
assert.doesNotMatch(settingsPage, /handleDefaultRoutingStrategyChange/);
assert.doesNotMatch(settingsPage, /默认路由策略/);

console.log("local routing automatic settings contract ok");
