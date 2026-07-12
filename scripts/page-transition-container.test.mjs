import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

function normalizeSource(source) {
  return source.replace(/\r\n?/g, "\n");
}

const appSource = normalizeSource(await readFile("src/app/App.tsx", "utf8"));
const hostSource = normalizeSource(await readFile("src/app/TransientPageHost.tsx", "utf8"));

assert.ok(
  appSource.includes('from "@/app/TransientPageHost"') &&
    /<TransientPageHost\s+page=\{activeTransientPage\}\s+onExitComplete=\{restoreTransientReturnFocus\}\s*\/>/.test(
      appSource,
    ),
  "App should delegate transient rendering and presence cleanup to one host",
);
assert.equal(
  appSource.match(/<TransientPageHost\b/g)?.length ?? 0,
  1,
  "App should render exactly one transient host",
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
  ) && !/\buseMemo\b/.test(appSource),
  "App should construct the active descriptor directly so callbacks stay fresh without mirrored dependencies",
);
assert.match(
  appSource,
  /default:\s*\{\s*const exhaustivePageId: never = pageId;\s*return exhaustivePageId;\s*\}/,
  "transient route rendering should fail TypeScript exhaustiveness when a future page is unhandled",
);
assert.ok(
  appSource.includes("transientParentRouteId: AppRouteId | null;") &&
    appSource.includes("transientParentRouteId: null,") &&
    appSource.includes("resolveTransientParentRouteId(") &&
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
  appSource.includes('type ShellPageState = "active" | "background" | "inactive";') &&
    appSource.includes('isCurrentTransientPage ? "background" : "active"') &&
    appSource.includes('const active = shellPageState === "active";') &&
    appSource.includes('const inert = shellPageState !== "active";'),
  "shell pages should have explicit active, visible-background, and inactive states",
);
assert.ok(
  appSource.includes("<PageActivityProvider key={routeId} active={active}>") &&
    appSource.includes("data-page-transition-state={shellPageState}") &&
    appSource.includes('inert={inert ? "" : undefined}') &&
    appSource.includes("aria-hidden={inert}"),
  "background and inactive shells should remain mounted without refreshing or accepting focus",
);
assert.ok(
  appSource.includes("mountedRouteIds.has(activeShellRouteId)") &&
    appSource.includes("[...mountedRouteIds, activeShellRouteId]"),
  "a transient route should always have its retained parent shell rendered beneath it",
);
assert.ok(
  appSource.includes("const isReturningFromTransient") &&
    appSource.includes(
      'data-page-transition-handoff={isReturningFromTransient ? "transient-exit" : "none"}',
    ),
  "returning from a transient page should not retrigger the shell entry animation",
);
assert.ok(
  appSource.includes(
    "onPointerDownCapture={(event) => rememberShellFocusTarget(event.target)}",
  ) &&
    appSource.includes("onFocusCapture={(event) => rememberShellFocusTarget(event.target)}"),
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

console.log("page transition container contract ok");
