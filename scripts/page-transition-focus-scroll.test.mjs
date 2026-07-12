import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

function normalizeSource(source) {
  return source.replace(/\r\n?/g, "\n");
}

const [appSource, hostSource, shellSource, navigationSource, policySource, stylesSource] =
  await Promise.all(
    [
      "src/app/App.tsx",
      "src/app/TransientPageHost.tsx",
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

const stackRule = readRule(".app-page-transition-stack");
const layerRule = readRule(
  ".app-page-transition-layer,\n[data-page-transition-layer]",
);
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
assertDeclaration(scrollbarRule, "display", "none");

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
  appSource.includes(
    "onPointerDownCapture={(event) => rememberShellFocusTarget(event.target)}",
  ) &&
    appSource.includes("onFocusCapture={(event) => rememberShellFocusTarget(event.target)}"),
  "the transition stack should track shell invokers from pointer and focus capture",
);
const navigateToSource =
  appSource.match(/const navigateTo = useCallback\([\s\S]*?\n  \);/)?.[0] ?? "";
assert.ok(
  navigateToSource.includes("isShellPage(activeRouteId) && !isShellPage(routeId)") &&
    navigateToSource.includes("lastShellFocusTargetRef.current") &&
    navigateToSource.includes("document.activeElement") &&
    navigateToSource.includes("transientReturnFocusRef.current =") &&
    /\[\s*activeRouteId\s*\],?\s*\);/.test(navigateToSource),
  "shell-to-transient navigation should capture the recorded or active shell element without overwriting transient replacements",
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
  appSource,
  /<TransientPageHost\s+page=\{activeTransientPage\}\s+onExitComplete=\{restoreTransientReturnFocus\}\s*\/>/,
  "App should restore shell focus through the transient host exit callback",
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
  hostSource.includes('<AnimatePresence initial={false} mode="wait">') &&
    hostSource.includes("cloneElement(transientPresence, {") &&
    hostSource.includes("onExitComplete: () =>") &&
    hostSource.includes("if (!page)") &&
    hostSource.includes("onExitComplete?.()") &&
    hostSource.includes('initial={false}') &&
    hostSource.includes('mode="wait"'),
  "presence completion should restore focus only after closing to a shell, not during transient replacement",
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
