import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

function normalizeSource(source) {
  return source.replace(/\r\n?/g, "\n");
}

const appSource = normalizeSource(await readFile("src/app/App.tsx", "utf8"));
const hostSource = normalizeSource(await readFile("src/app/TransientPageHost.tsx", "utf8"));
const shellHostSource = normalizeSource(await readFile("src/app/ShellPageHost.tsx", "utf8"));
const controllerSource = normalizeSource(await readFile("src/app/navigationController.ts", "utf8"));
const policySource = normalizeSource(await readFile("src/app/navigationPolicy.ts", "utf8"));

assert.ok(
  shellHostSource.includes('from "@/app/TransientPageHost"') &&
    /<TransientPageHost\s+page=\{activeTransientPage\}\s+onExitComplete=\{onExitComplete\}\s*\/>/.test(
      shellHostSource,
    ),
  "ShellPageHost should delegate transient rendering and presence cleanup to one host",
);
assert.equal(
  shellHostSource.match(/<TransientPageHost\b/g)?.length ?? 0,
  1,
  "ShellPageHost should render exactly one transient host",
);

for (const legacyIdentifier of [
  "TRANSIENT_EXIT_TIMEOUT_MS",
  "RenderedTransientPage",
  "lastActiveTransientPageRef",
  "transientExitTimeoutRef",
  "exitingTransientPage",
  "pendingExitingTransientPage",
  "handleTransientExitComplete",
  "handleTransientExitAnimationEnd",
  "useLayoutEffect",
  "onAnimationEnd",
]) {
  assert.ok(
    !appSource.includes(legacyIdentifier),
    `App should remove the manual transient lifecycle: ${legacyIdentifier}`,
  );
}

assert.ok(
  /import type \{[^}]*\bTransientPageId\b[^}]*\} from "@\/lib\/types\/navigation";/.test(
    appSource,
  ) &&
    !appSource.includes("type TransientPageId = Exclude<AppPageId, AppRouteId>;") &&
    appSource.includes(
      "function renderTransientPage(pageId: TransientPageId): TransientPageDescriptor",
    ) &&
    appSource.includes("switch (pageId)"),
  "transient descriptors should be typed separately from retained shell routes",
);
assert.ok(
  appSource.includes(
    `const activeTransientPage = isShellPage(activeRouteId)
    ? null
    : renderTransientPage(activeRouteId);`,
  ),
  "App should construct the active descriptor directly so callbacks stay fresh without mirrored dependencies",
);
assert.match(
  appSource,
  /default:\s*\{\s*const exhaustivePageId: never = pageId;\s*return exhaustivePageId;\s*\}/,
  "transient route rendering should fail TypeScript exhaustiveness when a future page is unhandled",
);
assert.ok(
  policySource.includes("transientParentRouteId: AppRouteId | null;") &&
    controllerSource.includes("transientParentRouteId: null,") &&
    controllerSource.includes("resolveTransientParentRouteId(") &&
    appSource.includes("resolveActiveShellRouteId("),
  "navigation state should retain the actual invoking shell and resolve static parents only as fallback",
);

const modelBasePricesCase =
  appSource.match(/case "modelBasePrices":([\s\S]*?)default:/)?.[1] ?? "";
assert.ok(
  modelBasePricesCase.includes("navigateTo(activeShellRouteId)") &&
    modelBasePricesCase.includes('backLabel={`返回${activeShellRouteLabel}`}') &&
    !modelBasePricesCase.includes('navigateTo("pricing")'),
  "model base prices should return to and describe its actual invoking shell",
);
assert.ok(
  appSource.includes('import { appRoutes } from "@/app/routes";') &&
    /appRoutes\.find\(\s*\(route\) => route\.id === activeShellRouteId,?\s*\)\?\.label/.test(
      appSource,
    ),
  "App should derive transient back copy from active-shell route metadata without page-specific labels",
);

assert.ok(
  shellHostSource.includes('export type ShellPageState = "active" | "background" | "entering" | "inactive";') &&
    shellHostSource.includes('transientActive ? "background" : "active"') &&
    shellHostSource.includes('const active = state === "active" || state === "entering";') &&
    shellHostSource.includes('const inert = !active;'),
  "shell pages should have explicit active, visible-background, entering, and inactive states",
);
assert.ok(
  shellHostSource.includes("<PageActivityProvider active={active}>") &&
    shellHostSource.includes("data-page-transition-state={state}") &&
    shellHostSource.includes('inert={inert ? "" : undefined}') &&
    shellHostSource.includes("aria-hidden={inert}"),
  "background and inactive shells should remain mounted without refreshing or accepting focus",
);
assert.ok(
  shellHostSource.includes("mountedRouteIds.has(activeShellRouteId)") &&
    shellHostSource.includes("[...mountedRouteIds, activeShellRouteId]"),
  "a transient route should always have its retained parent shell rendered beneath it",
);
assert.ok(
  appSource.includes("const isReturningFromTransient") &&
    shellHostSource.includes(
      'data-page-transition-handoff={returningFromTransient ? "transient-exit" : "none"}',
    ),
  "returning from a transient page should not retrigger the shell entry animation",
);
assert.ok(
  shellHostSource.includes(
    "onPointerDownCapture={(event) => onRememberShellFocusTarget(event.target)}",
  ) &&
    shellHostSource.includes("onFocusCapture={(event) => onRememberShellFocusTarget(event.target)}"),
  "the transition stack should centrally capture the shell invoker",
);

for (const instanceKey of [
  'instanceKey: "addProvider"',
  'instanceKey: `editProvider:${editingStationId ?? "edit-provider-empty"}`',
  'instanceKey: `stationDetail:${detailStationId ?? "station-detail-empty"}`',
  'instanceKey: `addKey:${initialKeyStationId ?? "add-key-unscoped"}`',
  'instanceKey: `editKey:${editingKeyId ?? "edit-key-empty"}`',
  'instanceKey: "modelBasePrices"',
]) {
  assert.ok(appSource.includes(instanceKey), `App should define stable identity: ${instanceKey}`);
}

assert.ok(
  hostSource.includes("key={page.instanceKey}") &&
    hostSource.includes('data-page-transition-state={isPresent ? "active" : "exiting"}'),
  "the host should use descriptor identity and Motion presence as its only exit state",
);

assert.ok(
  shellHostSource.includes("data-page-transition-layer") &&
    shellHostSource.includes("data-page-transition-state") &&
    shellHostSource.includes("data-page-transition-kind") &&
    shellHostSource.includes("data-page-transition-handoff") &&
    !shellHostSource.includes("data-page-transition-direction"),
  "App and Host should expose stable direction-free transition attributes",
);
assert.ok(
  shellHostSource.includes('inert={inert ? "" : undefined}') &&
    shellHostSource.includes("aria-hidden={inert}") &&
    hostSource.includes('inert={isPresent ? undefined : ""}') &&
    hostSource.includes("aria-hidden={!isPresent}"),
  "inactive shell and outgoing transient content should be isolated from focus and screen readers",
);
assert.ok(
  shellHostSource.includes("app-page-transition-stack") &&
    shellHostSource.includes("app-page-transition-layer") &&
    hostSource.includes("app-page-transition-overlay"),
  "App and Host should wire the transition stack, shell layer, and overlay classes",
);

console.log("page transition container contract ok");
