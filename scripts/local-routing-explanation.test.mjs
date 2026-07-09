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
    throw new Error(`${label} should not include secret-bearing text: ${needle}`);
  }
}

const explanationPanel = read("src/features/routing/RouteExplanationPanel.tsx");
const statusTab = read("src/features/routing/LocalRoutingStatusTab.tsx");

for (const source of [explanationPanel, statusTab]) {
  assertExcludes(source, "apiKey", "local routing explanation UI");
  assertExcludes(source, "Authorization", "local routing explanation UI");
}

assertIncludes(explanationPanel, "selectedReason", "RouteExplanationPanel");
assertIncludes(explanationPanel, "keptForStability", "RouteExplanationPanel");
assertIncludes(explanationPanel, "决策字段", "RouteExplanationPanel");
assertIncludes(explanationPanel, "FieldRow", "RouteExplanationPanel");
assertIncludes(explanationPanel, "fallbackCount", "RouteExplanationPanel");
assertIncludes(explanationPanel, "authorization|x-api-key|api[_-]?key", "RouteExplanationPanel");
assertIncludes(explanationPanel, "bearer|basic", "RouteExplanationPanel");

assertIncludes(statusTab, "渠道状态", "LocalRoutingStatusTab");
assertIncludes(statusTab, "请求日志", "LocalRoutingStatusTab");
assertIncludes(statusTab, "onOpenPage", "LocalRoutingStatusTab");
assertIncludes(statusTab, "\"channels\"", "LocalRoutingStatusTab");
assertIncludes(statusTab, "\"logs\"", "LocalRoutingStatusTab");

const routingPage = read("src/features/routing/RoutingPage.tsx");
const appPage = read("src/app/App.tsx");
assertIncludes(routingPage, "onOpenPage={onOpenPage}", "RoutingPage");
assertIncludes(appPage, "onOpenPage={(routeId) => setActiveRouteId(routeId)}", "App");

console.log("local routing explanation UI contract ok");
