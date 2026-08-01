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
const diagnosticsPanel = read("src/features/routing/RoutingStatusDiagnosticsPanel.tsx");
const statusTab = read("src/features/routing/LocalRoutingStatusTab.tsx");
const editTab = read("src/features/routing/LocalRoutingEditTab.tsx");
const candidateRow = read("src/features/routing/LocalRoutingCandidateRow.tsx");
const statusCandidateRow = read("src/features/routing/LocalRoutingStatusCandidateRow.tsx");
const settingsEditor = read("src/features/routing/LocalRoutingSettingsEditor.tsx");
const settingsFields = read("src/features/routing/LocalRoutingSettingsFields.tsx");
const editSurface = editTab + settingsEditor + settingsFields;

assertIncludes(routingPage, "SegmentedControl", "RoutingPage");
assertIncludes(routingPage, "activeTab", "RoutingPage");
assertIncludes(routingPage, 'type LocalRoutingTab = "status" | "edit"', "RoutingPage");
assertIncludes(routingPage, 'value: "status"', "RoutingPage");
assertIncludes(routingPage, 'value: "edit"', "RoutingPage");
assertIncludes(routingPage, "RoutingStatusDiagnosticsPanel", "RoutingPage");
assertExcludes(routingPage, 'value: "workspace"', "RoutingPage");
assertExcludes(routingPage, "RoutingOperationalPreviewPanel", "RoutingPage");
assertExcludes(routingPage, "保存策略", "RoutingPage");
assertNotSharedWorkspacePromiseAll(routingPage);

assertIncludes(statusTab, "MetricPanel", "LocalRoutingStatusTab");
assertIncludes(statusTab, "baseline_eligibility", "LocalRoutingStatusTab");
assertIncludes(statusTab, "candidateHeading", "LocalRoutingStatusTab");
assertIncludes(statusTab, "previewEligibleCandidateCount", "LocalRoutingStatusTab");
assertIncludes(statusTab, "previewExcludedCandidateCount", "LocalRoutingStatusTab");
assertIncludes(statusTab, "routeMetricValueClassName", "LocalRoutingStatusTab");
assertIncludes(statusTab, 'text-[20px] leading-6 text-foreground', "LocalRoutingStatusTab");
assertExcludes(statusTab, "eligibleUnderMultiplierLimitCount", "LocalRoutingStatusTab");
assertExcludes(statusTab, "healthyCandidateCount", "LocalRoutingStatusTab");
assertExcludes(statusTab, "function Metric(", "LocalRoutingStatusTab");
assertExcludes(statusTab, "lg:[&>*]:h-full", "LocalRoutingStatusTab");
assertExcludes(statusTab, "grid-rows-[auto_minmax(0,1fr)]", "LocalRoutingStatusTab");
assertExcludes(statusTab, 'contentClassName="grid h-full content-center gap-3"', "LocalRoutingStatusTab");
assertExcludes(statusTab, "formatDecisionTime(workspace.summary.lastDecisionAt)", "LocalRoutingStatusTab");
assertExcludes(statusTab, "latestDecision?.reason", "LocalRoutingStatusTab");
assertExcludes(statusTab, "latestDecision.badge ?", "LocalRoutingStatusTab");
assertExcludes(statusTab, "function StatusMetric(", "LocalRoutingStatusTab");
assertIncludes(statusTab, "LocalRoutingStatusCandidateHeader", "LocalRoutingStatusTab");
assertIncludes(statusCandidateRow, "候选密钥", "LocalRoutingStatusCandidateRow header");
assertIncludes(statusCandidateRow, "sm:hidden", "LocalRoutingStatusCandidateRow mobile labels");

assertIncludes(diagnosticsPanel, "路由诊断", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, "模拟路由", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, "最近决策", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, "simulateRouteQuery", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, "runtimeOverlay?.candidates", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, "deepLink.kind === \"simulate-model\"", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, "deepLink.kind === \"station-key\"", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, "deepLink.kind === \"station\"", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, 'className="relative min-w-0 flex-1 basis-[14rem]"', "RoutingStatusDiagnosticsPanel simulator input");
assertExcludes(diagnosticsPanel, "DataTableLite", "RoutingStatusDiagnosticsPanel");
assertExcludes(diagnosticsPanel, "previewPolicyVersion", "RoutingStatusDiagnosticsPanel");
assertExcludes(diagnosticsPanel, "capacityMode", "RoutingStatusDiagnosticsPanel");
assertExcludes(diagnosticsPanel, "runtime rev", "RoutingStatusDiagnosticsPanel");
assertExcludes(diagnosticsPanel, "Operational detail", "RoutingStatusDiagnosticsPanel");

assertIncludes(editTab, "LocalRoutingSettingsEditor", "LocalRoutingEditTab");
assertExcludes(editTab, "border border-slate-200 bg-white divide-y", "LocalRoutingEditTab");
assertIncludes(editSurface, "自动调度", "LocalRoutingEditTab surface");
assertExcludes(editSurface, "权重", "LocalRoutingEditTab surface");
assertExcludes(editSurface, "拖拽", "LocalRoutingEditTab surface");
assertExcludes(editSurface, "重排", "LocalRoutingEditTab surface");
assertIncludes(candidateRow, 'grid min-h-[68px] gap-3 px-3 py-2.5', "LocalRoutingCandidateRow");
assertIncludes(editTab, "LocalRoutingCandidateHeader", "LocalRoutingEditTab");
assertIncludes(candidateRow, "候选密钥", "LocalRoutingCandidateRow header");
assertIncludes(candidateRow, "sm:hidden", "LocalRoutingCandidateRow mobile labels");
assertIncludes(candidateRow, "buildCandidateDisplayFacts", "LocalRoutingCandidateRow");
assertExcludes(candidateRow, "ObjectRow", "LocalRoutingCandidateRow");

console.log("local routing page layout contract ok");
