import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const appSource = await readFile("src/app/App.tsx", "utf8");

assert.ok(
  appSource.includes("TRANSIENT_EXIT_TIMEOUT_MS"),
  "App should define a bounded timeout for outgoing transient cleanup",
);

assert.ok(
  appSource.includes("exitingTransientPage") &&
    appSource.includes("setExitingTransientPage") &&
    appSource.includes("lastActiveTransientPageRef") &&
    appSource.includes("handleTransientExitComplete"),
  "App should remember the last rendered transient page and clean it after exit animation",
);

assert.ok(
  appSource.includes("data-page-transition-layer") &&
    appSource.includes("data-page-transition-state") &&
    appSource.includes("data-page-transition-kind") &&
    appSource.includes("data-page-transition-direction"),
  "App should expose stable transition data attributes for styling and tests",
);

assert.ok(
  appSource.includes("inert={") &&
    appSource.includes("aria-hidden={"),
  "App should isolate inactive and outgoing pages from focus and screen readers",
);

assert.ok(
  appSource.includes("<PageActivityProvider active={isCurrentTransientPage}>") &&
    appSource.includes("<PageActivityProvider active={false}>"),
  "transient pages should have explicit active/inactive PageActivityProvider contexts",
);

assert.ok(
  appSource.includes("const active = activeRouteId === routeId && !isCurrentTransientPage") &&
    appSource.includes("<PageActivityProvider key={routeId} active={active}>"),
  "shell PageActivityProvider should be inactive while a transient overlay is current",
);

assert.ok(
  appSource.includes("onTransitionEnd={handleTransientExitComplete}") &&
    appSource.includes("window.setTimeout(handleTransientExitComplete"),
  "outgoing transient cleanup should use transitionend and timeout fallback",
);

assert.ok(
  appSource.includes("app-page-transition-stack") &&
    appSource.includes("app-page-transition-layer") &&
    appSource.includes("app-page-transition-overlay"),
  "App should wire the transition stack, layer, and overlay classes",
);

console.log("page transition container contract ok");
