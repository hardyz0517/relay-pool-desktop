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
  const sharedLoadPattern = /Promise\.all\(\[\s*loadRoutingWorkspace\(\),\s*loadLocalRoutingWorkspace\(\),?\s*\]\)/;
  if (sharedLoadPattern.test(source)) {
    throw new Error("RoutingPage should load legacy and local routing workspaces independently");
  }
}

const routingPage = read("src/features/routing/RoutingPage.tsx");
const statusTab = read("src/features/routing/LocalRoutingStatusTab.tsx");
const editTab = read("src/features/routing/LocalRoutingEditTab.tsx");
const settingsEditor = read("src/features/routing/LocalRoutingSettingsEditor.tsx");
const settingsFields = read("src/features/routing/LocalRoutingSettingsFields.tsx");
const editSurface = editTab + settingsEditor + settingsFields;

assertIncludes(routingPage, "SegmentedControl", "RoutingPage");
assertIncludes(routingPage, "activeTab", "RoutingPage");
assertIncludes(routingPage, "状态", "RoutingPage");
assertIncludes(routingPage, "编辑", "RoutingPage");
assertIncludes(statusTab, "本地端点", "LocalRoutingStatusTab");
assertIncludes(statusTab, "当前秘钥", "LocalRoutingStatusTab");
assertIncludes(statusTab, "lg:[&>*]:h-full", "LocalRoutingStatusTab");
assertExcludes(statusTab, "latestDecision?.reason", "LocalRoutingStatusTab");
assertIncludes(statusTab, "倍率上限", "LocalRoutingStatusTab");
assertIncludes(statusTab, "分组筛选", "LocalRoutingStatusTab");
assertIncludes(editTab, "LocalRoutingSettingsEditor", "LocalRoutingEditTab");
assertIncludes(editSurface, "自动调度", "LocalRoutingEditTab surface");
assertExcludes(editSurface, "权重", "LocalRoutingEditTab surface");
assertExcludes(editSurface, "拖拽", "LocalRoutingEditTab surface");
assertExcludes(editSurface, "重排", "LocalRoutingEditTab surface");
assertExcludes(routingPage, "保存策略", "RoutingPage");
assertNotSharedWorkspacePromiseAll(routingPage);

console.log("local routing page layout contract ok");
