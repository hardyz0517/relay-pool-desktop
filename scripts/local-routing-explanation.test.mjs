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

const statusTab = read("src/features/routing/LocalRoutingStatusTab.tsx");
const routingPage = read("src/features/routing/RoutingPage.tsx");
const appPage = read("src/app/App.tsx");

for (const removedText of ["排障入口", "最近一次路由解释", "RouteExplanationPanel", "onOpenPage"]) {
  assertExcludes(statusTab, removedText, "LocalRoutingStatusTab");
}

assertExcludes(routingPage, "onOpenPage", "RoutingPage");
assertExcludes(appPage, "<RoutingPage onOpenPage=", "App");
assertIncludes(statusTab, 'SectionCard title="本地路由状态"', "routing status band");
assertIncludes(statusTab, 'aria-labelledby="local-routing-candidates-title"', "candidate preview section");
assertIncludes(statusTab, "最近一次路由", "latest decision row");
assertIncludes(statusTab, "baseline_eligibility", "candidate baseline eligibility title");
assertIncludes(statusTab, "candidateHeading", "candidate heading derives from preview kind");

console.log("local routing status simplification contract ok");
