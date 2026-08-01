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
const rustRoutingTypes = read("src-tauri/src/application/routing_engine/routing_types.rs");
const rustSnapshot = read("src-tauri/src/application/routing_engine/routing_snapshot.rs");

assert.match(settingsTypes, /"automatic_balanced"/);
assert.match(settingsPage, /defaultRoutingStrategy: "automatic_balanced"/);
assert.match(settingsTypes, /automatic_balanced: "自动路由"/);
assert.match(routingPage, /queryClient\.invalidateQueries/);
assert.match(routingPage, /queryKeys\.localRoutingWorkspace/);
assert.match(routingPage, /useActivityQuery/);
assert.doesNotMatch(routingPage, /SETTINGS_UPDATED_EVENT|addEventListener\(SETTINGS_UPDATED_EVENT/);
assert.match(settingsEditor, /queryClient\.setQueryData\(queryKeys\.settings, nextSettings\)/);
assert.match(settingsEditor, /queryClient\.invalidateQueries\(\{ queryKey: queryKeys\.localRoutingWorkspace \}\)/);
assert.match(settingsEditor, /queryClient\.invalidateQueries\(\{ queryKey: routingQueryKeys\.all \}\)/);
assert.match(localRoutingApi, /getActiveBackendClient\(\)\.localRouting\.loadLocalRoutingWorkspace\(\)/);
assert.match(localRoutingApi, /getActiveBackendClient\(\)\.localRouting\.reorderLocalRoutingKeys\(input\)/);
assert.doesNotMatch(localRoutingApi, /getSettings|settings\.localProxyPort/);

assert.match(localRoutingTypes, /maxRateMultiplier: number \| null/);
assert.match(localRoutingTypes, /routingGroupFilter: RoutingGroupFilter/);
assert.match(localRoutingTypes, /previewEligibleCandidateCount: number/);
assert.match(localRoutingTypes, /previewExcludedCandidateCount: number/);
assert.match(localRoutingTypes, /routingGroupScope: RoutingGroupFilter/);
assert.match(localRoutingTypes, /routingGroupMatch: boolean/);
assert.match(localRoutingTypes, /previewEligible: boolean/);
assert.match(localRoutingTypes, /previewRejectReasons: string\[\]/);
assert.doesNotMatch(localRoutingTypes, /effectiveMultiplier: number \| null/);
assert.doesNotMatch(localRoutingTypes, /effectiveMultiplierSource: string \| null/);
assert.doesNotMatch(localRoutingTypes, /effectiveMultiplierConfidence: number \| null/);
assert.doesNotMatch(localRoutingTypes, /schedulerRejectReason: string \| null/);

assert.match(rustRoutingTypes, /pub(?:\(crate\))? max_rate_multiplier: Option<f64>/);
assert.match(rustRoutingTypes, /pub(?:\(crate\))? routing_group_filter: RoutingGroupFilter/);
assert.match(rustRoutingTypes, /pub(?:\(crate\))? preview_eligible_candidate_count: i64/);
assert.match(rustRoutingTypes, /pub(?:\(crate\))? preview_excluded_candidate_count: i64/);
assert.doesNotMatch(rustRoutingTypes, /pub(?:\(crate\))? effective_multiplier: Option<f64>/);
assert.doesNotMatch(rustSnapshot, /scheduler_group_binding_id|scheduler_group_id_hash|scheduler_group_type/);
assert.match(rustSnapshot, /preview_eligible_candidate_count/);
assert.match(rustSnapshot, /preview_excluded_candidate_count/);
assert.match(rustSnapshot, /RouteCandidateProjection/);
assert.match(rustSnapshot, /projection_preview_reject_reasons/);
assert.match(rustSnapshot, /settings\.max_rate_multiplier/);
assert.match(rustSnapshot, /settings\.default_routing_group_filter/);
assert.doesNotMatch(rustSnapshot, /fn evaluate_candidate/);

assert.match(statusTab, /maxRateMultiplier/);
assert.match(statusTab, /effectiveMaxRateMultiplier/);
assert.match(statusTab, /不限制/);
assert.match(statusTab, /previewEligibleCandidateCount/);
assert.match(statusTab, /previewExcludedCandidateCount/);
assert.match(statusTab, /分组筛选/);
assert.match(statusTab, /自动路由/);

assert.match(editSurface, /自动调度/);
assert.match(editSurface, /倍率上限/);
assert.match(editTab, /LocalRoutingSettingsEditor/);
assert.doesNotMatch(editSurface, /低价稳定优先/);
assert.doesNotMatch(editSurface, /策略草稿/);
assert.doesNotMatch(editSurface, /运行时会综合/);

assert.match(candidateRow, /buildCandidateDisplayFacts/);
assert.doesNotMatch(candidateRow, /effectiveMultiplier/);
assert.doesNotMatch(candidateRow, /effectiveMultiplierSource/);
assert.doesNotMatch(candidateRow, /previewRejectReasons/);
assert.doesNotMatch(candidateRow, /formatPreviewRejectReason/);

assert.doesNotMatch(settingsPage, /routingStrategyLabels/);
assert.doesNotMatch(settingsPage, /handleDefaultRoutingStrategyChange/);
assert.doesNotMatch(settingsPage, /默认路由策略/);
for (const label of ["低余额阈值", "允许余额耗尽兜底"]) {
  assert.doesNotMatch(settingsPage, new RegExp(`label="${label}"`));
}
assert.match(settingsFields, /默认低余额阈值/);
assert.match(settingsFields, /余额耗尽兜底/);

console.log("local routing automatic settings contract ok");
