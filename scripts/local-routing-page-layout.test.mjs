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

const routingPage = read("src/features/routing/RoutingPage.tsx");
const statusTab = read("src/features/routing/LocalRoutingStatusTab.tsx");
const editTab = read("src/features/routing/LocalRoutingEditTab.tsx");

assertIncludes(routingPage, "SegmentedControl", "RoutingPage");
assertIncludes(routingPage, "activeTab", "RoutingPage");
assertIncludes(routingPage, "状态", "RoutingPage");
assertIncludes(routingPage, "编辑", "RoutingPage");
assertIncludes(statusTab, "本地端点", "LocalRoutingStatusTab");
assertIncludes(statusTab, "当前主 Key", "LocalRoutingStatusTab");
assertIncludes(editTab, "低价优先 + 稳定保持", "LocalRoutingEditTab");
assertExcludes(editTab, "权重", "LocalRoutingEditTab");
assertExcludes(editTab, "拖拽", "LocalRoutingEditTab");
assertExcludes(editTab, "重排", "LocalRoutingEditTab");
assertExcludes(routingPage, "保存策略", "RoutingPage");

console.log("local routing page layout contract ok");
