import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const appSource = await readFile("src/app/App.tsx", "utf8");

function sourceIndex(snippet, message) {
  const index = appSource.indexOf(snippet);
  assert.notEqual(index, -1, message);
  return index;
}

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
  !appSource.includes("}, [activeTransientPage]);"),
  "last active transient page should not be updated in a standalone earlier effect",
);

const previousTransientReadIndex = sourceIndex(
  "const previousTransientPage = lastActiveTransientPageRef.current",
  "App should read the previous transient page before updating refs",
);
const previousRouteWriteIndex = sourceIndex(
  "previousRouteIdRef.current = activeRouteId",
  "App should update the previous route ref inside the combined transient preservation effect",
);
const activeTransientWriteIndex = sourceIndex(
  "lastActiveTransientPageRef.current = activeTransientPage",
  "App should update the last active transient ref after preserving the outgoing page",
);

assert.ok(
  previousTransientReadIndex < activeTransientWriteIndex &&
    previousRouteWriteIndex < activeTransientWriteIndex &&
    appSource.includes("}, [activeRouteId, activeTransientPage]);"),
  "App should preserve outgoing transient pages before updating the last active transient ref",
);

assert.ok(
  appSource.includes("handleTransientExitTransitionEnd") &&
    appSource.includes("event.target !== event.currentTarget") &&
    appSource.includes("onTransitionEnd={handleTransientExitTransitionEnd}") &&
    appSource.includes("window.setTimeout(handleTransientExitComplete"),
  "outgoing transient cleanup should ignore bubbled transitionend events and keep timeout fallback",
);

assert.ok(
  appSource.includes("app-page-transition-stack") &&
    appSource.includes("app-page-transition-layer") &&
    appSource.includes("app-page-transition-overlay"),
  "App should wire the transition stack, layer, and overlay classes",
);

console.log("page transition container contract ok");
