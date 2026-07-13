import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const app = await readFile("src/app/App.tsx", "utf8");
const appShell = await readFile("src/components/shell/AppShell.tsx", "utf8");
const host = await readFile("src/app/ShellPageHost.tsx", "utf8");
const controller = await readFile("src/app/navigationController.ts", "utf8");
const styles = await readFile("src/styles.css", "utf8");

assert.match(app, /useNavigationController\("dashboard"\)/);
assert.match(app, /activeRouteId=\{intent\.shellRouteId\}/);
assert.match(app, /committedNavigationSequence=\{committed\.sequence\}/);
assert.match(app, /intentNavigationSequence=\{intent\.sequence\}/);
assert.match(app, /intentShellRouteId=\{intent\.shellRouteId\}/);
assert.match(host, /motion\.div/);
assert.match(host, /MotionConfig reducedMotion="user"/);
assert.match(host, /onAnimationComplete/);
assert.match(host, /committedNavigationSequence > completedNavigationSequence/);
assert.match(host, /isLatestShellNavigationCompletion/);
assert.match(host, /refreshRouteId/);
assert.match(controller, /shouldNavigateToRoute\(intentRef\.current, routeId\)/);
assert.match(appShell, /data-navigation-route-id=\{route\.id\}/);
assert.match(host, /intentShellRouteId !== activeShellRouteId/);
assert.match(host, /return "leaving"/);
assert.match(
  host,
  /if \(routeId === previousShellRouteId\) \{\s*return "leaving";\s*\}/,
  "the outgoing page should keep one continuous leaving state until entry completes",
);
assert.match(
  host,
  /entering:\s*\{\s*opacity:\s*\[0,\s*1\],\s*y:\s*\[4,\s*0\],[\s\S]*?duration:\s*0\.16/,
  "the entering page should preserve the v0.2.0 4px/160ms fade-up feel",
);
assert.match(
  host,
  /leaving:\s*\{\s*opacity:\s*1,\s*transition:\s*\{\s*duration:\s*0\s*\}\s*\}/,
  "the current page should stay visually stable while the target render is pending",
);
assert.ok(!host.includes("visualHandoff"));
assert.ok(!host.includes("previousShellRouteIdRef"));
assert.match(styles, /data-page-transition-state="entering"/);
assert.ok(!host.includes('mode="wait"'));
assert.ok(!styles.includes("relayShellPageContentEnter"));
assert.ok(!/entering[\s\S]{0,300}(translate|scale|blur)/.test(styles));

console.log("navigation handoff contract passed");
