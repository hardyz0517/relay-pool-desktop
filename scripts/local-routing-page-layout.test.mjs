import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();

function read(path) {
  return readFileSync(join(root, path), "utf8");
}

function assertIncludes(source, needle, label) {
  if (!source.includes(needle)) {
    throw new Error(`${label} should include ${needle}`);
  }
}

function assertExcludes(source, needle, label) {
  if (source.includes(needle)) {
    throw new Error(`${label} should not include ${needle}`);
  }
}

function assertNotSharedWorkspacePromiseAll(source) {
  const sharedLoadPattern =
    /Promise\.all\(\[\s*loadRoutingWorkspace\(\),\s*loadLocalRoutingWorkspace\(\),?\s*\]\)/;
  if (sharedLoadPattern.test(source)) {
    throw new Error("RoutingPage should load legacy and local routing workspaces independently");
  }
}

const routingPage = read("src/features/routing/RoutingPage.tsx");
const statusTab = read("src/features/routing/LocalRoutingStatusTab.tsx");
const editTab = read("src/features/routing/LocalRoutingEditTab.tsx");
const candidateRow = read("src/features/routing/LocalRoutingCandidateRow.tsx");
const settingsEditor = read("src/features/routing/LocalRoutingSettingsEditor.tsx");
const settingsFields = read("src/features/routing/LocalRoutingSettingsFields.tsx");
const editSurface = editTab + settingsEditor + settingsFields;

assertIncludes(routingPage, "SegmentedControl", "RoutingPage");
assertIncludes(routingPage, "activeTab", "RoutingPage");
assertIncludes(routingPage, "状态", "RoutingPage");
assertIncludes(routingPage, "编辑", "RoutingPage");
assertIncludes(statusTab, "本地路由状态", "LocalRoutingStatusTab");
assertIncludes(statusTab, "最近一次路由", "LocalRoutingStatusTab");
assertIncludes(statusTab, "baseline_eligibility", "LocalRoutingStatusTab");
assertIncludes(statusTab, "candidateHeading", "LocalRoutingStatusTab");
assertIncludes(statusTab, "previewEligibleCandidateCount", "LocalRoutingStatusTab");
assertIncludes(statusTab, "previewExcludedCandidateCount", "LocalRoutingStatusTab");
assertIncludes(statusTab, "MetricPanel", "LocalRoutingStatusTab");
assertIncludes(statusTab, "路由策略概览", "LocalRoutingStatusTab");
assertIncludes(statusTab, "候选状态", "LocalRoutingStatusTab");
assertIncludes(statusTab, "routeMetricValueClassName", "LocalRoutingStatusTab");
assertIncludes(statusTab, 'text-[20px] leading-6 text-foreground', "LocalRoutingStatusTab");
assertExcludes(statusTab, "当前秘钥", "LocalRoutingStatusTab");
assertExcludes(statusTab, "当前密钥", "LocalRoutingStatusTab");
assertExcludes(statusTab, "eligibleUnderMultiplierLimitCount", "LocalRoutingStatusTab");
assertExcludes(statusTab, "healthyCandidateCount", "LocalRoutingStatusTab");
assertExcludes(statusTab, "function Metric(", "LocalRoutingStatusTab");
assertExcludes(statusTab, "lg:[&>*]:h-full", "LocalRoutingStatusTab");
assertExcludes(statusTab, "grid-rows-[auto_minmax(0,1fr)]", "LocalRoutingStatusTab");
assertExcludes(statusTab, 'contentClassName="grid h-full content-center gap-3"', "LocalRoutingStatusTab");
assertExcludes(statusTab, "formatDecisionTime(workspace.summary.lastDecisionAt)", "LocalRoutingStatusTab");
assertIncludes(statusTab, "baseline_eligibility", "LocalRoutingStatusTab");
assertIncludes(statusTab, "candidateHeading", "LocalRoutingStatusTab");
assertExcludes(statusTab, "latestDecision?.reason", "LocalRoutingStatusTab");
assertExcludes(statusTab, "latestDecision.badge ?", "LocalRoutingStatusTab");
assertExcludes(statusTab, "function StatusMetric(", "LocalRoutingStatusTab");
assertIncludes(statusTab, "倍率上限", "LocalRoutingStatusTab");
assertIncludes(statusTab, "分组筛选", "LocalRoutingStatusTab");
assertIncludes(editTab, "LocalRoutingSettingsEditor", "LocalRoutingEditTab");
assertExcludes(editTab, "border border-slate-200 bg-white divide-y", "LocalRoutingEditTab");
assertIncludes(editSurface, "自动调度", "LocalRoutingEditTab surface");
assertExcludes(editSurface, "权重", "LocalRoutingEditTab surface");
assertExcludes(editSurface, "拖拽", "LocalRoutingEditTab surface");
assertExcludes(editSurface, "重排", "LocalRoutingEditTab surface");
assertIncludes(candidateRow, 'grid min-h-[68px] gap-3 px-3 py-2.5', "LocalRoutingCandidateRow");
assertIncludes(candidateRow, "参与状态", "LocalRoutingCandidateRow");
assertIncludes(candidateRow, "有效倍率", "LocalRoutingCandidateRow");
assertIncludes(candidateRow, "formatPreviewRejectReason", "LocalRoutingCandidateRow");
assertExcludes(candidateRow, "ObjectRow", "LocalRoutingCandidateRow");
assertExcludes(routingPage, "保存策略", "RoutingPage");
assertNotSharedWorkspacePromiseAll(routingPage);

console.log("local routing page layout contract ok");
