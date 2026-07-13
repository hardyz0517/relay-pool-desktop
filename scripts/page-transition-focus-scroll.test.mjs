import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

function normalizeSource(source) {
  return source.replace(/\r\n?/g, "\n");
}

const [appSource, hostSource, shellHostSource, controllerSource, navigationPolicySource, shellSource, navigationSource, policySource, stylesSource] =
  await Promise.all(
    [
      "src/app/App.tsx",
      "src/app/TransientPageHost.tsx",
      "src/app/ShellPageHost.tsx",
      "src/app/navigationController.ts",
      "src/app/navigationPolicy.ts",
      "src/components/shell/AppShell.tsx",
      "src/lib/types/navigation.ts",
      "src/app/pageTransitionPolicy.ts",
      "src/styles.css",
    ].map(async (path) => normalizeSource(await readFile(path, "utf8"))),
  );

function readRule(selector) {
  const opening = `${selector} {`;
  const lineStart = stylesSource.indexOf(`\n${opening}`);
  const ruleStart = stylesSource.startsWith(opening)
    ? 0
    : lineStart === -1
      ? -1
      : lineStart + 1;

  assert.notEqual(ruleStart, -1, `styles should define exact rule ${selector}`);

  const bodyStart = ruleStart + opening.length;
  let depth = 1;

  for (let index = bodyStart; index < stylesSource.length; index += 1) {
    if (stylesSource[index] === "{") {
      depth += 1;
    } else if (stylesSource[index] === "}") {
      depth -= 1;
      if (depth === 0) {
        return stylesSource.slice(bodyStart, index);
      }
    }
  }

  assert.fail(`rule ${selector} should have a closing brace`);
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function assertDeclaration(ruleBody, property, value) {
  assert.match(
    ruleBody,
    new RegExp(
      `^\\s*${escapeRegExp(property)}:\\s*${escapeRegExp(value)};\\s*$`,
      "m",
    ),
    `expected ${property}: ${value}; in exact rule body`,
  );
}

function assertNoDeclaration(ruleBody, property) {
  assert.doesNotMatch(
    ruleBody,
    new RegExp(`^\\s*${escapeRegExp(property)}\\s*:`, "m"),
    `exact rule body should not declare ${property}`,
  );
}

const mainClassMatch = shellSource.match(/<main\s+className="([^"]+)"/);
assert.ok(mainClassMatch, "AppShell should render main with a static className");
const mainClasses = new Set(mainClassMatch[1].split(/\s+/));

for (const className of ["min-h-0", "flex-1", "overflow-hidden"]) {
  assert.ok(mainClasses.has(className), `AppShell main should include ${className}`);
}
for (const className of ["overflow-auto", "overflow-y-auto"]) {
  assert.ok(!mainClasses.has(className), `AppShell main should not include ${className}`);
}
assert.ok(
  !mainClasses.has("p-[var(--shell-page-gap)]"),
  "AppShell main should not leave a static gutter outside the page scrollport",
);

const stackRule = readRule(".app-page-transition-stack");
const layerRule = readRule(
  ".app-page-transition-layer,\n[data-page-transition-layer]",
);
const contentRule = readRule(".app-page-transition-content");
const scrollbarRule = readRule(
  ".app-page-transition-layer::-webkit-scrollbar,\n[data-page-transition-layer]::-webkit-scrollbar",
);

assertDeclaration(stackRule, "height", "100%");
assertDeclaration(stackRule, "min-height", "100%");
assertDeclaration(layerRule, "height", "100%");
assertDeclaration(layerRule, "min-height", "100%");
assertDeclaration(layerRule, "min-width", "0");
assertDeclaration(layerRule, "overflow-y", "auto");
assertDeclaration(layerRule, "scrollbar-width", "none");
assertNoDeclaration(layerRule, "padding");
assertDeclaration(contentRule, "padding", "var(--shell-page-gap)");
assertDeclaration(scrollbarRule, "display", "none");

assert.ok(
  /<motion\.div[\s\S]*?className="app-page-transition-content"/.test(shellHostSource) &&
    shellHostSource.includes("<ShellPageContent routeId={routeId} actions={actions} />"),
  "retained shell pages should use the same inner gutter wrapper as transient pages",
);

for (const property of ["transform", "transition", "animation"]) {
  assertNoDeclaration(layerRule, property);
}

assert.ok(
  appSource.includes("lastShellFocusTargetRef") &&
    appSource.includes("transientReturnFocusRef"),
  "App should own shell focus tracking and transient return-focus refs",
);
assert.match(
  appSource,
  /const rememberShellFocusTarget = useCallback\(\s*\(target: EventTarget \| null\)/,
  "App should own a memoized shell focus tracker that accepts EventTarget or null",
);
assert.ok(
  appSource.includes("target instanceof Element") &&
    appSource.includes("target.closest<HTMLElement>(ACTIONABLE_ELEMENT_SELECTOR)") &&
    appSource.includes(
      '[data-page-transition-kind="shell"][data-page-transition-state="active"]',
    ) &&
    appSource.includes("lastShellFocusTargetRef.current = candidate"),
  "shell focus tracking should resolve an actionable ancestor only in the active shell layer",
);
assert.ok(
  shellHostSource.includes(
    "onPointerDownCapture={(event) => onRememberShellFocusTarget(event.target)}",
  ) &&
    shellHostSource.includes("onFocusCapture={(event) => onRememberShellFocusTarget(event.target)}"),
  "the transition stack should track shell invokers from pointer and focus capture",
);
const navigateToSource =
  appSource.match(/const navigateTo = useCallback\([\s\S]*?\n  \}, \[\]\);/)?.[0] ?? "";
assert.ok(
  navigateToSource.includes("isShellPage(activeRouteIdRef.current) && !isShellPage(routeId)") &&
    navigateToSource.includes("lastShellFocusTargetRef.current") &&
    navigateToSource.includes("document.activeElement") &&
    navigateToSource.includes("transientReturnFocusRef.current =") &&
    controllerSource.includes("resolveTransientParentRouteId(") &&
    /\[\s*\]\);/.test(navigateToSource),
  "shell-to-transient navigation should capture focus and its actual shell parent without overwriting replacements",
);
assert.ok(
  navigationPolicySource.includes("export type CommittedNavigation") &&
    navigationPolicySource.includes("transientParentRouteId: AppRouteId | null;") &&
    /resolveActiveShellRouteId\(\s*activeRouteId,\s*transientParentRouteId,?\s*\)/.test(
      appSource,
    ),
  "the actual invoking shell should remain the visible background so its focus target can be restored",
);
assert.ok(
  appSource.includes("const restoreTransientReturnFocus = useCallback") &&
    appSource.includes("transientReturnFocusRef.current = null") &&
    appSource.includes("target?.isConnected") &&
    appSource.includes('target.closest("[inert]")') &&
    appSource.includes("target.focus({ preventScroll: true })"),
  "return focus should clear its ref and restore only a connected, non-inert target without scrolling",
);
assert.match(
  shellHostSource,
  /<TransientPageHost\s+page=\{activeTransientPage\}\s+onExitComplete=\{onExitComplete\}\s*\/>/,
  "ShellPageHost should forward the transient host exit callback",
);
assert.ok(
  appSource.includes("onExitComplete={restoreTransientReturnFocus}"),
  "App should restore shell focus through the ShellPageHost exit callback",
);

assert.match(
  hostSource,
  /type TransientPageHostProps = \{[\s\S]*?onExitComplete\?: \(\) => void;[\s\S]*?\};/,
  "TransientPageHost should accept an optional exit-complete callback",
);
assert.ok(
  hostSource.includes("useLayoutEffect") &&
    hostSource.includes("useRef") &&
    hostSource.includes("const rootRef = useRef<HTMLDivElement>(null)") &&
    hostSource.includes("useLayoutEffect(() =>") &&
    /useLayoutEffect\(\(\) => \{[\s\S]*?\}, \[\]\);/.test(hostSource) &&
    hostSource.includes("ref={rootRef}"),
  "each transient layer should own a root ref and a mount-only layout focus effect",
);
assert.ok(
  hostSource.includes('querySelector<HTMLElement>("[data-page-autofocus]")') &&
    hostSource.includes("querySelector<HTMLElement>(ACTIONABLE_ELEMENT_SELECTOR)") &&
    hostSource.includes("focusTarget?.focus({ preventScroll: true })"),
  "a transient should prefer explicit autofocus, then focus its first actionable control without scrolling",
);
assert.ok(
  !/\bcloneElement\b/.test(hostSource),
  "the host should pass lifecycle props directly to AnimatePresence without cloneElement",
);
assert.ok(
  hostSource.includes("completeTransientPageExit"),
  "the AnimatePresence exit callback should delegate to the executable exit policy",
);

const { completeTransientPageExit } = await import(
  "../src/app/transientPageExitPolicy.ts"
);

function createCapturedCompletion(initialSnapshot) {
  let latestSnapshot = initialSnapshot;
  const capturedCompletion = () => completeTransientPageExit(latestSnapshot);

  return {
    capturedCompletion,
    commit(snapshot) {
      latestSnapshot = snapshot;
    },
  };
}

let activeToFinalStaleCalls = 0;
let activeToFinalLatestCalls = 0;
const activeToFinal = createCapturedCompletion({
  hasActivePage: true,
  onExitComplete: () => {
    activeToFinalStaleCalls += 1;
  },
});
const activeToFinalCompletion = activeToFinal.capturedCompletion;
activeToFinal.commit({
  hasActivePage: false,
  onExitComplete: () => {
    activeToFinalLatestCalls += 1;
  },
});
assert.equal(
  activeToFinal.capturedCompletion,
  activeToFinalCompletion,
  "Motion should keep the same captured completion callback while committed state changes",
);
activeToFinalCompletion();
assert.equal(
  activeToFinalStaleCalls,
  0,
  "active-to-final navigation should not call the stale captured callback",
);
assert.equal(
  activeToFinalLatestCalls,
  1,
  "active-to-final navigation should call the latest committed callback exactly once",
);

let finalToActiveStaleCalls = 0;
let finalToActiveLatestCalls = 0;
const finalToActive = createCapturedCompletion({
  hasActivePage: false,
  onExitComplete: () => {
    finalToActiveStaleCalls += 1;
  },
});
const finalToActiveCompletion = finalToActive.capturedCompletion;
finalToActive.commit({
  hasActivePage: true,
  onExitComplete: () => {
    finalToActiveLatestCalls += 1;
  },
});
assert.equal(
  finalToActive.capturedCompletion,
  finalToActiveCompletion,
  "replacement navigation should not replace Motion's captured completion callback",
);
finalToActiveCompletion();
assert.equal(
  finalToActiveStaleCalls,
  0,
  "final-to-active navigation should not call the stale captured callback",
);
assert.equal(
  finalToActiveLatestCalls,
  0,
  "final-to-active navigation should not call the latest active-page callback",
);

assert.doesNotThrow(
  () => completeTransientPageExit({ hasActivePage: false }),
  "a final exit without an optional host callback should remain safe",
);

assert.ok(
  navigationSource.includes(
    "export type TransientPageId = Exclude<AppPageId, AppRouteId>;",
  ),
  "navigation should export the exhaustive transient page ID union",
);
assert.ok(
  hostSource.includes("pageId: TransientPageId;") &&
    !hostSource.includes("pageId: AppPageId;"),
  "transient descriptors should reject shell route IDs",
);

for (const directionIdentifier of [
  "PageTransitionDirection",
  "enterDirection",
  "exitDirection",
]) {
  assert.ok(
    !policySource.includes(directionIdentifier),
    `transition policy should remove dead direction metadata: ${directionIdentifier}`,
  );
}

console.log("page transition focus and scroll contract ok");
