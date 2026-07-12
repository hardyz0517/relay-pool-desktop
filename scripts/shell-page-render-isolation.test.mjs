import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const app = await readFile("src/app/App.tsx", "utf8");
const host = await readFile("src/app/ShellPageHost.tsx", "utf8").catch(() => "");
const registry = await readFile("src/app/shellPageRegistry.tsx", "utf8").catch(() => "");
const boundary = await readFile("src/app/ShellPageErrorBoundary.tsx", "utf8").catch(() => "");

assert.match(host, /const ShellPageSlot = memo/);
assert.match(host, /<ShellPageContent routeId=\{routeId\}/);
assert.match(host, /data-page-transition-page-id=\{routeId\}/);
assert.match(registry, /export type ShellPageActions/);
assert.match(registry, /export const ShellPageContent = memo/);
assert.match(boundary, /getDerivedStateFromError/);
assert.match(host, /<ShellPageErrorBoundary>/);
assert.ok(!app.includes("function renderShellPage"));
assert.ok(!app.includes("shellRouteIds.map"));
assert.ok(!host.includes("children: ReactNode"));

console.log("shell page render isolation contract passed");
