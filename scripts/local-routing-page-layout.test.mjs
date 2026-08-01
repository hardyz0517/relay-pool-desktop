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
const workspacePanel = read("src/features/routing/RoutingOperationalPreviewPanel.tsx");
const statusTab = read("src/features/routing/LocalRoutingStatusTab.tsx");
const editTab = read("src/features/routing/LocalRoutingEditTab.tsx");
const candidateRow = read("src/features/routing/LocalRoutingCandidateRow.tsx");
const settingsEditor = read("src/features/routing/LocalRoutingSettingsEditor.tsx");
const settingsFields = read("src/features/routing/LocalRoutingSettingsFields.tsx");
const editSurface = editTab + settingsEditor + settingsFields;

assertIncludes(routingPage, "SegmentedControl", "RoutingPage");
assertIncludes(routingPage, "activeTab", "RoutingPage");
assertIncludes(routingPage, "工作台", "RoutingPage");
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

assertIncludes(workspacePanel, 'className="grid min-w-0 gap-4"', "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, 'contentClassName="grid min-w-0 gap-3"', "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, 'className="max-h-[420px] min-w-0 [&_table]:min-w-[980px]"', "RoutingOperationalPreviewPanel candidate table");
assertIncludes(workspacePanel, 'className: "w-[16rem] max-w-[16rem]"', "RoutingOperationalPreviewPanel candidate column");
assertIncludes(workspacePanel, 'className: "w-[18rem] max-w-[18rem]"', "RoutingOperationalPreviewPanel capability column");
assertIncludes(workspacePanel, 'className="relative min-w-0 flex-1 basis-[14rem]"', "RoutingOperationalPreviewPanel simulator input");
assertIncludes(workspacePanel, 'className="grid w-full min-w-0 gap-1 px-3 py-2 text-left text-sm hover:bg-hover"', "RoutingOperationalPreviewPanel decision rows");
assertIncludes(workspacePanel, 'className="grid min-w-0 gap-2 overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface p-3"', "RoutingOperationalPreviewPanel trace panel");
assertIncludes(workspacePanel, 'className="min-w-0 break-words"', "RoutingOperationalPreviewPanel long local names");
assertIncludes(workspacePanel, 'className="break-all">{row.detailCode}</span>', "RoutingOperationalPreviewPanel long detail codes");
assertIncludes(workspacePanel, 'className="break-all">key {row.stationKeyId}</span>', "RoutingOperationalPreviewPanel long key ids");
assertIncludes(workspacePanel, 'className="break-words rounded-[var(--surface-radius)] border border-danger-border', "RoutingOperationalPreviewPanel typed route errors");
assertExcludes(workspacePanel, 'className="grid gap-2 lg:grid-cols-[minmax(0,1fr)_minmax(22rem,0.8fr)]"', "RoutingOperationalPreviewPanel timeline grid");
assertExcludes(workspacePanel, 'className="relative min-w-[14rem] flex-1"', "RoutingOperationalPreviewPanel simulator input");

console.log("local routing page layout contract ok");
