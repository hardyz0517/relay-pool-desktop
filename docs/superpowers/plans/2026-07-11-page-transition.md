# Page Transition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build reliable, lightweight page transitions for Relay Pool Desktop main navigation pages and internal transient pages.

**Architecture:** Keep `AppShell` stable and move transition behavior into the app page container. Add a focused transition policy helper so route classification, parent sidebar route mapping, and transition direction live in one place. Use CSS transitions, React state, `PageActivityProvider`, and explicit hidden/focus isolation without introducing a new animation library.

**Tech Stack:** React 18, TypeScript, Vite, Tailwind CSS utilities, plain CSS in `src/styles.css`, Node static contract scripts in `scripts/*.test.mjs`.

---

## File Structure

- Create: `src/app/pageTransitionPolicy.ts`
  - Owns route classification, parent shell route mapping, transition kind, and direction metadata.
- Modify: `src/app/App.tsx`
  - Uses transition policy helpers, keeps shell pages mounted, preserves outgoing transient pages for exit animation, and applies active/inactive accessibility state.
- Modify: `src/styles.css`
  - Adds page transition CSS variables/classes and `prefers-reduced-motion` behavior.
- Create: `scripts/page-transition-policy.test.mjs`
  - Static contract test for all current page ids and policy exports.
- Create: `scripts/page-transition-container.test.mjs`
  - Static contract test for App container behavior: explicit transient activity context, inert/aria isolation, overlay cleanup, and style class wiring.
- Create: `scripts/page-transition-styles.test.mjs`
  - Static contract test for transition CSS selectors, animations, pointer isolation, and reduced-motion support.
- Modify: `docs/superpowers/plans/2026-07-11-page-transition.md`
  - Check off completed tasks during execution.

---

### Task 1: Page Transition Policy Contract

**Files:**
- Create: `scripts/page-transition-policy.test.mjs`
- Create: `src/app/pageTransitionPolicy.ts`
- Modify: `src/app/App.tsx`

- [ ] **Step 1: Write the failing policy contract test**

Create `scripts/page-transition-policy.test.mjs`:

```js
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
```

- [ ] **Step 2: Run the policy contract test and verify RED**

Run:

```powershell
node scripts/page-transition-policy.test.mjs
```

Expected: FAIL because `src/app/pageTransitionPolicy.ts` does not exist yet.

- [ ] **Step 3: Add the transition policy helper**

Create `src/app/pageTransitionPolicy.ts`:

```ts
import type { AppPageId, AppRouteId } from "@/lib/types/navigation";

export type PageTransitionKind = "shell" | "transient";
export type PageTransitionDirection = "none" | "forward" | "back";

export type PageTransitionPolicy = {
  pageId: AppPageId;
  kind: PageTransitionKind;
  parentRouteId: AppRouteId;
  enterDirection: PageTransitionDirection;
  exitDirection: PageTransitionDirection;
};

const shellPagePolicies: Record<AppRouteId, PageTransitionPolicy> = {
  dashboard: {
    pageId: "dashboard",
    kind: "shell",
    parentRouteId: "dashboard",
    enterDirection: "none",
    exitDirection: "none",
  },
  stations: {
    pageId: "stations",
    kind: "shell",
    parentRouteId: "stations",
    enterDirection: "none",
    exitDirection: "none",
  },
  keyPool: {
    pageId: "keyPool",
    kind: "shell",
    parentRouteId: "keyPool",
    enterDirection: "none",
    exitDirection: "none",
  },
  routing: {
    pageId: "routing",
    kind: "shell",
    parentRouteId: "routing",
    enterDirection: "none",
    exitDirection: "none",
  },
  pricing: {
    pageId: "pricing",
    kind: "shell",
    parentRouteId: "pricing",
    enterDirection: "none",
    exitDirection: "none",
  },
  channels: {
    pageId: "channels",
    kind: "shell",
    parentRouteId: "channels",
    enterDirection: "none",
    exitDirection: "none",
  },
  collectors: {
    pageId: "collectors",
    kind: "shell",
    parentRouteId: "collectors",
    enterDirection: "none",
    exitDirection: "none",
  },
  changes: {
    pageId: "changes",
    kind: "shell",
    parentRouteId: "changes",
    enterDirection: "none",
    exitDirection: "none",
  },
  logs: {
    pageId: "logs",
    kind: "shell",
    parentRouteId: "logs",
    enterDirection: "none",
    exitDirection: "none",
  },
  settings: {
    pageId: "settings",
    kind: "shell",
    parentRouteId: "settings",
    enterDirection: "none",
    exitDirection: "none",
  },
};

const transientPagePolicies = {
  addProvider: {
    pageId: "addProvider",
    kind: "transient",
    parentRouteId: "stations",
    enterDirection: "forward",
    exitDirection: "back",
  },
  editProvider: {
    pageId: "editProvider",
    kind: "transient",
    parentRouteId: "stations",
    enterDirection: "forward",
    exitDirection: "back",
  },
  stationDetail: {
    pageId: "stationDetail",
    kind: "transient",
    parentRouteId: "stations",
    enterDirection: "forward",
    exitDirection: "back",
  },
  addKey: {
    pageId: "addKey",
    kind: "transient",
    parentRouteId: "keyPool",
    enterDirection: "forward",
    exitDirection: "back",
  },
  editKey: {
    pageId: "editKey",
    kind: "transient",
    parentRouteId: "keyPool",
    enterDirection: "forward",
    exitDirection: "back",
  },
  modelBasePrices: {
    pageId: "modelBasePrices",
    kind: "transient",
    parentRouteId: "pricing",
    enterDirection: "forward",
    exitDirection: "back",
  },
} satisfies Record<string, PageTransitionPolicy>;

const pageTransitionPolicies = {
  ...shellPagePolicies,
  ...transientPagePolicies,
} satisfies Record<AppPageId, PageTransitionPolicy>;

export function getPageTransitionPolicy(pageId: AppPageId): PageTransitionPolicy {
  return pageTransitionPolicies[pageId];
}

export function isShellPage(pageId: AppPageId): pageId is AppRouteId {
  return getPageTransitionPolicy(pageId).kind === "shell";
}

export function getShellRouteId(pageId: AppPageId): AppRouteId {
  return getPageTransitionPolicy(pageId).parentRouteId;
}
```

- [ ] **Step 4: Move App route classification imports to the policy helper**

Modify the import section in `src/app/App.tsx`:

```ts
import { useEffect, useState, type ReactNode } from "react";
import { AppShell } from "@/components/shell/AppShell";
import { PageActivityProvider } from "@/components/shell/PageActivity";
import { getShellRouteId, isShellPage } from "@/app/pageTransitionPolicy";
```

Remove the local `isShellPage()` and `getShellRouteId()` function definitions at the bottom of `src/app/App.tsx`. Keep the `type AppRouteId` import because `renderShellPage(routeId: AppRouteId)` still uses it.

- [ ] **Step 5: Run the policy contract test and verify GREEN**

Run:

```powershell
node scripts/page-transition-policy.test.mjs
```

Expected output:

```text
page transition policy contract ok
```

- [ ] **Step 6: Commit Task 1**

Run:

```powershell
git add -- scripts/page-transition-policy.test.mjs src/app/pageTransitionPolicy.ts src/app/App.tsx
git commit -m "feat: add page transition policy"
```

---

### Task 2: Page Transition Container Contract

**Files:**
- Create: `scripts/page-transition-container.test.mjs`
- Modify: `src/app/App.tsx`

- [ ] **Step 1: Write the failing container contract test**

Create `scripts/page-transition-container.test.mjs`:

```js
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
```

- [ ] **Step 2: Run the container contract test and verify RED**

Run:

```powershell
node scripts/page-transition-container.test.mjs
```

Expected: FAIL because `src/app/App.tsx` does not yet keep outgoing transient pages or expose transition data attributes.

- [ ] **Step 3: Add outgoing transient state and cleanup helpers**

In `src/app/App.tsx`, update the React import:

```ts
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
```

Update the transition policy import:

```ts
import {
  getPageTransitionPolicy,
  getShellRouteId,
  isShellPage,
} from "@/app/pageTransitionPolicy";
```

Add these types and constants above `export function App()`:

```ts
const TRANSIENT_EXIT_TIMEOUT_MS = 240;

type RenderedTransientPage = {
  pageId: AppPageId;
  node: ReactNode;
};
```

Inside `App()`, after the existing page-local state hooks, add:

```ts
  const previousRouteIdRef = useRef<AppPageId>(activeRouteId);
  const lastActiveTransientPageRef = useRef<RenderedTransientPage | null>(null);
  const transientExitTimeoutRef = useRef<number | null>(null);
  const [exitingTransientPage, setExitingTransientPage] =
    useState<RenderedTransientPage | null>(null);
```

Replace `const transientPage = renderTransientPage();` with:

```ts
  const activeTransitionPolicy = getPageTransitionPolicy(activeRouteId);
  const activeTransientPage = useMemo<RenderedTransientPage | null>(() => {
    if (activeTransitionPolicy.kind !== "transient") {
      return null;
    }
    return {
      pageId: activeRouteId,
      node: renderTransientPage(),
    };
  }, [activeRouteId, activeTransitionPolicy.kind]);

  const isCurrentTransientPage = activeTransitionPolicy.kind === "transient";
```

Add this effect before `const shellRouteIds = ...`:

```ts
  useEffect(() => {
    if (activeTransientPage) {
      lastActiveTransientPageRef.current = activeTransientPage;
    }
  }, [activeTransientPage]);

  useEffect(() => {
    const previousRouteId = previousRouteIdRef.current;
    const previousPolicy = getPageTransitionPolicy(previousRouteId);

    if (previousRouteId !== activeRouteId && previousPolicy.kind === "transient") {
      const previousTransientPage = lastActiveTransientPageRef.current;
      setExitingTransientPage(
        previousTransientPage?.pageId === previousRouteId ? previousTransientPage : null,
      );
    }

    previousRouteIdRef.current = activeRouteId;
  }, [activeRouteId]);

  useEffect(() => {
    if (!exitingTransientPage) {
      return;
    }

    if (transientExitTimeoutRef.current !== null) {
      window.clearTimeout(transientExitTimeoutRef.current);
    }

    transientExitTimeoutRef.current = window.setTimeout(
      handleTransientExitComplete,
      TRANSIENT_EXIT_TIMEOUT_MS,
    );

    return () => {
      if (transientExitTimeoutRef.current !== null) {
        window.clearTimeout(transientExitTimeoutRef.current);
        transientExitTimeoutRef.current = null;
      }
    };
  }, [exitingTransientPage]);

  function handleTransientExitComplete() {
    if (transientExitTimeoutRef.current !== null) {
      window.clearTimeout(transientExitTimeoutRef.current);
      transientExitTimeoutRef.current = null;
    }
    setExitingTransientPage(null);
  }
```

This ref-based outgoing page capture is required because return handlers clear ids such as `editingStationId` before switching back to a shell route. Rebuilding the outgoing page after those state updates could pass `null` props into the exiting page.

- [ ] **Step 4: Replace the App render wrapper with transition layers**

Replace the current `return (...)` block in `src/app/App.tsx` with:

```tsx
  return (
    <AppShell activeRouteId={activeShellRouteId} onRouteChange={(routeId) => setActiveRouteId(routeId)}>
      <div className="app-page-transition-stack">
        {shellRouteIds.map((routeId) => {
          const active = activeRouteId === routeId && !isCurrentTransientPage;
          const inert = !active;

          return (
            <PageActivityProvider key={routeId} active={active}>
              <div
                aria-hidden={inert}
                className="app-page-transition-layer"
                data-page-transition-layer
                data-page-transition-kind="shell"
                data-page-transition-direction="none"
                data-page-transition-state={active ? "active" : "inactive"}
                inert={inert ? "" : undefined}
              >
                {renderShellPage(routeId)}
              </div>
            </PageActivityProvider>
          );
        })}

        {activeTransientPage && (
          <PageActivityProvider active={isCurrentTransientPage}>
            <div
              aria-hidden={!isCurrentTransientPage}
              className="app-page-transition-layer app-page-transition-overlay"
              data-page-transition-layer
              data-page-transition-kind="transient"
              data-page-transition-direction={activeTransitionPolicy.enterDirection}
              data-page-transition-state="active"
              inert={!isCurrentTransientPage ? "" : undefined}
            >
              {activeTransientPage.node}
            </div>
          </PageActivityProvider>
        )}

        {exitingTransientPage && (
          <PageActivityProvider active={false}>
            <div
              aria-hidden
              className="app-page-transition-layer app-page-transition-overlay"
              data-page-transition-layer
              data-page-transition-kind="transient"
              data-page-transition-direction={
                getPageTransitionPolicy(exitingTransientPage.pageId).exitDirection
              }
              data-page-transition-state="exiting"
              inert=""
              onTransitionEnd={handleTransientExitComplete}
            >
              {exitingTransientPage.node}
            </div>
          </PageActivityProvider>
        )}
      </div>
    </AppShell>
  );
```

If TypeScript reports that `inert` is not a valid React DOM attribute, add this declaration near the top of `src/app/App.tsx` after imports:

```ts
declare module "react" {
  interface HTMLAttributes<T> {
    inert?: "" | undefined;
  }
}
```

- [ ] **Step 5: Run the container contract test and verify expected partial failure**
- [ ] **Step 5: Run the container contract test and verify GREEN**

Run:

```powershell
node scripts/page-transition-container.test.mjs
```

Expected output:

```text
page transition container contract ok
```

- [ ] **Step 6: Commit Task 2**

Run:

```powershell
git add -- scripts/page-transition-container.test.mjs src/app/App.tsx
git commit -m "feat: add page transition container"
```

---

### Task 3: Page Transition Styles

**Files:**
- Modify: `src/styles.css`
- Create: `scripts/page-transition-styles.test.mjs`

- [ ] **Step 1: Write the failing style contract test**

Create `scripts/page-transition-styles.test.mjs`:

```js
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const stylesSource = await readFile("src/styles.css", "utf8");

assert.ok(
  stylesSource.includes(".app-page-transition-stack") &&
    stylesSource.includes("[data-page-transition-layer]") &&
    stylesSource.includes('data-page-transition-state="active"') &&
    stylesSource.includes('data-page-transition-state="inactive"') &&
    stylesSource.includes('data-page-transition-state="exiting"'),
  "styles should define stack, layer, active, inactive, and exiting selectors",
);

assert.ok(
  stylesSource.includes("relayPageFadeUp") &&
    stylesSource.includes("relayTransientEnter") &&
    stylesSource.includes("relayTransientExit"),
  "styles should define shell fade-up and transient enter/exit animations",
);

assert.ok(
  stylesSource.includes("pointer-events: none") &&
    stylesSource.includes("visibility: hidden") &&
    stylesSource.includes("display: none"),
  "inactive and exiting styles should isolate hidden pages from interaction",
);

assert.ok(
  stylesSource.includes("@media (prefers-reduced-motion: reduce)") &&
    stylesSource.includes("animation-duration: 1ms") &&
    stylesSource.includes("transition-duration: 1ms"),
  "styles should reduce motion when the system requests it",
);

console.log("page transition styles contract ok");
```

- [ ] **Step 2: Run the style contract test and verify RED**

Run:

```powershell
node scripts/page-transition-styles.test.mjs
```

Expected: FAIL because `src/styles.css` does not yet define page transition selectors or animations.

- [ ] **Step 3: Add page transition CSS**

Append to `src/styles.css`:

```css
.app-page-transition-stack {
  position: relative;
  min-height: 100%;
}

.app-page-transition-layer {
  min-width: 0;
  transform: translateY(0);
  opacity: 1;
  transition:
    opacity 160ms ease-out,
    transform 160ms ease-out;
}

.app-page-transition-layer[data-page-transition-state="inactive"] {
  display: none;
  pointer-events: none;
  visibility: hidden;
}

.app-page-transition-layer[data-page-transition-state="active"] {
  display: block;
  pointer-events: auto;
  visibility: visible;
}

.app-page-transition-layer[data-page-transition-kind="shell"][data-page-transition-state="active"] {
  animation: relayPageFadeUp 160ms ease-out;
}

.app-page-transition-overlay {
  inset: 0;
  position: absolute;
  z-index: 1;
  background: hsl(var(--background));
}

.app-page-transition-overlay[data-page-transition-state="active"][data-page-transition-direction="forward"] {
  animation: relayTransientEnter 180ms ease-out;
}

.app-page-transition-overlay[data-page-transition-state="exiting"][data-page-transition-direction="back"] {
  pointer-events: none;
  animation: relayTransientExit 180ms ease-out forwards;
}

@keyframes relayPageFadeUp {
  from {
    opacity: 0;
    transform: translateY(4px);
  }

  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@keyframes relayTransientEnter {
  from {
    opacity: 0;
    transform: translateX(16px);
  }

  to {
    opacity: 1;
    transform: translateX(0);
  }
}

@keyframes relayTransientExit {
  from {
    opacity: 1;
    transform: translateX(0);
  }

  to {
    opacity: 0;
    transform: translateX(16px);
  }
}

@media (prefers-reduced-motion: reduce) {
  .app-page-transition-layer,
  .app-page-transition-overlay {
    animation-duration: 1ms !important;
    transition-duration: 1ms !important;
    transform: none !important;
  }
}
```

- [ ] **Step 4: Run the style contract test and verify GREEN**

Run:

```powershell
node scripts/page-transition-styles.test.mjs
```

Expected output:

```text
page transition styles contract ok
```

- [ ] **Step 5: Commit Task 3**

Run:

```powershell
git add -- scripts/page-transition-styles.test.mjs src/styles.css
git commit -m "feat: style page transitions"
```

---

### Task 4: Build Verification and Manual QA

**Files:**
- Modify: `docs/superpowers/plans/2026-07-11-page-transition.md`

- [ ] **Step 1: Run focused static contracts**

Run:

```powershell
node scripts/page-transition-policy.test.mjs
node scripts/page-transition-container.test.mjs
node scripts/page-transition-styles.test.mjs
node scripts/page-activation-refresh.test.mjs
node scripts/model-base-prices-page.test.mjs
```

Expected output includes:

```text
page transition policy contract ok
page transition container contract ok
page transition styles contract ok
```

`page-activation-refresh.test.mjs` and `model-base-prices-page.test.mjs` should exit with code 0.

- [ ] **Step 2: Run the frontend build**

Run:

```powershell
pnpm build
```

Expected: `tsc --noEmit` and `vite build` complete successfully.

- [ ] **Step 3: Start the dev server for manual QA**

Run:

```powershell
pnpm dev
```

Expected: Vite serves the app on `http://127.0.0.1:5173/` or the next available port.

- [ ] **Step 4: Manual QA checklist**

Open the app and verify:

- Main navigation from 总览 to 设置 uses a short fade-up transition.
- Main navigation from 设置 to 请求日志 does not move the sidebar or main shell.
- 中转站资产 to 中转站详情 uses a slight right-side push-in transition.
- Returning from 中转站详情 to 中转站资产 uses the reverse transient exit.
- 价格 / 倍率 to 模型基础价格 keeps the sidebar highlight on 价格 / 倍率.
- Rapidly click 总览, 设置, 请求日志, then 价格 / 倍率; no invisible page blocks clicks.
- Press Tab after switching pages; focus does not enter an invisible page.

- [ ] **Step 5: Commit plan checkbox updates if changed**

If task checkboxes were updated during execution, run:

```powershell
git add -- docs/superpowers/plans/2026-07-11-page-transition.md
git commit -m "docs: update page transition implementation progress"
```

---

## Self-Review

Spec coverage:

- Main page fade-up transition is implemented in Task 3.
- Internal transient slide-in and return exit are implemented in Tasks 2 and 3.
- Page instance keep-alive remains based on `mountedRouteIds` in Task 2.
- Parent sidebar highlight remains based on `getShellRouteId()` through the centralized policy in Task 1.
- Reduced motion is implemented in Task 3.
- Outgoing transient cleanup, rapid navigation cleanup, and focus isolation are covered in Task 2 and tested in Task 4.
- No new animation library, View Transition API, iframe architecture, or backend changes are introduced.

Placeholder scan:

- The plan contains no open-ended implementation markers or vague test instructions.
- Each code-changing step includes concrete file paths, code snippets, commands, and expected results.

Type consistency:

- `PageTransitionPolicy`, `PageTransitionKind`, and `PageTransitionDirection` are defined before use.
- `getPageTransitionPolicy()`, `isShellPage()`, and `getShellRouteId()` names match across tests, App imports, and policy helper.
- `TRANSIENT_EXIT_TIMEOUT_MS`, `exitingTransientPage`, and `handleTransientExitComplete` names match the contract test.
