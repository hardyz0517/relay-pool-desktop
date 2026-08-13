# Sidebar Local Proxy Status Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the bottom-left sidebar indicator accurately show whether the local proxy is running.

**Architecture:** `AppShell` remains presentation-only: it reads `ProxyStatus` through the existing typed proxy API on mount and every two seconds. `ProxyStatus.running` determines the indicator's dot color and accessible text; start/stop behavior remains owned by existing dashboard and settings controls.

**Tech Stack:** React 18, TypeScript, Tailwind CSS, Node assertion scripts, Vite.

---

### Task 1: Add a Sidebar Proxy Status Regression Test

**Files:**
- Create: `scripts/sidebar-local-proxy-status.test.mjs`
- Modify: `src/components/shell/AppShell.tsx`
- Test: `scripts/sidebar-local-proxy-status.test.mjs`

- [ ] **Step 1: Write the failing test**

Create `scripts/sidebar-local-proxy-status.test.mjs`:

```js
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const appShellSource = await readFile("src/components/shell/AppShell.tsx", "utf8");

assert.match(
  appShellSource,
  /import\s+\{\s*getProxyStatus\s*\}\s+from\s+"@\/lib\/api\/proxy"/,
  "app shell should read the typed proxy status API",
);
assert.ok(
  appShellSource.includes("const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);"),
  "app shell should retain the most recently read proxy status",
);
assert.ok(
  appShellSource.includes("getProxyStatus().then(setProxyStatus).catch(() => {})") &&
    appShellSource.includes("setInterval(refreshProxyStatus, 2_000)") &&
    appShellSource.includes("clearInterval(intervalId)"),
  "app shell should read proxy status on mount and refresh it every two seconds",
);
assert.ok(
  appShellSource.includes('title={proxyRunning ? "本地代理运行中" : "本地代理未启动"}') &&
    appShellSource.includes('text-emerald-500') &&
    appShellSource.includes('text-amber-500'),
  "sidebar indicator should expose running and stopped labels with distinct green and amber dots",
);
```

- [ ] **Step 2: Run the test to verify RED**

Run:

```powershell
node scripts/sidebar-local-proxy-status.test.mjs
```

Expected: the assertion for the missing `getProxyStatus` import fails, proving the existing hard-coded indicator does not meet the contract.

- [ ] **Step 3: Implement the minimal state refresh and visual mapping**

In `src/components/shell/AppShell.tsx`, add these imports:

```tsx
import { getProxyStatus } from "@/lib/api/proxy";
import type { ProxyStatus } from "@/lib/types/proxy";
```

Add the state next to `changeEvents` and `settings`:

```tsx
const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
const proxyRunning = proxyStatus?.running ?? false;
```

Add the refresh effect before the settings effect:

```tsx
useEffect(() => {
  function refreshProxyStatus() {
    void getProxyStatus().then(setProxyStatus).catch(() => {});
  }

  refreshProxyStatus();
  const intervalId = window.setInterval(refreshProxyStatus, 2_000);
  return () => window.clearInterval(intervalId);
}, []);
```

Replace the fixed title, aria label, and dot class with:

```tsx
title={proxyRunning ? "本地代理运行中" : "本地代理未启动"}
aria-label={proxyRunning ? "本地代理运行中" : "本地代理未启动"}
...
<Circle className={cn("h-2.5 w-2.5 fill-current", proxyRunning ? "text-emerald-500" : "text-amber-500")} />
```

- [ ] **Step 4: Run the regression test to verify GREEN**

Run:

```powershell
node scripts/sidebar-local-proxy-status.test.mjs
```

Expected: process exits `0` with no assertion output.

- [ ] **Step 5: Run the frontend verification suite**

Run:

```powershell
node scripts/dashboard-local-route-start.test.mjs
pnpm.cmd build
```

Expected: the existing dashboard local-route contract remains green and `tsc --noEmit && vite build` exits `0`.

- [ ] **Step 6: Commit the implementation**

Run:

```powershell
git add -- src/components/shell/AppShell.tsx scripts/sidebar-local-proxy-status.test.mjs
git diff --cached --name-only
git commit -m "fix: sync sidebar proxy status"
```

Expected staged paths are exactly `src/components/shell/AppShell.tsx` and `scripts/sidebar-local-proxy-status.test.mjs`.
