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
    appSource.includes("data-page-transition-direction") &&
    appSource.includes("data-page-transition-handoff"),
  "App should expose stable transition data attributes for styling and tests",
);

assert.ok(
  appSource.includes("inert={") &&
    appSource.includes("aria-hidden={"),
  "App should isolate inactive and outgoing pages from focus and screen readers",
);

assert.ok(
  appSource.includes('inert?: "" | undefined'),
  "React inert typing should allow the empty string attribute value",
);

assert.ok(
  appSource.includes('inert={inert ? "" : undefined}'),
  "inactive shell layers should render inert as an empty string attribute",
);

assert.ok(
  appSource.includes('inert={transientPageIsExiting ? "" : undefined}'),
  "the single transient layer should become inert while exiting",
);

assert.ok(
  /<PageActivityProvider[\s\S]*?active=\{!transientPageIsExiting\}/.test(appSource),
  "the retained transient page should become inactive without remounting",
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

assert.ok(
  appSource.includes("const pendingExitingTransientPage") &&
    appSource.includes("const renderedTransientPage") &&
    appSource.includes("activeTransientPage ??") &&
    appSource.includes("pendingExitingTransientPage ??") &&
    appSource.includes("exitingTransientPage"),
  "App should keep the outgoing transient page in the same render branch before layout effects run",
);

assert.equal(
  appSource.match(/className="app-page-transition-layer app-page-transition-overlay"/g)?.length ?? 0,
  1,
  "App should render one stable transient layer instead of remounting an outgoing clone",
);

assert.ok(
  !appSource.includes("{activeTransientPage && (") &&
    !appSource.includes("{exitingTransientPage && (") &&
    appSource.includes("{renderedTransientPage.node}"),
  "active and exiting transient states should share one component position",
);

assert.ok(
  appSource.includes("const isReturningFromTransient") &&
    appSource.includes(
      'data-page-transition-handoff={isReturningFromTransient ? "transient-exit" : "none"}',
    ),
  "return handoff state should survive outgoing overlay cleanup so the shell cannot animate late",
);

const pendingTransientReadIndex = sourceIndex(
  "const pendingExitingTransientPage",
  "App should derive the outgoing transient page during render",
);
const activeTransientWriteIndex = sourceIndex(
  "lastActiveTransientPageRef.current = activeTransientPage",
  "App should update the last active transient ref after preserving the outgoing page",
);

assert.ok(
  pendingTransientReadIndex < activeTransientWriteIndex &&
    appSource.includes("previousRouteId: current.activeRouteId") &&
    appSource.includes("useLayoutEffect(() => {") &&
    appSource.includes("}, [activeTransientPage, pendingExitingTransientPage]);"),
  "App should preserve navigation history and the outgoing transient instance before paint",
);

assert.ok(
  appSource.includes("handleTransientExitAnimationEnd") &&
    appSource.includes("event.target !== event.currentTarget") &&
    /onAnimationEnd=\{[\s\S]*?transientPageIsExiting\s*\?\s*handleTransientExitAnimationEnd/.test(
      appSource,
    ) &&
    appSource.includes("window.setTimeout(handleTransientExitComplete"),
  "outgoing transient cleanup should ignore bubbled animationend events and keep timeout fallback",
);

assert.ok(
  appSource.includes("app-page-transition-stack") &&
    appSource.includes("app-page-transition-layer") &&
    appSource.includes("app-page-transition-overlay"),
  "App should wire the transition stack, layer, and overlay classes",
);

console.log("page transition container contract ok");
