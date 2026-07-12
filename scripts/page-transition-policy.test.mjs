import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const policySource = await readFile("src/app/pageTransitionPolicy.ts", "utf8");
const appSource = await readFile("src/app/App.tsx", "utf8");
const policyModule = await import("../src/app/pageTransitionPolicy.ts");

const shellPages = [
  "dashboard",
  "stations",
  "keyPool",
  "routing",
  "pricing",
  "channels",
  "collectors",
  "changes",
  "logs",
  "settings",
];

const transientPages = [
  ["addProvider", "stations"],
  ["editProvider", "stations"],
  ["stationDetail", "stations"],
  ["addKey", "keyPool"],
  ["editKey", "keyPool"],
  ["modelBasePrices", "pricing"],
];

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function getPolicyBlock(pageId) {
  const policyBlockPattern = new RegExp(
    `\\b${escapeRegExp(pageId)}:\\s*\\{([\\s\\S]*?)\\n\\s*\\}`,
  );
  const match = policySource.match(policyBlockPattern);
  assert.ok(match, `page transition policy should include policy block ${pageId}`);
  return match[1];
}

for (const routeId of shellPages) {
  const policyBlock = getPolicyBlock(routeId);

  assert.ok(
      policyBlock.includes(`pageId: "${routeId}"`) &&
      policyBlock.includes('kind: "shell"') &&
      policyBlock.includes(`parentRouteId: "${routeId}"`) &&
      policyBlock.includes('retention: "keep"') &&
      policyBlock.includes("prewarmPriority:"),
    `page transition policy should map shell route ${routeId} to itself`,
  );
}

for (const [pageId, parentRouteId] of transientPages) {
  const policyBlock = getPolicyBlock(pageId);

  assert.ok(
      policyBlock.includes(`pageId: "${pageId}"`) &&
      policyBlock.includes(`parentRouteId: "${parentRouteId}"`) &&
      policyBlock.includes('kind: "transient"') &&
      policyBlock.includes('retention: "keep"') &&
      policyBlock.includes("prewarmPriority: null"),
    `page transition policy should map ${pageId} to parent route ${parentRouteId}`,
  );
  assert.deepEqual(
    [...policyBlock.matchAll(/^\s*(\w+):/gm)].map((match) => match[1]),
    ["pageId", "kind", "parentRouteId", "retention", "prewarmPriority"],
    `transient policy ${pageId} should contain only pageId, kind, parentRouteId, retention, and prewarmPriority`,
  );
}

for (const directionIdentifier of [
  "PageTransitionDirection",
  "enterDirection",
  "exitDirection",
]) {
  assert.ok(
    !policySource.includes(directionIdentifier),
    `page transition policy should remove dead direction metadata: ${directionIdentifier}`,
  );
}

const {
  resolveActiveShellRouteId,
  resolveTransientParentRouteId,
} = policyModule;
assert.equal(
  typeof resolveTransientParentRouteId,
  "function",
  "policy should export a pure resolver for the actual transient parent",
);
assert.equal(
  typeof resolveActiveShellRouteId,
  "function",
  "policy should export a pure resolver for the active shell",
);

assert.equal(
  resolveTransientParentRouteId("settings", "modelBasePrices", null),
  "settings",
  "settings -> modelBasePrices should retain settings as the actual parent",
);
assert.equal(
  resolveTransientParentRouteId("pricing", "modelBasePrices", null),
  "pricing",
  "pricing -> modelBasePrices should retain pricing as the actual parent",
);
assert.equal(
  resolveTransientParentRouteId("modelBasePrices", "editProvider", "settings"),
  "settings",
  "transient replacement should preserve the invoking shell",
);
assert.equal(
  resolveTransientParentRouteId("modelBasePrices", "settings", "settings"),
  null,
  "navigating to a shell should clear the transient parent",
);
assert.equal(
  resolveActiveShellRouteId("modelBasePrices", "settings"),
  "settings",
  "an actual transient parent should override the static fallback",
);
assert.equal(
  resolveActiveShellRouteId("modelBasePrices", null),
  "pricing",
  "a direct transient route should fall back to its static policy parent",
);

assert.ok(
  policySource.includes("export function getPageTransitionPolicy"),
  "policy helper should export getPageTransitionPolicy",
);

assert.ok(
  policySource.includes("export function isShellPage"),
  "policy helper should export isShellPage",
);

assert.ok(
  policySource.includes("export function getShellRouteId"),
  "policy helper should export getShellRouteId",
);

assert.ok(
  appSource.includes('from "@/app/pageTransitionPolicy"'),
  "App should import route classification from the transition policy helper",
);

assert.ok(
  !/function isShellPage\(pageId: AppPageId\)/.test(appSource) &&
    !/function getShellRouteId\(pageId: AppPageId\)/.test(appSource),
  "App should not keep duplicate local route classification helpers",
);

console.log("page transition policy contract ok");
