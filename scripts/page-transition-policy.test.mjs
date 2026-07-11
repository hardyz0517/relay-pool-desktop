import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const policySource = await readFile("src/app/pageTransitionPolicy.ts", "utf8");
const appSource = await readFile("src/app/App.tsx", "utf8");

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

for (const routeId of shellPages) {
  assert.ok(
    policySource.includes(`"${routeId}"`),
    `page transition policy should include shell route ${routeId}`,
  );
}

for (const [pageId, parentRouteId] of transientPages) {
  assert.ok(
    policySource.includes(`${pageId}:`) &&
      policySource.includes(`parentRouteId: "${parentRouteId}"`) &&
      policySource.includes('kind: "transient"') &&
      policySource.includes('enterDirection: "forward"') &&
      policySource.includes('exitDirection: "back"'),
    `page transition policy should map ${pageId} to parent route ${parentRouteId}`,
  );
}

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
