# Navigation Performance and Page Activity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make sidebar navigation acknowledge input within one to two frames, keep the last navigation intent reliable under burst input, share live Tauri data without duplicate polling, and preserve retained page state without allowing hidden pages to refresh or rerender.

**Architecture:** Add a monotonic intent/commit navigation controller and a memoized shell-page host so only the source and target slots participate in a switch. Move shared server facts into TanStack Query, couple query subscription to page activity, and keep retained DOM/view state separate from live cached facts. Shell navigation uses concurrent preparation plus a non-blocking opacity handoff; transient pages retain their existing centralized host.

**Tech Stack:** React 18.3, TypeScript 5.7, TanStack Query 5.90, Framer Motion 12, Tauri 2, Vite 6, Node contract scripts, Playwright/browser performance sampling.

**Design:** `docs/superpowers/specs/2026-07-12-navigation-performance-and-page-activity-design.md`

---

## Execution Prerequisites

- Execute in an isolated worktree created with `superpowers:using-git-worktrees`; the current checkout contains unrelated Channel, Key Pool, Routing, and Rust edits.
- Start from a commit that contains `65aa01e` and all user-owned changes that are meant to coexist with this work.
- At the start of every task, run `git status --short` and `git diff --cached --name-only`.
- Stage only the exact paths named in that task. Never use `git add .`, `git add -A`, or `git commit -a`.
- If a fresh worktree has no dependencies, run `pnpm.cmd install --ignore-scripts --frozen-lockfile` before the first RED command.
- Do not modify `src-tauri/**`, database schema, collector behavior, proxy routing, pricing logic, or credential storage.

## Planned File Structure

### Navigation ownership

- Create `src/app/navigationController.ts`: pure intent/commit types, transition functions, and the stable React controller hook.
- Create `src/app/ShellPageHost.tsx`: retained slot lifecycle, interactive/background/inactive states, shell handoff cleanup, and render isolation.
- Create `src/app/shellPageRegistry.tsx`: typed route-to-page registry and stable page action interface.
- Create `src/app/useIdlePagePrewarm.ts`: cancellable one-page-at-a-time idle prewarming.
- Create `src/app/navigationPerformance.ts`: development-only navigation marks and measurement snapshots.
- Modify `src/app/App.tsx`: retain only business entity state and wire the four navigation modules together.

### Query ownership

- Create `src/lib/query/queryClient.ts`: one QueryClient and default failure/refocus policy.
- Create `src/lib/query/queryKeys.ts`: all shared raw-fact keys.
- Create `src/lib/query/resourceQueries.ts`: typed query options for shared Tauri reads.
- Create `src/lib/query/useActivityQuery.ts`: the only page-owned `useQuery` wrapper; controls both `enabled` and `subscribed`.
- Modify `src/main.tsx`: install `QueryClientProvider` outside the existing toast/updater providers.
- Keep feature projections in their existing feature modules; query modules return raw facts only.

### Page activity ownership

- Modify `src/components/shell/PageActivity.tsx`: expose `interactive` and `refreshEnabled`, preserve `usePageActivation` as a compatibility edge.
- Keep `src/components/ui/InteractionActivity.tsx`: portal interaction compatibility remains centralized there.
- Modify `src/components/shell/AppShell.tsx`: become the sole global consumer of settings, proxy status, and unread change facts.

### First migration pages

- Modify `src/features/dashboard/DashboardPage.tsx`.
- Modify `src/features/logs/LogsPage.tsx`.
- Modify `src/features/changes/ChangeCenterPage.tsx`.
- Modify `src/features/stations/StationsPage.tsx`.

These pages contain the measured cold-mount cost or duplicate polling that directly affects the reported symptom. Remaining shell pages migrate only after these contracts pass.

---

### Task 1: Install the shared query foundation

**Files:**
- Create: `scripts/query-client-contract.test.mjs`
- Create: `src/lib/query/queryClient.ts`
- Create: `src/lib/query/QueryErrorNotifier.tsx`
- Create: `src/lib/query/queryKeys.ts`
- Modify: `src/main.tsx:1-17`
- Modify: `package.json`
- Modify: `pnpm-lock.yaml`

- [ ] **Step 1: Write the failing query foundation contract**

```js
// scripts/query-client-contract.test.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { QueryClient } from "@tanstack/react-query";

const pkg = JSON.parse(await readFile("package.json", "utf8"));
const main = await readFile("src/main.tsx", "utf8");
const client = await readFile("src/lib/query/queryClient.ts", "utf8").catch(() => "");
const notifier = await readFile("src/lib/query/QueryErrorNotifier.tsx", "utf8").catch(() => "");
const keys = await readFile("src/lib/query/queryKeys.ts", "utf8").catch(() => "");

assert.equal(pkg.dependencies["@tanstack/react-query"], "^5.90.3");
assert.match(main, /QueryClientProvider client=\{queryClient\}/);
assert.match(client, /new QueryClient/);
assert.match(client, /refetchOnWindowFocus:\s*true/);
assert.match(client, /refetchIntervalInBackground:\s*false/);
assert.match(main, /<QueryErrorNotifier \/>/);
assert.match(notifier, /errorUpdatedAt/);
assert.match(notifier, /lastNotifiedAt/);
assert.match(notifier, /toast\.error\("数据刷新失败"/);
assert.ok(!notifier.includes("queryKey"), "failure notification must not expose query parameters");
for (const key of [
  "settings",
  "proxyStatus",
  "requestLogs",
  "stations",
  "stationAssets",
  "keyPool",
  "balanceSnapshots",
  "changeEvents",
  "localRoutingWorkspace",
  "pricing",
  "channelStatus",
]) {
  assert.ok(keys.includes(key), `queryKeys should define ${key}`);
}

const behaviorClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
let readCount = 0;
const sharedOptions = {
  queryKey: ["dedupe-test"],
  queryFn: async () => {
    readCount += 1;
    await new Promise((resolve) => setTimeout(resolve, 5));
    return "fresh";
  },
};
assert.deepEqual(
  await Promise.all([
    behaviorClient.fetchQuery(sharedOptions),
    behaviorClient.fetchQuery(sharedOptions),
  ]),
  ["fresh", "fresh"],
);
assert.equal(readCount, 1, "same-key reads should share one in-flight request");

behaviorClient.setQueryData(["last-good-test"], "last-good");
await assert.rejects(
  behaviorClient.fetchQuery({
    queryKey: ["last-good-test"],
    queryFn: async () => { throw new Error("transient failure"); },
    staleTime: 0,
  }),
  /transient failure/,
);
assert.equal(behaviorClient.getQueryData(["last-good-test"]), "last-good");
```

- [ ] **Step 2: Run the contract and verify the intended RED**

Run: `node scripts/query-client-contract.test.mjs`

Expected: FAIL because the dependency and query files do not exist.

- [ ] **Step 3: Install the pinned dependency**

Run: `pnpm.cmd add @tanstack/react-query@^5.90.3`

Expected: `package.json` and `pnpm-lock.yaml` include `@tanstack/react-query` without unrelated dependency churn.

- [ ] **Step 4: Add the QueryClient**

```ts
// src/lib/query/queryClient.ts
import { QueryClient } from "@tanstack/react-query";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 0,
      refetchOnWindowFocus: true,
      refetchIntervalInBackground: false,
    },
    mutations: {
      retry: false,
    },
  },
});
```

- [ ] **Step 5: Add centralized query keys**

```ts
// src/lib/query/queryKeys.ts
export const queryKeys = {
  settings: ["settings"] as const,
  proxyStatus: ["proxyStatus"] as const,
  requestLogs: ["requestLogs"] as const,
  stations: ["stations"] as const,
  stationAssets: ["stationAssets"] as const,
  stationAsset: (stationId: string) => ["stationAssets", stationId] as const,
  keyPool: ["keyPool"] as const,
  balanceSnapshots: ["balanceSnapshots"] as const,
  changeEvents: ["changeEvents"] as const,
  localRoutingWorkspace: ["localRoutingWorkspace"] as const,
  pricing: ["pricing"] as const,
  channelStatus: ["channelStatus"] as const,
} as const;
```

- [ ] **Step 6: Install the provider without changing existing provider order**

First add a single notification bridge inside both providers. It subscribes to QueryCache updates, deduplicates by `queryHash + errorUpdatedAt`, and emits only a generic sanitized message. Page-local deterministic errors still render in their existing inline error surface; this bridge covers background/global reads without exposing query keys, station IDs, credentials, or raw backend messages.

```tsx
// src/lib/query/QueryErrorNotifier.tsx
import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useToast } from "@/components/ui";

export function QueryErrorNotifier() {
  const queryClient = useQueryClient();
  const toast = useToast();
  const lastNotifiedAt = useRef(new Map<string, number>());

  useEffect(() => queryClient.getQueryCache().subscribe((event) => {
    if (event.type !== "updated" || event.action.type !== "error") return;
    const { errorUpdatedAt } = event.query.state;
    if (lastNotifiedAt.current.get(event.query.queryHash) === errorUpdatedAt) return;
    lastNotifiedAt.current.set(event.query.queryHash, errorUpdatedAt);
    toast.error("数据刷新失败", "已保留最近一次成功数据，请稍后重试。");
  }), [queryClient, toast]);

  return null;
}
```

Then install the provider without changing existing provider order:

```tsx
// src/main.tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { App } from "@/app/App";
import { ToastProvider } from "@/components/ui";
import { UpdaterProvider } from "@/features/updater/UpdaterProvider";
import { installDesktopWebViewGuards } from "@/lib/desktopGuards";
import { QueryErrorNotifier } from "@/lib/query/QueryErrorNotifier";
import { queryClient } from "@/lib/query/queryClient";
import "@/styles.css";

installDesktopWebViewGuards();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <QueryErrorNotifier />
        <UpdaterProvider>
          <App />
        </UpdaterProvider>
      </ToastProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);
```

- [ ] **Step 7: Run GREEN verification**

Run: `node scripts/query-client-contract.test.mjs`

Expected: PASS.

Run: `pnpm.cmd build`

Expected: TypeScript and Vite succeed; only the existing large-chunk warning is acceptable.

- [ ] **Step 8: Commit the query foundation**

```powershell
git add -- package.json pnpm-lock.yaml src/main.tsx src/lib/query/QueryErrorNotifier.tsx src/lib/query/queryClient.ts src/lib/query/queryKeys.ts scripts/query-client-contract.test.mjs
git commit -m "feat: add shared query foundation"
```

---

### Task 2: Make page activity control query subscription

**Files:**
- Create: `scripts/page-activity-query-contract.test.mjs`
- Create: `src/app/navigationPerformance.ts`
- Create: `src/lib/query/useActivityQuery.ts`
- Modify: `src/components/shell/PageActivity.tsx:1-31`
- Modify: `scripts/page-activation-refresh.test.mjs`

- [ ] **Step 1: Write the failing activity contract**

```js
// scripts/page-activity-query-contract.test.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const activity = await readFile("src/components/shell/PageActivity.tsx", "utf8");
const query = await readFile("src/lib/query/useActivityQuery.ts", "utf8").catch(() => "");

assert.match(activity, /type PageActivity = \{/);
assert.match(activity, /interactive: boolean/);
assert.match(activity, /refreshEnabled: boolean/);
assert.match(activity, /export function usePageActivity/);
assert.match(query, /enabled:\s*queryEnabled/);
assert.match(query, /subscribed:\s*active/);
assert.match(query, /recordHiddenPageQueryStart/);
assert.ok(!query.includes("setInterval"));
```

- [ ] **Step 2: Run RED**

Run: `node scripts/page-activity-query-contract.test.mjs`

Expected: FAIL because the richer activity context and query wrapper are absent.

- [ ] **Step 3: Replace PageActivity with the explicit two-axis contract**

```tsx
// src/components/shell/PageActivity.tsx
import { createContext, useContext, useEffect, useMemo, useRef, type ReactNode } from "react";
import {
  InteractionActivityProvider,
  useInteractionActivity,
} from "@/components/ui/InteractionActivity";

export type PageActivity = {
  interactive: boolean;
  refreshEnabled: boolean;
};

type PageActivation = {
  isInitial: boolean;
};

const PageActivityContext = createContext<PageActivity>({
  interactive: true,
  refreshEnabled: true,
});

export function PageActivityProvider({ active, children }: { active: boolean; children: ReactNode }) {
  const value = useMemo<PageActivity>(
    () => ({ interactive: active, refreshEnabled: active }),
    [active],
  );

  return (
    <PageActivityContext.Provider value={value}>
      <InteractionActivityProvider active={active}>{children}</InteractionActivityProvider>
    </PageActivityContext.Provider>
  );
}

export function usePageActivity() {
  return useContext(PageActivityContext);
}

export function usePageActivation(onActivate: (activation: PageActivation) => void) {
  const { refreshEnabled } = usePageActivity();
  const interactive = useInteractionActivity();
  const onActivateRef = useRef(onActivate);
  const wasActiveRef = useRef(false);
  const hasActivatedRef = useRef(false);

  onActivateRef.current = onActivate;

  useEffect(() => {
    const active = interactive && refreshEnabled;
    if (active && !wasActiveRef.current) {
      onActivateRef.current({ isInitial: !hasActivatedRef.current });
      hasActivatedRef.current = true;
    }
    wasActiveRef.current = active;
  }, [interactive, refreshEnabled]);
}
```

- [ ] **Step 4: Add the only page-owned query wrapper**

```ts
// src/lib/query/useActivityQuery.ts
import { useEffect } from "react";
import {
  useQuery,
  type DefaultError,
  type QueryKey,
  type UseQueryOptions,
  type UseQueryResult,
} from "@tanstack/react-query";
import { recordHiddenPageQueryStart } from "@/app/navigationPerformance";

type ActivityQueryOptions<
  TQueryFnData,
  TError,
  TData,
  TQueryKey extends QueryKey,
> = Omit<UseQueryOptions<TQueryFnData, TError, TData, TQueryKey>, "enabled" | "subscribed"> & {
  enabled?: boolean;
};

export function useActivityQuery<
  TQueryFnData,
  TError = DefaultError,
  TData = TQueryFnData,
  TQueryKey extends QueryKey = QueryKey,
>(
  active: boolean,
  options: ActivityQueryOptions<TQueryFnData, TError, TData, TQueryKey>,
): UseQueryResult<TData, TError> {
  const queryEnabled = active && options.enabled !== false;
  const result = useQuery({
    ...options,
    enabled: queryEnabled,
    subscribed: active,
  });

  useEffect(() => {
    if (!active && result.fetchStatus === "fetching") recordHiddenPageQueryStart();
  }, [active, result.fetchStatus]);

  return result;
}
```

Add the development-only counter/snapshot API. Production recording is a no-op and no query key or route payload is recorded.

```ts
// src/app/navigationPerformance.ts
const enabled = import.meta.env.DEV;
let hiddenPageQueryStarts = 0;

export type NavigationPerformanceSnapshot = {
  hiddenPageQueryStarts: number;
};

export function recordHiddenPageQueryStart() {
  if (enabled) hiddenPageQueryStarts += 1;
}

export function getNavigationPerformanceSnapshot(): NavigationPerformanceSnapshot {
  return { hiddenPageQueryStarts };
}

declare global {
  interface Window {
    __relayNavigationPerformance?: {
      snapshot: typeof getNavigationPerformanceSnapshot;
    };
  }
}

if (enabled && typeof window !== "undefined") {
  window.__relayNavigationPerformance = { snapshot: getNavigationPerformanceSnapshot };
}
```

- [ ] **Step 5: Update the activation regression script**

Add assertions that `PageActivityProvider` exposes both axes and that inactive query consumers set both `enabled` and `subscribed` false. Keep the existing first-entry and inactive-to-active assertions.

- [ ] **Step 6: Run GREEN verification**

Run: `node scripts/page-activity-query-contract.test.mjs`

Expected: PASS.

Run: `node scripts/page-activation-refresh.test.mjs`

Expected: PASS.

Run: `pnpm.cmd build`

Expected: PASS.

- [ ] **Step 7: Commit activity/query subscription ownership**

```powershell
git add -- src/app/navigationPerformance.ts src/components/shell/PageActivity.tsx src/lib/query/useActivityQuery.ts scripts/page-activity-query-contract.test.mjs scripts/page-activation-refresh.test.mjs
git commit -m "feat: bind queries to page activity"
```

---

### Task 3: Move Shell global facts into one query owner

**Files:**
- Create: `scripts/shell-query-cache.test.mjs`
- Create: `src/lib/query/resourceQueries.ts`
- Modify: `src/components/shell/AppShell.tsx:15-100`

- [ ] **Step 1: Write the failing Shell ownership contract**

```js
// scripts/shell-query-cache.test.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const shell = await readFile("src/components/shell/AppShell.tsx", "utf8");
const resources = await readFile("src/lib/query/resourceQueries.ts", "utf8").catch(() => "");

assert.match(resources, /settingsQueryOptions/);
assert.match(resources, /proxyStatusQueryOptions/);
assert.match(resources, /changeEventsQueryOptions/);
assert.match(shell, /useQueryClient/);
assert.match(shell, /useQuery\(settingsQueryOptions\(\)\)/);
assert.match(shell, /useQuery\(proxyStatusQueryOptions\(2_000\)\)/);
assert.match(shell, /useQuery\(changeEventsQueryOptions\(10_000\)\)/);
assert.ok(!shell.includes("window.setInterval"));
assert.ok(!shell.includes("useState<ProxyStatus"));
assert.ok(!shell.includes("useState<ChangeEvent"));
```

- [ ] **Step 2: Run RED**

Run: `node scripts/shell-query-cache.test.mjs`

Expected: FAIL because AppShell owns local caches and intervals.

- [ ] **Step 3: Add typed raw-fact query options**

```ts
// src/lib/query/resourceQueries.ts
import { queryOptions } from "@tanstack/react-query";
import { listChangeEvents } from "@/lib/api/changeEvents";
import { listBalanceSnapshots } from "@/lib/api/economics";
import { getProxyStatus, listRequestLogs } from "@/lib/api/proxy";
import { getSettings } from "@/lib/api/settings";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import { queryKeys } from "@/lib/query/queryKeys";

export const settingsQueryOptions = () =>
  queryOptions({
    queryKey: queryKeys.settings,
    queryFn: getSettings,
    staleTime: 60_000,
  });

export const proxyStatusQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.proxyStatus,
    queryFn: getProxyStatus,
    staleTime: 1_000,
    refetchInterval,
  });

export const requestLogsQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.requestLogs,
    queryFn: listRequestLogs,
    staleTime: 2_000,
    refetchInterval,
  });

export const stationsQueryOptions = () =>
  queryOptions({
    queryKey: queryKeys.stations,
    queryFn: listStations,
    staleTime: 30_000,
  });

export const keyPoolQueryOptions = () =>
  queryOptions({
    queryKey: queryKeys.keyPool,
    queryFn: listKeyPoolItems,
    staleTime: 10_000,
  });

export const balanceSnapshotsQueryOptions = () =>
  queryOptions({
    queryKey: queryKeys.balanceSnapshots,
    queryFn: listBalanceSnapshots,
    staleTime: 30_000,
  });

export const changeEventsQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.changeEvents,
    queryFn: listChangeEvents,
    staleTime: 10_000,
    refetchInterval,
  });
```

- [ ] **Step 4: Replace AppShell local caches and timers**

Use plain `useQuery` because these are Shell-global subscriptions:

```tsx
const queryClient = useQueryClient();
const { data: settings = null } = useQuery(settingsQueryOptions());
const { data: proxyStatus = null } = useQuery(proxyStatusQueryOptions(2_000));
const { data: changeEvents = [] } = useQuery(changeEventsQueryOptions(10_000));

useEffect(() => {
  const handleProxyStatusUpdated = (event: Event) => {
    queryClient.setQueryData(
      queryKeys.proxyStatus,
      (event as CustomEvent<ProxyStatus>).detail,
    );
  };
  const handleSettingsUpdated = (event: Event) => {
    const next = (event as CustomEvent<AppSettings>).detail;
    if (next) {
      queryClient.setQueryData(queryKeys.settings, next);
      return;
    }
    void queryClient.invalidateQueries({ queryKey: queryKeys.settings });
  };
  const handleChangeEventsUpdated = () => {
    void queryClient.invalidateQueries({ queryKey: queryKeys.changeEvents });
  };

  window.addEventListener(PROXY_STATUS_UPDATED_EVENT, handleProxyStatusUpdated);
  window.addEventListener(SETTINGS_UPDATED_EVENT, handleSettingsUpdated);
  window.addEventListener(CHANGE_EVENTS_UPDATED_EVENT, handleChangeEventsUpdated);
  return () => {
    window.removeEventListener(PROXY_STATUS_UPDATED_EVENT, handleProxyStatusUpdated);
    window.removeEventListener(SETTINGS_UPDATED_EVENT, handleSettingsUpdated);
    window.removeEventListener(CHANGE_EVENTS_UPDATED_EVENT, handleChangeEventsUpdated);
  };
}, [queryClient]);
```

Delete the three local `useState` declarations and all raw `setInterval` effects. Preserve `visibleRoutes`, collector fallback navigation, badge projection, and existing JSX.

- [ ] **Step 5: Run GREEN and existing sidebar regressions**

Run: `node scripts/shell-query-cache.test.mjs`

Expected: PASS.

Run: `node scripts/sidebar-local-proxy-status.test.mjs`

Expected: PASS.

Run: `pnpm.cmd build`

Expected: PASS.

- [ ] **Step 6: Commit Shell query ownership**

```powershell
git add -- src/components/shell/AppShell.tsx src/lib/query/resourceQueries.ts scripts/shell-query-cache.test.mjs
git commit -m "refactor: centralize shell live facts"
```

---

### Task 4: Migrate Dashboard off duplicate workspace state and polling

**Files:**
- Create: `scripts/dashboard-shared-query.test.mjs`
- Modify: `src/features/dashboard/DashboardPage.tsx:81-136`
- Modify: `scripts/dashboard-query-service.test.mjs`
- Modify: `scripts/dashboard-balance-refresh.test.mjs`

- [ ] **Step 1: Write the failing Dashboard query contract**

```js
// scripts/dashboard-shared-query.test.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

for (const option of [
  "proxyStatusQueryOptions",
  "requestLogsQueryOptions",
  "keyPoolQueryOptions",
  "stationsQueryOptions",
  "balanceSnapshotsQueryOptions",
  "settingsQueryOptions",
  "changeEventsQueryOptions",
]) {
  assert.ok(source.includes(option), `Dashboard should consume ${option}`);
}
assert.match(source, /usePageActivity\(\)/);
assert.match(source, /useActivityQuery/);
assert.ok(!source.includes("loadDashboardWorkspace"));
assert.ok(!source.includes("window.setInterval"));
assert.ok(!source.includes("setProxyStatus"));
assert.ok(!source.includes("setRequestLogs"));
```

- [ ] **Step 2: Run RED**

Run: `node scripts/dashboard-shared-query.test.mjs`

Expected: FAIL because Dashboard still owns a composite workspace and timers.

- [ ] **Step 3: Replace server-fact state with activity-aware queries**

At the top of `DashboardPage`, retain only action flags and add:

```tsx
const queryClient = useQueryClient();
const { refreshEnabled } = usePageActivity();
const proxyStatusQuery = useActivityQuery(
  refreshEnabled,
  proxyStatusQueryOptions(false),
);
const requestLogsQuery = useActivityQuery(
  refreshEnabled,
  requestLogsQueryOptions(proxyStatusQuery.data?.running ? 2_000 : false),
);
const keyPoolQuery = useActivityQuery(refreshEnabled, keyPoolQueryOptions());
const stationsQuery = useActivityQuery(refreshEnabled, stationsQueryOptions());
const balancesQuery = useActivityQuery(refreshEnabled, balanceSnapshotsQueryOptions());
const settingsQuery = useActivityQuery(refreshEnabled, settingsQueryOptions());
const changeEventsQuery = useActivityQuery(refreshEnabled, changeEventsQueryOptions(false));

const proxyStatus = proxyStatusQuery.data ?? null;
const requestLogs = requestLogsQuery.data ?? [];
const keyPoolItems = keyPoolQuery.data ?? [];
const stations = stationsQuery.data ?? [];
const balanceSnapshots = balancesQuery.data ?? [];
const settings = settingsQuery.data ?? null;
const changeEvents = changeEventsQuery.data ?? [];
const dashboardLoaded = [
  proxyStatusQuery.data,
  requestLogsQuery.data,
  keyPoolQuery.data,
  stationsQuery.data,
  balancesQuery.data,
  settingsQuery.data,
  changeEventsQuery.data,
].every((value) => value !== undefined);
```

Delete the server-fact `useState` declarations, `loadDashboardWorkspace()` activation effect, `refreshDashboardRuntimeFacts`, and both interval effects.

- [ ] **Step 4: Update proxy actions to write the shared cache**

Before start/stop/restart, cancel the stale status read; after success replace local setters with:

```ts
await queryClient.cancelQueries({ queryKey: queryKeys.proxyStatus });
const nextStatus = await startLocalProxy(); // use the matching existing API for each action
queryClient.setQueryData(queryKeys.proxyStatus, nextStatus);
```

Keep the API functions' existing immediate DOM event as a second consumer path. Do not duplicate settings, logs, balances, or change events into local state.

- [ ] **Step 5: Update old query-service tests to the new ownership**

Change `dashboard-query-service.test.mjs` to assert that `DashboardPage` no longer imports `loadDashboardWorkspace`. Keep `dashboardQueries.ts` temporarily for compatibility until Task 11 removes unused workspace loaders. Change the balance refresh test to assert `balanceSnapshotsQueryOptions` plus active subscription instead of a raw interval.

- [ ] **Step 6: Run Dashboard GREEN verification**

Run:

```powershell
node scripts/dashboard-shared-query.test.mjs
node scripts/dashboard-query-service.test.mjs
node scripts/dashboard-balance-refresh.test.mjs
node scripts/dashboard-performance-metrics.test.mjs
node scripts/dashboard-request-count-source.test.mjs
pnpm.cmd build
```

Expected: all scripts and build PASS.

- [ ] **Step 7: Commit Dashboard migration**

```powershell
git add -- src/features/dashboard/DashboardPage.tsx scripts/dashboard-shared-query.test.mjs scripts/dashboard-query-service.test.mjs scripts/dashboard-balance-refresh.test.mjs
git commit -m "refactor: share dashboard live queries"
```

---

### Task 5: Migrate Logs and Change Center to shared cache

**Files:**
- Create: `scripts/log-change-shared-query.test.mjs`
- Modify: `src/features/logs/LogsPage.tsx:39-100`
- Modify: `src/features/changes/ChangeCenterPage.tsx:36-77`
- Modify: `scripts/log-query-service.test.mjs`
- Modify: `scripts/change-query-service.test.mjs`

- [ ] **Step 1: Write the failing shared-page contract**

```js
// scripts/log-change-shared-query.test.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const logs = await readFile("src/features/logs/LogsPage.tsx", "utf8");
const changes = await readFile("src/features/changes/ChangeCenterPage.tsx", "utf8");

assert.match(logs, /requestLogsQueryOptions/);
assert.match(logs, /keyPoolQueryOptions/);
assert.match(logs, /settingsQueryOptions/);
assert.match(logs, /proxyStatusQueryOptions/);
assert.ok(!logs.includes("loadRequestLogWorkspace"));
assert.ok(!logs.includes("setLogs("));

assert.match(changes, /changeEventsQueryOptions/);
assert.match(changes, /stationsQueryOptions/);
assert.match(changes, /queryClient\.setQueryData\(queryKeys\.changeEvents/);
assert.ok(!changes.includes("loadChangeCenterWorkspace"));
assert.ok(!changes.includes("setEvents("));
```

- [ ] **Step 2: Run RED**

Run: `node scripts/log-change-shared-query.test.mjs`

Expected: FAIL on both pages' local workspace ownership.

- [ ] **Step 3: Migrate Logs reads and clear mutation**

Use `usePageActivity` plus three `useActivityQuery` calls:

```tsx
const queryClient = useQueryClient();
const { refreshEnabled } = usePageActivity();
const proxyStatusQuery = useActivityQuery(refreshEnabled, proxyStatusQueryOptions(false));
const logsQuery = useActivityQuery(
  refreshEnabled,
  requestLogsQueryOptions(proxyStatusQuery.data?.running ? 2_000 : false),
);
const keysQuery = useActivityQuery(refreshEnabled, keyPoolQueryOptions());
const settingsQuery = useActivityQuery(refreshEnabled, settingsQueryOptions());

const logs = logsQuery.data ?? [];
const keys = keysQuery.data ?? [];
const developerModeEnabled = settingsQuery.data?.developerModeEnabled ?? false;
const loading = logsQuery.isPending && logsQuery.data === undefined;
const error = logsQuery.error ? readError(logsQuery.error) : null;
```

The refresh command becomes:

```ts
async function refreshLogs(showSuccess = false) {
  await Promise.all([
    queryClient.refetchQueries({ queryKey: queryKeys.requestLogs, type: "active" }),
    queryClient.refetchQueries({ queryKey: queryKeys.keyPool, type: "active" }),
    queryClient.refetchQueries({ queryKey: queryKeys.settings, type: "active" }),
  ]);
  if (showSuccess) toast.success("使用记录已刷新");
}
```

After `clearRequestLogs()` succeeds:

```ts
await queryClient.cancelQueries({ queryKey: queryKeys.requestLogs });
await clearRequestLogs();
queryClient.setQueryData(queryKeys.requestLogs, []);
```

Keep filter, pagination, selected ID, confirmation, and action state local.

- [ ] **Step 4: Migrate Change Center reads and write-through updates**

```tsx
const queryClient = useQueryClient();
const { refreshEnabled } = usePageActivity();
const eventsQuery = useActivityQuery(refreshEnabled, changeEventsQueryOptions(false));
const stationsQuery = useActivityQuery(refreshEnabled, stationsQueryOptions());

const events = eventsQuery.data ?? [];
const stationNamesById = useMemo(
  () => new Map((stationsQuery.data ?? []).map((station) => [station.id, station.name])),
  [stationsQuery.data],
);
const loading = eventsQuery.isPending && eventsQuery.data === undefined;
const error = eventsQuery.error ? readError(eventsQuery.error) : null;
```

Keep `usePageActivation` only for the mark-read-on-entry write. Before mark/read/dismiss/clear, cancel `queryKeys.changeEvents`; after each result, call `queryClient.setQueryData(queryKeys.changeEvents, nextEvents)` and then `notifyChangeEventsUpdated()`. Do not refetch the full workspace before updating visible state.

- [ ] **Step 5: Update the legacy query-service assertions**

Change the two scripts to assert that pages use resource query options and no longer consume `load*Workspace`. Leave the raw loader files until the inventory cleanup task.

- [ ] **Step 6: Run GREEN verification**

Run:

```powershell
node scripts/log-change-shared-query.test.mjs
node scripts/log-query-service.test.mjs
node scripts/change-query-service.test.mjs
node scripts/page-activation-refresh.test.mjs
pnpm.cmd build
```

Expected: PASS.

- [ ] **Step 7: Commit the two page migrations**

```powershell
git add -- src/features/logs/LogsPage.tsx src/features/changes/ChangeCenterPage.tsx scripts/log-change-shared-query.test.mjs scripts/log-query-service.test.mjs scripts/change-query-service.test.mjs
git commit -m "refactor: share logs and change queries"
```

---

### Task 6: Stop hidden Stations work and share station facts

**Files:**
- Create: `scripts/stations-page-activity-query.test.mjs`
- Create: `src/lib/query/withQueryTimeout.ts`
- Modify: `src/lib/query/queryKeys.ts`
- Modify: `src/lib/query/resourceQueries.ts`
- Modify: `src/features/stations/StationsPage.tsx:144-368`
- Modify: `scripts/page-activation-refresh.test.mjs`

- [ ] **Step 1: Write the failing Stations activity contract**

```js
// scripts/stations-page-activity-query.test.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/StationsPage.tsx", "utf8");
const resources = await readFile("src/lib/query/resourceQueries.ts", "utf8");

assert.match(source, /stationsQueryOptions/);
assert.match(source, /balanceSnapshotsQueryOptions/);
assert.match(source, /changeEventsQueryOptions/);
assert.match(source, /useQueries/);
assert.match(resources, /stationAssetQueryOptions/);
assert.match(resources, /withQueryTimeout/);
assert.ok(!source.includes("STATION_ASSET_REFRESH_INTERVAL_MS"));
assert.ok(!source.includes("window.setInterval"));
assert.ok(!source.includes("refreshStationAssetEnrichment"));
```

- [ ] **Step 2: Run RED**

Run: `node scripts/stations-page-activity-query.test.mjs`

Expected: FAIL because Stations owns an unconditional interval and enrichment state machine.

- [ ] **Step 3: Preserve the existing station timeout as a query helper**

```ts
// src/lib/query/withQueryTimeout.ts
export function withQueryTimeout<T>(
  promise: Promise<T>,
  label: string,
  timeoutMs: number,
): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    let settled = false;
    const timeoutId = globalThis.setTimeout(() => {
      if (settled) return;
      settled = true;
      reject(new Error(`${label} timed out after ${timeoutMs}ms`));
    }, timeoutMs);

    promise.then(
      (value) => {
        if (settled) return;
        settled = true;
        globalThis.clearTimeout(timeoutId);
        resolve(value);
      },
      (error) => {
        if (settled) return;
        settled = true;
        globalThis.clearTimeout(timeoutId);
        reject(error);
      },
    );
  });
}
```

- [ ] **Step 4: Add per-station asset query options**

Add the latest snapshot import and option:

```ts
export const stationAssetQueryOptions = (stationId: string) =>
  queryOptions({
    queryKey: queryKeys.stationAsset(stationId),
    queryFn: () => withQueryTimeout(
      getLatestCollectorSnapshot(stationId),
      `station asset snapshot ${stationId}`,
      6_000,
    ),
    staleTime: 30_000,
  });
```

Import `getLatestCollectorSnapshot` and `withQueryTimeout` directly in `resourceQueries.ts`. Add an executable fake-timer assertion to `stations-page-activity-query.test.mjs` that a never-settling promise rejects after the configured duration and that a resolved promise clears its timer.

- [ ] **Step 5: Replace shared station fact state**

```tsx
const { refreshEnabled } = usePageActivity();
const stationsQuery = useActivityQuery(refreshEnabled, stationsQueryOptions());
const balancesQuery = useActivityQuery(refreshEnabled, balanceSnapshotsQueryOptions());
const changesQuery = useActivityQuery(refreshEnabled, changeEventsQueryOptions(false));
const stations = stationsQuery.data ?? [];
const balanceSnapshots = balancesQuery.data ?? [];
const changeEvents = changesQuery.data ?? [];
const loading = stationsQuery.isPending && stationsQuery.data === undefined;
const error = stationsQuery.error ? readError(stationsQuery.error) : null;

const stationAssetQueries = useQueries({
  queries: stations.map((station) => ({
    ...stationAssetQueryOptions(station.id),
    enabled: refreshEnabled,
    subscribed: refreshEnabled,
  })),
});

const assetSnapshotsByStation = useMemo(
  () => new Map(
    stations.map((station, index) => [
      station.id,
      stationAssetQueries[index]?.data ?? null,
    ]),
  ),
  [stationAssetQueries, stations],
);
```

Delete the unconditional interval, `stationAssetRefreshSequence`, `refreshStations`, `refreshStationAssetEnrichment`, and local shared-fact setters. Preserve dialog credentials, key editing, drag state, and `refreshExtras`; those are page-local or sensitive view state.

- [ ] **Step 6: Invalidate exact facts after station writes**

Before create/update/delete/reorder/collect actions, cancel reads for the keys the mutation can change. After success, invalidate only those keys:

```ts
await Promise.all([
  queryClient.cancelQueries({ queryKey: queryKeys.stations }),
  queryClient.cancelQueries({ queryKey: queryKeys.balanceSnapshots }),
  queryClient.cancelQueries({ queryKey: queryKeys.stationAssets }),
]);

await performExistingStationMutation();

await Promise.all([
  queryClient.invalidateQueries({ queryKey: queryKeys.stations }),
  queryClient.invalidateQueries({ queryKey: queryKeys.balanceSnapshots }),
  queryClient.invalidateQueries({ queryKey: queryKeys.stationAssets }),
]);
```

Do not call a page-wide `refreshStations()` function.

- [ ] **Step 7: Run GREEN verification**

Run:

```powershell
node scripts/stations-page-activity-query.test.mjs
node scripts/page-activation-refresh.test.mjs
node scripts/station-asset-loading-boundary.test.mjs
node scripts/station-assets-current-projections.test.mjs
pnpm.cmd build
```

Expected: PASS.

- [ ] **Step 8: Commit Stations activity ownership**

```powershell
git add -- src/lib/query/queryKeys.ts src/lib/query/resourceQueries.ts src/lib/query/withQueryTimeout.ts src/features/stations/StationsPage.tsx scripts/stations-page-activity-query.test.mjs scripts/page-activation-refresh.test.mjs
git commit -m "refactor: pause hidden station work"
```

---

### Task 7: Add the monotonic navigation controller

**Files:**
- Create: `scripts/navigation-controller.test.mjs`
- Create: `src/app/navigationPolicy.ts`
- Create: `src/app/navigationController.ts`

- [ ] **Step 1: Write executable navigation policy tests**

The test imports only pure exported functions; keep React-specific code out of the assertions.

```js
// scripts/navigation-controller.test.mjs
import assert from "node:assert/strict";
import {
  commitNavigationIntent,
  createInitialNavigationIntent,
  createNavigationIntent,
} from "../src/app/navigationPolicy.ts";

const initial = createInitialNavigationIntent("dashboard");
const stations = createNavigationIntent("stations", "stations", null, 1);
const logs = createNavigationIntent("logs", "logs", null, 2);

assert.equal(stations.shellRouteId, "stations");
assert.equal(logs.sequence, 2);

const committed = {
  activeRouteId: "dashboard",
  previousRouteId: null,
  transientParentRouteId: null,
};
assert.equal(commitNavigationIntent(committed, stations, 2), committed);
assert.equal(commitNavigationIntent(committed, logs, 2).activeRouteId, "logs");

const detail = createNavigationIntent("stationDetail", "stations", "stations", 3);
const edit = createNavigationIntent("editProvider", "stations", "stations", 4);
assert.equal(detail.transientParentRouteId, "stations");
assert.equal(edit.transientParentRouteId, "stations");
assert.equal(edit.shellRouteId, "stations");
```

- [ ] **Step 2: Run RED**

Run: `node scripts/navigation-controller.test.mjs`

Expected: FAIL because the module does not exist.

- [ ] **Step 3: Add pure navigation intent functions without runtime path aliases**

```ts
// src/app/navigationPolicy.ts
import type { AppPageId, AppRouteId } from "@/lib/types/navigation";

export type CommittedNavigation = {
  activeRouteId: AppPageId;
  previousRouteId: AppPageId | null;
  transientParentRouteId: AppRouteId | null;
};

export type NavigationIntent = {
  routeId: AppPageId;
  shellRouteId: AppRouteId;
  transientParentRouteId: AppRouteId | null;
  sequence: number;
};

export function createInitialNavigationIntent(routeId: AppRouteId): NavigationIntent {
  return {
    routeId,
    shellRouteId: routeId,
    transientParentRouteId: null,
    sequence: 0,
  };
}

export function createNavigationIntent(
  routeId: AppPageId,
  shellRouteId: AppRouteId,
  transientParentRouteId: AppRouteId | null,
  sequence: number,
): NavigationIntent {
  return {
    routeId,
    shellRouteId,
    transientParentRouteId,
    sequence,
  };
}

export function commitNavigationIntent(
  current: CommittedNavigation,
  intent: NavigationIntent,
  latestSequence: number,
): CommittedNavigation {
  if (intent.sequence !== latestSequence || current.activeRouteId === intent.routeId) {
    return current;
  }
  return {
    activeRouteId: intent.routeId,
    previousRouteId: current.activeRouteId,
    transientParentRouteId: intent.transientParentRouteId,
  };
}
```

- [ ] **Step 4: Add the stable React controller around the pure policy**

```ts
// src/app/navigationController.ts
import { useCallback, useRef, useState, useTransition } from "react";
import {
  resolveActiveShellRouteId,
  resolveTransientParentRouteId,
} from "@/app/pageTransitionPolicy";
import {
  commitNavigationIntent,
  createInitialNavigationIntent,
  createNavigationIntent,
  type CommittedNavigation,
} from "@/app/navigationPolicy";
import type { AppPageId, AppRouteId } from "@/lib/types/navigation";

export function useNavigationController(initialRouteId: AppRouteId) {
  const initialIntent = createInitialNavigationIntent(initialRouteId);
  const [intent, setIntent] = useState(initialIntent);
  const [committed, setCommitted] = useState<CommittedNavigation>({
    activeRouteId: initialRouteId,
    previousRouteId: null,
    transientParentRouteId: null,
  });
  const [pending, startTransition] = useTransition();
  const intentRef = useRef(initialIntent);
  const sequenceRef = useRef(0);

  const navigate = useCallback((routeId: AppPageId) => {
    const sequence = sequenceRef.current + 1;
    sequenceRef.current = sequence;
    const transientParentRouteId = resolveTransientParentRouteId(
      intentRef.current.routeId,
      routeId,
      intentRef.current.transientParentRouteId,
    );
    const nextIntent = createNavigationIntent(
      routeId,
      resolveActiveShellRouteId(routeId, transientParentRouteId),
      transientParentRouteId,
      sequence,
    );
    intentRef.current = nextIntent;
    setIntent(nextIntent);
    startTransition(() => {
      setCommitted((current) =>
        commitNavigationIntent(current, nextIntent, sequenceRef.current),
      );
    });
  }, []);

  return { intent, committed, pending, navigate };
}
```

- [ ] **Step 5: Run RED/GREEN policy verification**

Run: `node scripts/navigation-controller.test.mjs`

Expected: PASS. `navigationPolicy.ts` contains no runtime imports; its type-only alias import is removed by Node's TypeScript stripping before module resolution.

Run: `pnpm.cmd build`

Expected: PASS.

- [ ] **Step 6: Commit the controller in isolation**

```powershell
git add -- src/app/navigationPolicy.ts src/app/navigationController.ts scripts/navigation-controller.test.mjs
git commit -m "feat: add latest-intent navigation controller"
```

---

### Task 8: Extract a memoized shell page registry and host

**Files:**
- Create: `scripts/shell-page-render-isolation.test.mjs`
- Create: `src/app/ShellPageErrorBoundary.tsx`
- Create: `src/app/shellPageRegistry.tsx`
- Create: `src/app/ShellPageHost.tsx`
- Modify: `src/app/App.tsx:183-219, 302-341`
- Modify: `scripts/page-transition-container.test.mjs`
- Modify: `scripts/page-activation-refresh.test.mjs`

- [ ] **Step 1: Write the failing render-isolation contract**

```js
// scripts/shell-page-render-isolation.test.mjs
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
```

- [ ] **Step 2: Run RED**

Run: `node scripts/shell-page-render-isolation.test.mjs`

Expected: FAIL because page construction remains inline in App.

- [ ] **Step 3: Add a typed stable page registry**

`ShellPageActions` must contain exactly the callbacks currently passed from App:

```tsx
export type ShellPageActions = {
  addProvider: () => void;
  editProvider: (stationId: string) => void;
  openStation: (station: Station) => void;
  addKey: (stationId: string | null) => void;
  editKey: (stationKeyId: string) => void;
  openModelBasePrices: () => void;
};

export const ShellPageContent = memo(function ShellPageContent({
  routeId,
  actions,
}: {
  routeId: AppRouteId;
  actions: ShellPageActions;
}) {
  switch (routeId) {
    case "stations":
      return <StationsPage onAddProvider={actions.addProvider} onEditProvider={actions.editProvider} onOpenStation={actions.openStation} />;
    case "keyPool":
      return <KeyPoolPage onAddKey={actions.addKey} onEditKey={actions.editKey} />;
    case "channels":
      return <ChannelStatusPage />;
    case "collectors":
      return <CollectorsPage />;
    case "changes":
      return <ChangeCenterPage />;
    case "pricing":
      return <PricingPage onOpenModelBasePrices={actions.openModelBasePrices} />;
    case "routing":
      return <RoutingPage />;
    case "logs":
      return <LogsPage />;
    case "settings":
      return <SettingsPage onOpenModelBasePrices={actions.openModelBasePrices} />;
    case "dashboard":
    default:
      return <DashboardPage />;
  }
});
```

Include explicit imports for every page and `Station`/`AppRouteId`. Do not accept arbitrary React children because child identity would defeat slot memoization.

- [ ] **Step 4: Add the memoized host**

First add the route-local boundary:

```tsx
// src/app/ShellPageErrorBoundary.tsx
import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "@/components/ui";

type Props = { children: ReactNode };
type State = { failed: boolean };

export class ShellPageErrorBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(_error: Error, _info: ErrorInfo) {
    // Do not log raw page data or credentials. Development diagnostics use aggregate counters only.
  }

  private retry = () => this.setState({ failed: false });

  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <div className="flex min-h-full items-center justify-center p-6" role="alert">
        <div className="grid max-w-sm gap-3 text-center">
          <h2 className="text-base font-semibold text-slate-900">页面加载失败</h2>
          <p className="text-sm text-slate-500">可以重试，或从侧边栏切换到其他页面。</p>
          <Button className="justify-self-center" onClick={this.retry} variant="secondary">
            重试
          </Button>
        </div>
      </div>
    );
  }
}
```

```tsx
export type ShellPageState = "active" | "background" | "inactive";

const ShellPageSlot = memo(function ShellPageSlot({
  routeId,
  state,
  actions,
}: {
  routeId: AppRouteId;
  state: ShellPageState;
  actions: ShellPageActions;
}) {
  const active = state === "active";
  const inert = !active;
  return (
    <PageActivityProvider active={active}>
      <div
        aria-hidden={inert}
        className="app-page-transition-layer"
        data-page-transition-kind="shell"
        data-page-transition-layer
        data-page-transition-page-id={routeId}
        data-page-transition-state={state}
        inert={inert ? "" : undefined}
      >
        <div className="app-page-transition-content">
          <ShellPageErrorBoundary>
            <ShellPageContent routeId={routeId} actions={actions} />
          </ShellPageErrorBoundary>
        </div>
      </div>
    </PageActivityProvider>
  );
});
```

`ShellPageHost` maps mounted route IDs and computes state from committed shell route plus transient activity. It owns the transition stack wrapper and renders `TransientPageHost` after shell slots.

- [ ] **Step 5: Stabilize all App actions**

Use stable `useCallback` functions around entity state setters plus `navigate`, then create one `useMemo<ShellPageActions>` object. No callback may depend on `activeRouteId`; use the controller's stable `navigate` and refs for focus/current-route reads.

- [ ] **Step 6: Run isolation and existing transition contracts**

Run:

```powershell
node scripts/shell-page-render-isolation.test.mjs
node scripts/page-transition-container.test.mjs
node scripts/page-activation-refresh.test.mjs
node scripts/page-transition-focus-scroll.test.mjs
pnpm.cmd build
```

Expected: PASS.

- [ ] **Step 7: Commit the render boundary**

```powershell
git add -- src/app/App.tsx src/app/ShellPageErrorBoundary.tsx src/app/ShellPageHost.tsx src/app/shellPageRegistry.tsx scripts/shell-page-render-isolation.test.mjs scripts/page-transition-container.test.mjs scripts/page-activation-refresh.test.mjs
git commit -m "refactor: isolate retained shell pages"
```

---

### Task 9: Integrate intent acknowledgement and non-blocking shell handoff

**Files:**
- Create: `scripts/navigation-handoff-contract.test.mjs`
- Modify: `src/app/navigationPerformance.ts`
- Modify: `src/app/App.tsx`
- Modify: `src/app/ShellPageHost.tsx`
- Modify: `src/components/shell/AppShell.tsx`
- Modify: `src/styles.css:72-152`
- Modify: `scripts/page-transition-styles.test.mjs`

- [ ] **Step 1: Write the failing handoff contract**

```js
// scripts/navigation-handoff-contract.test.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const app = await readFile("src/app/App.tsx", "utf8");
const host = await readFile("src/app/ShellPageHost.tsx", "utf8");
const styles = await readFile("src/styles.css", "utf8");

assert.match(app, /useNavigationController\("dashboard"\)/);
assert.match(app, /activeRouteId=\{intent\.shellRouteId\}/);
assert.match(host, /data-page-transition-state="entering"/);
assert.match(host, /event\.target === event\.currentTarget/);
assert.match(styles, /data-page-transition-state="entering"/);
assert.ok(!host.includes('mode="wait"'));
assert.ok(styles.includes("opacity"));
assert.ok(!/entering[\s\S]{0,300}(translate|scale|blur)/.test(styles));
```

- [ ] **Step 2: Run RED**

Run: `node scripts/navigation-handoff-contract.test.mjs`

Expected: FAIL because App still exposes only committed route and shell slots have no entering state.

- [ ] **Step 3: Wire the controller into App**

Replace the existing navigation `useState` with:

```tsx
const { intent, committed, pending, navigate } = useNavigationController("dashboard");
const { activeRouteId, previousRouteId, transientParentRouteId } = committed;
```

Pass `intent.shellRouteId` to `AppShell` so sidebar acknowledgement is urgent. Pass committed navigation plus `intent.sequence` to `ShellPageHost`. Use the stable `navigate` everywhere current code calls `navigateTo`.

- [ ] **Step 4: Add a bounded shell handoff state**

`ShellPageHost` may retain only the current previous shell for visual handoff. Add an `enteringSequence` state initialized from the latest committed shell change. During a shell-to-shell handoff:

- previous shell state is `background`;
- target shell state is `entering` and interactive;
- all others are `inactive`;
- `onAnimationEnd` checks `event.target === event.currentTarget` and marks that sequence complete;
- a 200ms fallback may end visual retention, but it must never change committed navigation.

No FIFO exit array or queued route nodes are allowed.

- [ ] **Step 5: Add development-only navigation measurements**

```ts
// src/app/navigationPerformance.ts
const enabled = import.meta.env.DEV;
let hiddenPageQueryStarts = 0;

export type NavigationPerformanceSnapshot = {
  hiddenPageQueryStarts: number;
};

export const navigationMarks = {
  intent: (sequence: number) => `navigation:${sequence}:intent`,
  indicator: (sequence: number) => `navigation:${sequence}:indicator`,
  content: (sequence: number) => `navigation:${sequence}:content`,
  complete: (sequence: number) => `navigation:${sequence}:complete`,
};

export function markNavigation(name: string) {
  if (enabled) performance.mark(name);
}

export function measureNavigation(name: string, start: string, end: string) {
  if (!enabled) return null;
  performance.measure(name, start, end);
  return performance.getEntriesByName(name).at(-1)?.duration ?? null;
}

export function recordHiddenPageQueryStart() {
  if (enabled) hiddenPageQueryStarts += 1;
}

export function getNavigationPerformanceSnapshot(): NavigationPerformanceSnapshot {
  return { hiddenPageQueryStarts };
}

declare global {
  interface Window {
    __relayNavigationPerformance?: {
      snapshot: typeof getNavigationPerformanceSnapshot;
    };
  }
}

if (enabled && typeof window !== "undefined") {
  window.__relayNavigationPerformance = { snapshot: getNavigationPerformanceSnapshot };
}
```

Mark intent inside `navigate`, indicator commit in an AppShell layout effect, content commit in a ShellPageHost layout effect, and visual completion after entering cleanup. Never include route payloads, IDs, keys, or data content in the mark names.

- [ ] **Step 6: Add opacity-only entering CSS**

```css
.app-page-transition-layer[data-page-transition-state="entering"] {
  position: absolute;
  inset: 0;
  z-index: 1;
  display: block;
  pointer-events: auto;
  visibility: visible;
  animation: relayShellPageEnter 140ms ease-out;
  will-change: opacity;
}

@keyframes relayShellPageEnter {
  from { opacity: 0; }
  to { opacity: 1; }
}
```

The background previous shell remains non-interactive. Reduced motion sets the entering duration to 1ms.

- [ ] **Step 7: Run handoff GREEN verification**

Run:

```powershell
node scripts/navigation-handoff-contract.test.mjs
node scripts/navigation-controller.test.mjs
node scripts/page-transition-styles.test.mjs
node scripts/page-transition-container.test.mjs
node scripts/page-transition-focus-scroll.test.mjs
pnpm.cmd build
```

Expected: PASS.

- [ ] **Step 8: Commit navigation acknowledgement and handoff**

```powershell
git add -- src/app/App.tsx src/app/ShellPageHost.tsx src/app/navigationPerformance.ts src/components/shell/AppShell.tsx src/styles.css scripts/navigation-handoff-contract.test.mjs scripts/page-transition-styles.test.mjs
git commit -m "feat: acknowledge navigation before page work"
```

---

### Task 10: Add cancellable idle prewarming

**Files:**
- Create: `scripts/page-idle-prewarm.test.mjs`
- Create: `src/app/useIdlePagePrewarm.ts`
- Modify: `src/app/App.tsx`
- Modify: `src/app/pageTransitionPolicy.ts`

- [ ] **Step 1: Write the failing prewarm policy test**

```js
// scripts/page-idle-prewarm.test.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const hook = await readFile("src/app/useIdlePagePrewarm.ts", "utf8").catch(() => "");
const policy = await readFile("src/app/pageTransitionPolicy.ts", "utf8");

assert.match(hook, /requestIdleCallback/);
assert.match(hook, /pointerdown/);
assert.match(hook, /keydown/);
assert.match(hook, /isInputPending/);
assert.match(policy, /prewarmPriority/);
assert.match(policy, /settings:[\s\S]*prewarmPriority:\s*1/);
assert.match(policy, /stations:[\s\S]*prewarmPriority:\s*2/);
assert.match(policy, /changes:[\s\S]*prewarmPriority:\s*3/);
```

- [ ] **Step 2: Run RED**

Run: `node scripts/page-idle-prewarm.test.mjs`

Expected: FAIL because no prewarm scheduler or policy exists.

- [ ] **Step 3: Add explicit prewarm metadata**

Extend `PageTransitionPolicy`:

```ts
export type PageTransitionPolicy = {
  pageId: AppPageId;
  kind: PageTransitionKind;
  parentRouteId: AppRouteId;
  retention: "keep";
  prewarmPriority: number | null;
};
```

Set `settings=1`, `stations=2`, `changes=3`; all other pages use `null`. Transient pages always use `null`.

- [ ] **Step 4: Implement one-item cancellable scheduling**

```ts
// src/app/useIdlePagePrewarm.ts
import { useEffect, useRef } from "react";
import type { AppRouteId } from "@/lib/types/navigation";

type IdleDeadlineLike = {
  didTimeout: boolean;
  timeRemaining: () => number;
};

type IdleWindow = Window & {
  requestIdleCallback?: (
    callback: (deadline: IdleDeadlineLike) => void,
    options?: { timeout: number },
  ) => number;
  cancelIdleCallback?: (handle: number) => void;
};

type SchedulingNavigator = Navigator & {
  scheduling?: { isInputPending?: () => boolean };
};

export function useIdlePagePrewarm({
  candidates,
  mountedRouteIds,
  disabled,
  onPrewarm,
}: {
  candidates: readonly AppRouteId[];
  mountedRouteIds: ReadonlySet<AppRouteId>;
  disabled: boolean;
  onPrewarm: (routeId: AppRouteId) => void;
}) {
  const onPrewarmRef = useRef(onPrewarm);
  onPrewarmRef.current = onPrewarm;

  useEffect(() => {
    if (disabled) return;
    const next = candidates.find((routeId) => !mountedRouteIds.has(routeId));
    if (!next) return;

    const idleWindow = window as IdleWindow;
    const schedulingNavigator = navigator as SchedulingNavigator;
    let idleHandle: number | null = null;
    let timeoutHandle: number | null = null;
    let disposed = false;

    const cancelScheduled = () => {
      if (idleHandle !== null) idleWindow.cancelIdleCallback?.(idleHandle);
      if (timeoutHandle !== null) window.clearTimeout(timeoutHandle);
      idleHandle = null;
      timeoutHandle = null;
    };

    function run(deadline?: IdleDeadlineLike) {
      idleHandle = null;
      timeoutHandle = null;
      if (disposed) return;
      const inputPending = schedulingNavigator.scheduling?.isInputPending?.() ?? false;
      if (inputPending || (deadline && !deadline.didTimeout && deadline.timeRemaining() < 4)) {
        schedule(250);
        return;
      }
      onPrewarmRef.current(next);
    }

    function schedule(delay = 0) {
      cancelScheduled();
      if (disposed) return;
      if (delay > 0) {
        timeoutHandle = window.setTimeout(() => schedule(), delay);
        return;
      }
      if (idleWindow.requestIdleCallback) {
        idleHandle = idleWindow.requestIdleCallback(run, { timeout: 1_000 });
        return;
      }
      timeoutHandle = window.setTimeout(() => run(), 250);
    }

    const postponeForInput = () => schedule(500);
    window.addEventListener("pointerdown", postponeForInput, { passive: true });
    window.addEventListener("keydown", postponeForInput);
    schedule();

    return () => {
      disposed = true;
      cancelScheduled();
      window.removeEventListener("pointerdown", postponeForInput);
      window.removeEventListener("keydown", postponeForInput);
    };
  }, [candidates, disabled, mountedRouteIds]);
}
```

Prewarming only adds an inactive route to `mountedRouteIds`; page queries remain `enabled:false` and `subscribed:false` through PageActivity.

- [ ] **Step 5: Run GREEN verification**

Run:

```powershell
node scripts/page-idle-prewarm.test.mjs
node scripts/page-transition-policy.test.mjs
node scripts/page-activity-query-contract.test.mjs
pnpm.cmd build
```

Expected: PASS.

- [ ] **Step 6: Commit idle prewarming**

```powershell
git add -- src/app/App.tsx src/app/pageTransitionPolicy.ts src/app/useIdlePagePrewarm.ts scripts/page-idle-prewarm.test.mjs scripts/page-transition-policy.test.mjs
git commit -m "perf: prewarm heavy shell pages when idle"
```

---

### Task 11: Complete remaining shell query migration and remove compatibility loaders

**Files:**
- Create: `scripts/hidden-page-query-boundary.test.mjs`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/features/pricing/PricingPage.tsx`
- Modify: `src/features/channels/ChannelStatusTab.tsx`
- Modify: `src/features/channels/ChannelMonitoringTab.tsx`
- Modify: `src/features/collectors/CollectorsPage.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/lib/query/queryKeys.ts`
- Modify: `src/lib/query/resourceQueries.ts`
- Delete when unreferenced: `src/lib/queries/dashboardQueries.ts`
- Delete when unreferenced: `src/lib/queries/changeQueries.ts`
- Delete when unreferenced: `src/lib/queries/logQueries.ts`
- Modify: `scripts/query-services-boundary.test.mjs`
- Modify: page-specific query service scripts

- [ ] **Step 1: Recheck overlap before editing**

Run:

```powershell
git status --short -- src/features/key-pool src/features/routing src/features/channels src/lib/queries
git log -5 --oneline -- src/features/key-pool src/features/routing src/features/channels src/lib/queries
```

Expected: the execution worktree contains the intended latest user changes and is clean for these paths. If not, integrate those changes before continuing; do not overwrite them.

- [ ] **Step 2: Write the hidden-page query boundary**

```js
// scripts/hidden-page-query-boundary.test.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pages = [
  "src/features/key-pool/KeyPoolPage.tsx",
  "src/features/routing/RoutingPage.tsx",
  "src/features/pricing/PricingPage.tsx",
  "src/features/channels/ChannelStatusTab.tsx",
  "src/features/channels/ChannelMonitoringTab.tsx",
  "src/features/collectors/CollectorsPage.tsx",
  "src/features/settings/SettingsPage.tsx",
];

for (const path of pages) {
  const source = await readFile(path, "utf8");
  assert.ok(source.includes("usePageActivity"), `${path} should read page activity`);
  assert.ok(source.includes("useActivityQuery") || !source.includes("load"), `${path} should use activity-bound server reads`);
  assert.ok(!source.includes("window.setInterval"), `${path} must not own an unconditional interval`);
}
```

- [ ] **Step 3: Run RED**

Run: `node scripts/hidden-page-query-boundary.test.mjs`

Expected: FAIL on every page that still owns activation loading or polling.

- [ ] **Step 4: Migrate each page by explicit fact ownership**

Add these exact raw-fact keys/options before changing page state:

| Page | Query key | Query function |
|---|---|---|
| Key Pool | `queryKeys.keyPool` | `listKeyPoolItems` |
| Key Pool | `queryKeys.stations` | `listStations` |
| Key Pool | `queryKeys.channelMonitors` | `listChannelMonitors` |
| Key Pool | `queryKeys.channelMonitorTemplates` | `listChannelMonitorTemplates` |
| Key Pool | `queryKeys.stationGroupBindings(stationId)` | `listStationGroupBindings(stationId)` |
| Key Pool | `queryKeys.groupRateRecords(stationId)` | `listGroupRateRecords(stationId)` |
| Key Pool | `queryKeys.stationKeyCapabilities(keyId)` | `getStationKeyCapabilities(keyId)` |
| Routing | `queryKeys.localRoutingWorkspace` | `loadLocalRoutingWorkspace` |
| Pricing | `queryKeys.pricingRules` | `listPricingRules` |
| Pricing | `queryKeys.stationKeys(stationId)` | `listStationKeys(stationId)` |
| Pricing | station/group keys above | existing station and group APIs |
| Channels | `queryKeys.channelStatus` | `loadChannelStatusWorkspace` |
| Channels | `queryKeys.channelMonitoring` | `loadChannelMonitoringWorkspace` |
| Collectors | `queryKeys.collectorSnapshot(stationId)` | `getLatestCollectorSnapshot(stationId)` |
| Collectors | `queryKeys.collectorHistory(stationId)` | `listCollectorSnapshots(stationId)` |
| Collectors | `queryKeys.collectorRuns(stationId)` | `listCollectorRuns(stationId)` |
| Collectors | `queryKeys.captureStatus(stationId)` | `getCaptureSessionStatus(stationId)` |
| Settings | `queryKeys.settings` | `getSettings` |
| Settings | `queryKeys.proxyStatus` | `getProxyStatus` |

Then apply these page-specific ownership rules:

- Key Pool keeps filters, drag state, dialogs, connectivity result, and unsaved forms local. Key/monitor writes update or invalidate only their listed keys.
- Routing keeps `activeTab` local and replaces operation sequence/local workspace state with one activity query. Settings events invalidate `localRoutingWorkspace`.
- Pricing keeps comparison filters and view-model projection local; raw pricing/station/group facts come from listed query options.
- Channel Status/Monitoring keep the existing raw workspace loaders behind activity query options and preserve the current channel view-model code.
- Collectors keeps manual credential/session form state local; its snapshot, history, run, and capture reads use station-scoped active queries and are never logged.
- Settings initializes its form from `settingsQueryOptions`; mutation success writes returned `AppSettings` into `queryKeys.settings` before dispatching the existing settings event.
- Every mutation cancels the exact affected keys before invoking Tauri, then writes returned facts or invalidates those same keys after success; no page-wide invalidation is permitted.

For each page, the exact active hook shape is:

```tsx
const { refreshEnabled } = usePageActivity();
const query = useActivityQuery(refreshEnabled, domainQueryOptions());
const data = query.data ?? stableEmptyValue;
const loading = query.isPending && query.data === undefined;
const error = query.error ? readError(query.error) : null;
```

Do not create page-specific intervals or duplicate server fact state.

- [ ] **Step 5: Remove obsolete workspace loaders only after zero-reference proof**

Run:

```powershell
rg -n "loadDashboardWorkspace|loadChangeCenterWorkspace|loadRequestLogWorkspace" src scripts
```

Expected before deletion: references exist only in the three loader modules and legacy tests being updated. Delete those modules and remove them from the explicit query-service inventory. Keep routing/channel loader modules that still provide raw API orchestration behind query options.

- [ ] **Step 6: Run focused and full query contracts**

Run:

```powershell
node scripts/hidden-page-query-boundary.test.mjs
node scripts/query-services-boundary.test.mjs
node scripts/local-routing-query-service.test.mjs
node scripts/routing-query-service.test.mjs
node scripts/channel-query-service.test.mjs
node scripts/page-activation-refresh.test.mjs
pnpm.cmd build
```

Expected: PASS.

- [ ] **Step 7: Commit remaining migration with exact paths**

Stage only files actually changed after the overlap check. Confirm with `git diff --cached --name-only`, then commit:

```powershell
git commit -m "refactor: bind shell data to active queries"
```

---

### Task 12: Browser, Tauri, performance, and boundary acceptance

**Files:**
- Create: `scripts/navigation-performance-browser.mjs`
- Modify: `scripts/page-transition-container.test.mjs`
- Modify: `scripts/page-transition-styles.test.mjs`

- [ ] **Step 1: Add a browser performance smoke script**

Create a Playwright CLI function file:

```js
// scripts/navigation-performance-browser.mjs
async (page) => {
  const targets = [
    { label: "中转站资产", routeId: "stations" },
    { label: "变更中心", routeId: "changes" },
    { label: "设置", routeId: "settings" },
    { label: "使用记录", routeId: "logs" },
    { label: "价格 / 倍率", routeId: "pricing" },
    { label: "总览", routeId: "dashboard" },
  ];
  const percentile = (values, ratio) => {
    const sorted = [...values].sort((a, b) => a - b);
    return sorted[Math.max(0, Math.ceil(sorted.length * ratio) - 1)] ?? 0;
  };

  async function runBurst(intervalMs) {
    return page.evaluate(async ({ targets, interval }) => {
      const clicks = new Map();
      const acknowledgements = new Map();
      const contentDurations = new Map();
      const pendingAcknowledgements = new Set();
      const pendingContent = new Set();
      const longTasks = [];
      const buttons = Array.from(document.querySelectorAll("aside nav button"));
      const targetByLabel = new Map(targets.map((target) => [target.label, target]));
      const observer = new MutationObserver(() => {
        for (const button of buttons) {
          const label = button.getAttribute("aria-label");
          const target = label ? targetByLabel.get(label) : null;
          if (target && clicks.has(target.routeId) && button.classList.contains("bg-slate-900") && !acknowledgements.has(target.routeId) && !pendingAcknowledgements.has(target.routeId)) {
            pendingAcknowledgements.add(target.routeId);
            requestAnimationFrame(() => {
              acknowledgements.set(target.routeId, performance.now() - clicks.get(target.routeId));
              pendingAcknowledgements.delete(target.routeId);
            });
          }
        }
        for (const layer of document.querySelectorAll('[data-page-transition-kind="shell"]')) {
          const routeId = layer.getAttribute("data-page-transition-page-id");
          const state = layer.getAttribute("data-page-transition-state");
          if (routeId && clicks.has(routeId) && (state === "entering" || state === "active") && !contentDurations.has(routeId) && !pendingContent.has(routeId)) {
            pendingContent.add(routeId);
            requestAnimationFrame(() => {
              contentDurations.set(routeId, performance.now() - clicks.get(routeId));
              pendingContent.delete(routeId);
            });
          }
        }
      });
      observer.observe(document.body, {
        attributes: true,
        subtree: true,
        attributeFilter: ["class", "data-page-transition-state"],
      });
      const taskObserver = new PerformanceObserver((list) => {
        longTasks.push(...list.getEntries().map((entry) => entry.duration));
      });
      taskObserver.observe({ entryTypes: ["longtask"] });

      targets.forEach((target, index) => {
        window.setTimeout(() => {
          const button = buttons.find((item) => item.getAttribute("aria-label") === target.label);
          clicks.set(target.routeId, performance.now());
          button?.click();
        }, index * interval);
      });

      await new Promise((resolve) => window.setTimeout(resolve, targets.length * interval + 1_000));
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      observer.disconnect();
      taskObserver.disconnect();
      return {
        active: buttons.find((button) => button.classList.contains("bg-slate-900"))?.getAttribute("aria-label") ?? null,
        acknowledgementDurations: [...acknowledgements.values()],
        contentDurations: [...contentDurations.values()],
        maxLongTask: Math.max(0, ...longTasks),
        interactiveLayers: document.querySelectorAll('[data-page-transition-layer]:not([inert])').length,
        hiddenPageQueryStarts: window.__relayNavigationPerformance?.snapshot().hiddenPageQueryStarts ?? null,
      };
    }, { targets, interval: intervalMs });
  }

  await page.reload({ waitUntil: "domcontentloaded" });
  await page.waitForTimeout(1_200);
  await runBurst(160);
  const normal = await runBurst(80);
  const extreme = await runBurst(12);
  const acknowledgementP95 = percentile(normal.acknowledgementDurations, 0.95);
  const contentP95 = percentile(normal.contentDurations, 0.95);
  if (normal.active !== targets.at(-1).label) throw new Error("normal burst did not end on the final route");
  if (extreme.active !== targets.at(-1).label) throw new Error("extreme burst did not end on the final route");
  if (normal.acknowledgementDurations.length !== targets.length) throw new Error("an 80ms click lacked acknowledgement");
  if (normal.contentDurations.length !== targets.length) throw new Error("an 80ms click lacked content commit");
  if (acknowledgementP95 > 32) throw new Error(`acknowledgement p95 ${acknowledgementP95}ms exceeds 32ms`);
  if (contentP95 > 100) throw new Error(`content p95 ${contentP95}ms exceeds 100ms`);
  if (normal.maxLongTask > 50) throw new Error(`navigation long task ${normal.maxLongTask}ms exceeds 50ms`);
  if (normal.interactiveLayers !== 1 || extreme.interactiveLayers !== 1) throw new Error("navigation left multiple interactive layers");
  if (extreme.hiddenPageQueryStarts !== 0) throw new Error(`hidden pages started ${extreme.hiddenPageQueryStarts ?? "unknown"} queries`);
  return { normal, extreme, acknowledgementP95, contentP95 };
}
```

The script must:

- install a `PerformanceObserver` for `longtask` entries;
- click Dashboard, Stations, Changes, Settings, Logs, and Pricing at 80ms intervals;
- record click time, sidebar active-class mutation, active shell layer mutation, and the next animation-frame paint opportunity for each acknowledgement/commit;
- repeat after all pages are warm;
- execute a 12ms burst and assert only the final destination, not every intermediate paint;
- report the maximum long task and p95 acknowledgement/content durations;
- assert exactly one interactive shell/transient layer.

Store screenshots/traces under `output/playwright/`; do not add them to Git.

- [ ] **Step 2: Run all focused contracts**

Run:

```powershell
node scripts/query-client-contract.test.mjs
node scripts/page-activity-query-contract.test.mjs
node scripts/shell-query-cache.test.mjs
node scripts/dashboard-shared-query.test.mjs
node scripts/log-change-shared-query.test.mjs
node scripts/stations-page-activity-query.test.mjs
node scripts/navigation-controller.test.mjs
node scripts/shell-page-render-isolation.test.mjs
node scripts/navigation-handoff-contract.test.mjs
node scripts/page-idle-prewarm.test.mjs
node scripts/hidden-page-query-boundary.test.mjs
```

Expected: every script PASS.

- [ ] **Step 3: Run existing transition and feature regressions**

Run:

```powershell
node scripts/page-transition-policy.test.mjs
node scripts/page-transition-container.test.mjs
node scripts/page-transition-styles.test.mjs
node scripts/page-transition-focus-scroll.test.mjs
node scripts/motion-page-transition.test.mjs
node scripts/page-activation-refresh.test.mjs
node scripts/model-base-prices-page.test.mjs
node scripts/dashboard-performance-metrics.test.mjs
node scripts/station-asset-loading-boundary.test.mjs
node scripts/query-services-boundary.test.mjs
```

Expected: every script PASS.

- [ ] **Step 4: Run build verification**

Run: `pnpm.cmd build`

Expected: TypeScript and Vite PASS; only the pre-existing chunk-size warning is acceptable.

- [ ] **Step 5: Run automated real-browser DOM/performance verification**

Start current source on a free port:

```powershell
pnpm.cmd exec vite --host 127.0.0.1 --port 5175
```

Run the Playwright smoke script against `http://127.0.0.1:5175`, using system Chrome if the bundled browser is unavailable.

```powershell
npx.cmd --yes --package @playwright/cli playwright-cli -s=navperf open http://127.0.0.1:5175 --browser chrome
npx.cmd --yes --package @playwright/cli playwright-cli -s=navperf run-code --filename scripts/navigation-performance-browser.mjs
npx.cmd --yes --package @playwright/cli playwright-cli -s=navperf close
```

Expected:

- acknowledgement p95 at or below 32ms;
- warm content commit p95 at or below 100ms;
- no navigation long task over 50ms;
- every 80ms click has visible sidebar acknowledgement;
- 12ms burst ends on the final requested route;
- one interactive layer;
- zero hidden-page query starts.

- [ ] **Step 6: Run current-source Tauri verification**

Run: `pnpm.cmd tauri:dev`

In the actual desktop WebView, repeat cold and warm Dashboard/Stations/Changes/Settings navigation using the development performance snapshot exposed by `navigationPerformance.ts`.

Expected:

- the same acknowledgement and warm thresholds hold with real database/Tauri calls;
- cold prewarmed page commit p95 is at or below 200ms;
- proxy/request-log/change refreshes continue while their declared active/global consumers exist;
- minimized/hidden windows do not run page-owned polling;
- no console errors, overlay residue, focus loss, or stale route activation.

- [ ] **Step 7: Run final boundary audit**

Run:

```powershell
git diff --check
git status --short
git diff --cached --name-only
git diff --name-only -- src-tauri
rg -n "window\.setInterval" src/features src/components/shell
rg -n "loadDashboardWorkspace|loadChangeCenterWorkspace|loadRequestLogWorkspace" src scripts
```

Expected:

- no whitespace errors;
- no unintended staged files;
- no `src-tauri` changes;
- any remaining interval is explicitly global or activity-gated and documented by a focused test;
- removed workspace loaders have zero references.
- performance thresholds remain unchanged; a miss returns the task to profiling and implementation rather than weakening the approved specification.

- [ ] **Step 8: Commit acceptance assets**

```powershell
git add -- scripts/navigation-performance-browser.mjs scripts/page-transition-container.test.mjs scripts/page-transition-styles.test.mjs
git commit -m "test: verify responsive page navigation"
```

---

## Plan Self-Review

### Spec coverage

- Reliability: Tasks 2, 3, 5, 6, 7, 9, and 12 cover query ownership, last-intent sequence, write-through consistency, unique interaction layers, failure isolation, and runtime verification.
- Maintainability: Tasks 1, 2, 3, and 8 create the four explicit Navigation/Host/Query/view-state boundaries and remove page-owned generic infrastructure.
- Extensibility: Tasks 1, 6, 8, 10, and 11 centralize query keys, resource options, page registry, prewarm metadata, and future retention policy.
- Page activity: Tasks 2, 3, 4, 5, 6, and 11 ensure active pages subscribe and refresh while hidden pages remain mounted but unsubscribed.
- Immediate feedback: Tasks 7-9 separate urgent intent from concurrent content commit and make shell animation non-blocking.
- Performance proof: Task 12 verifies browser and real Tauri behavior rather than relying on source-string tests alone.

### Type consistency

- `NavigationIntent.routeId`, `CommittedNavigation.activeRouteId`, and registry route IDs all use `AppPageId`/`AppRouteId` from `src/lib/types/navigation.ts`.
- `PageActivity.refreshEnabled` is the boolean passed to `useActivityQuery`; the wrapper owns `enabled` and `subscribed`.
- All shared fact consumers use keys from `queryKeys` and options from `resourceQueries`.
- `ShellPageActions` is one stable object passed through memoized slots; arbitrary React children are not used.
- Shell visual handoff sequence never changes committed navigation.

### Scope consistency

- No task changes Rust, database, proxy routing, collector semantics, pricing algorithms, or credential storage.
- Existing feature projections remain in feature modules.
- Query modules read raw facts and never perform feature projection or destructive writes.
- Page retention remains `keep`; LRU/discard is only a typed future extension.
- Transient lifecycle remains centralized and is not rewritten as part of shell navigation.
