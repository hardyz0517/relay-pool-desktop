# Relay Pool Desktop UI Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn Relay Pool Desktop into a calmer white desktop tool with a narrow icon toolbar, prominent workbench metrics, unified object rows, and a page-level Add Provider flow.

**Architecture:** Keep the current React/Vite/Tauri front-end structure and avoid backend data-model changes. Add small reusable UI primitives first, then migrate the shell and pages onto those primitives in phases so each commit stays reviewable. Preserve the existing Tauri APIs and mock fallbacks.

**Tech Stack:** Tauri 2, React 18, TypeScript, Vite, Tailwind CSS, lucide-react, @dnd-kit.

---

## Preflight Rules

- Work in an isolated feature branch or worktree before implementation. Recommended branch name: `codex/relay-pool-ui-upgrade`.
- Do not stage generated `.superpowers/` files.
- Do not use `git add .`, `git add -A`, or `git commit -a`.
- The current repository may already contain unrelated modified files. Before each commit, run `git diff --cached --name-only` and confirm it contains only paths from the task.
- This plan is UI-only. Do not change Rust proxy routing, stream semantics, local key storage, or database schema.

## File Structure

### Existing Files To Modify

- `src/styles.css`: design tokens for white/system gray/Apple blue, neutral borders, 8px radius, shell width.
- `src/components/ui/layout.ts`: JS constants matching CSS tokens.
- `src/components/ui/button.tsx`: semantic button variants and icon size.
- `src/components/ui/Dialog.tsx`: neutral dialog border, 8px radius, non-cyan overlay/shadow.
- `src/components/ui/StatusBadge.tsx`: neutral/Apple-blue `info` tone, no cyan default.
- `src/components/ui/SectionCard.tsx`: restrained card styling and header spacing.
- `src/components/ui/MetricCard.tsx`: align with the new MetricPanel visual grammar.
- `src/components/ui/SegmentedControl.tsx`: use gray background and selected white surface.
- `src/components/ui/index.ts`: export new primitives.
- `src/components/shell/AppShell.tsx`: replace collapsible sidebar with fixed narrow icon toolbar.
- `src/components/shell/PageScaffold.tsx`: support page-level back action and optional status pill.
- `src/app/App.tsx`: add internal navigation to page-level Add Provider.
- `src/app/routes.tsx`: rename dashboard route to `工作台`, prune or group route labels if needed.
- `src/lib/types/navigation.ts`: add non-sidebar page id for Add Provider.
- `src/features/dashboard/DashboardPage.tsx`: convert 总览 into 代理工作台.
- `src/features/stations/StationsPage.tsx`: use ObjectRow, route Add Provider entry to page form.
- `src/features/key-pool/KeyPoolPage.tsx`: use ObjectRow for key rows.
- `src/features/collectors/CollectorsPage.tsx`: use ObjectRow for collector tasks/snapshots.
- `src/features/routing/RoutingPage.tsx`: use ObjectRow for accepted/rejected route candidates.
- `src/features/settings/SettingsPage.tsx`: keep settings functional but align buttons/cards/dialogs.

### New Files To Create

- `src/components/ui/Card.tsx`: low-level neutral card primitive.
- `src/components/ui/IconButton.tsx`: accessible icon-only button wrapper using existing Button.
- `src/components/ui/MetricPanel.tsx`: 4-metric panel for workbench and page summaries.
- `src/components/ui/ObjectRow.tsx`: reusable object-list row with icon, title, subtitle, badges, metrics, hover/focus actions, optional drag handle.
- `src/components/ui/PageForm.tsx`: page-level form shell with bottom action bar.
- `src/features/stations/AddProviderPage.tsx`: immersive preset-first Add Provider page.
- `src/features/stations/providerPresets.ts`: preset metadata and default form values.

---

### Task 1: Preflight, Branch, And Baseline Verification

**Files:**
- Read: `docs/superpowers/specs/2026-06-28-relay-pool-ui-upgrade-design.md`
- Read: `src/app/App.tsx`
- Read: `src/components/shell/AppShell.tsx`
- Read: `src/features/dashboard/DashboardPage.tsx`

- [ ] **Step 1: Confirm the working tree before changes**

Run:

```powershell
git status --short
git branch --show-current
```

Expected:

```text
Current branch is visible.
Existing modified or untracked files are known before UI upgrade work starts.
```

- [ ] **Step 2: Create or switch to an isolated branch/worktree**

If not already isolated, use the `superpowers:using-git-worktrees` skill. The branch should be:

```text
codex/relay-pool-ui-upgrade
```

Expected:

```text
Implementation work does not overwrite unrelated user changes in the original checkout.
```

- [ ] **Step 3: Run baseline frontend verification**

Run:

```powershell
pnpm.cmd build
```

Expected when the current checkout is healthy:

```text
tsc --noEmit
vite build
built in
```

If it fails before any implementation change, record the exact failure in the task log and continue only after deciding whether the failure is pre-existing.

- [ ] **Step 4: Commit no files in this task**

This task is read-only unless a worktree/branch setup command is required by the chosen execution mode.

---

### Task 2: Design Tokens And UI Primitive Semantics

**Files:**
- Modify: `src/styles.css`
- Modify: `src/components/ui/layout.ts`
- Modify: `src/components/ui/button.tsx`
- Modify: `src/components/ui/StatusBadge.tsx`
- Modify: `src/components/ui/Dialog.tsx`
- Modify: `src/components/ui/SectionCard.tsx`
- Modify: `src/components/ui/SegmentedControl.tsx`
- Create: `src/components/ui/Card.tsx`
- Create: `src/components/ui/IconButton.tsx`
- Modify: `src/components/ui/index.ts`

- [ ] **Step 1: Update CSS tokens**

In `src/styles.css`, make the `:root` block use these values:

```css
:root {
  color-scheme: light;
  --background: 0 0% 100%;
  --foreground: 222 22% 12%;
  --muted: 220 14% 96%;
  --muted-foreground: 220 9% 42%;
  --accent: 211 100% 52%;
  --accent-foreground: 0 0% 100%;
  --border: 220 13% 89%;
  --surface-radius: 8px;
  --surface-shadow: 0 8px 24px rgba(15, 23, 42, 0.05);
  --surface-shadow-hover: 0 12px 30px rgba(15, 23, 42, 0.08);
  --shell-sidebar-width: 64px;
  --shell-header-height: 52px;
  --shell-page-gap: 16px;
}
```

- [ ] **Step 2: Align layout constants**

Replace `src/components/ui/layout.ts` with:

```ts
export const shellLayout = {
  sidebarWidth: 64,
  headerHeight: 52,
  pageGap: 16,
  cardRadius: 8,
  cardShadow: "0 8px 24px rgba(15, 23, 42, 0.05)",
} as const;
```

- [ ] **Step 3: Make Button semantic and Apple-blue**

Replace the current `ButtonProps` and class mapping in `src/components/ui/button.tsx` with:

```tsx
type ButtonVariant = "primary" | "secondary" | "ghost" | "outline" | "danger";
type ButtonSize = "sm" | "md" | "icon";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
};

const sizeClassName: Record<ButtonSize, string> = {
  sm: "h-7 rounded-[7px] px-2 text-xs",
  md: "h-8 rounded-[var(--surface-radius)] px-3 text-[13px]",
  icon: "h-8 w-8 rounded-[var(--surface-radius)] px-0",
};
```

Use this class mapping inside the button:

```tsx
className={cn(
  "inline-flex cursor-pointer items-center justify-center gap-2 font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent)/0.35)] disabled:pointer-events-none disabled:cursor-default disabled:opacity-50",
  sizeClassName[size],
  variant === "primary" &&
    "bg-[hsl(var(--accent))] text-white shadow-[0_1px_2px_rgba(10,132,255,0.22)] hover:bg-[#0077ed]",
  variant === "secondary" &&
    "border border-border bg-white text-slate-700 hover:bg-slate-50",
  variant === "ghost" &&
    "text-slate-600 hover:bg-slate-100 hover:text-slate-900",
  variant === "outline" &&
    "border border-border bg-white text-slate-700 hover:bg-slate-50",
  variant === "danger" &&
    "border border-rose-200 bg-white text-rose-700 hover:bg-rose-50",
  className,
)}
```

Ensure the function default is:

```tsx
export function Button({
  className,
  variant = "primary",
  size = "md",
  type = "button",
  ...props
}: ButtonProps) {
```

- [ ] **Step 4: Add Card primitive**

Create `src/components/ui/Card.tsx`:

```tsx
import type { HTMLAttributes } from "react";
import { cn } from "@/lib/utils";

type CardProps = HTMLAttributes<HTMLDivElement> & {
  interactive?: boolean;
};

export function Card({ className, interactive = false, ...props }: CardProps) {
  return (
    <div
      className={cn(
        "rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]",
        interactive && "transition-shadow hover:shadow-[var(--surface-shadow-hover)]",
        className,
      )}
      {...props}
    />
  );
}
```

- [ ] **Step 5: Add IconButton wrapper**

Create `src/components/ui/IconButton.tsx`:

```tsx
import type { ButtonHTMLAttributes, ReactNode } from "react";
import { Button } from "./button";

type IconButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> & {
  label: string;
  children: ReactNode;
  variant?: "primary" | "secondary" | "ghost" | "outline" | "danger";
};

export function IconButton({
  label,
  title,
  variant = "ghost",
  children,
  ...props
}: IconButtonProps) {
  return (
    <Button
      {...props}
      variant={variant}
      size="icon"
      title={title ?? label}
      aria-label={label}
    >
      {children}
    </Button>
  );
}
```

- [ ] **Step 6: Neutralize StatusBadge info tone**

In `src/components/ui/StatusBadge.tsx`, keep the exported tone type and replace `toneClassName` with:

```ts
const toneClassName: Record<StatusTone, string> = {
  healthy: "border-emerald-200 bg-emerald-50 text-emerald-700",
  warning: "border-amber-200 bg-amber-50 text-amber-700",
  error: "border-rose-200 bg-rose-50 text-rose-700",
  disabled: "border-slate-200 bg-slate-50 text-slate-500",
  info: "border-blue-200 bg-blue-50 text-blue-700",
};
```

- [ ] **Step 7: Neutralize Dialog styling**

In `src/components/ui/Dialog.tsx`, replace cyan borders and oversized radius with:

```tsx
"max-h-[calc(100vh-32px)] w-full max-w-[780px] overflow-hidden rounded-[var(--surface-radius)] border border-border bg-white shadow-[0_24px_70px_rgba(15,23,42,0.16)]"
```

Replace both `border-cyan-100` occurrences with:

```tsx
border-border
```

- [ ] **Step 8: Adjust SectionCard and SegmentedControl**

In `src/components/ui/SectionCard.tsx`, keep the current API and make sure the root class is:

```tsx
"overflow-hidden rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]"
```

In `src/components/ui/SegmentedControl.tsx`, use gray base and white selected state:

```tsx
"inline-flex items-center rounded-[var(--surface-radius)] border border-border bg-slate-100 p-0.5"
"bg-white text-slate-900 shadow-sm"
"text-slate-600 hover:text-slate-900"
```

- [ ] **Step 9: Export new primitives**

Add these exports to `src/components/ui/index.ts`:

```ts
export { Card } from "./Card";
export { IconButton } from "./IconButton";
```

- [ ] **Step 10: Verify and commit**

Run:

```powershell
pnpm.cmd build
git diff --cached --name-only
```

Stage exact files:

```powershell
git add -- src/styles.css src/components/ui/layout.ts src/components/ui/button.tsx src/components/ui/StatusBadge.tsx src/components/ui/Dialog.tsx src/components/ui/SectionCard.tsx src/components/ui/SegmentedControl.tsx src/components/ui/Card.tsx src/components/ui/IconButton.tsx src/components/ui/index.ts
git diff --cached --name-only
```

Expected staged files are exactly the ten files above.

Commit:

```powershell
git commit -m "feat: refresh ui tokens and primitives"
```

---

### Task 3: MetricPanel And ObjectRow Primitives

**Files:**
- Create: `src/components/ui/MetricPanel.tsx`
- Create: `src/components/ui/ObjectRow.tsx`
- Modify: `src/components/ui/MetricCard.tsx`
- Modify: `src/components/ui/index.ts`

- [ ] **Step 1: Add MetricPanel**

Create `src/components/ui/MetricPanel.tsx`:

```tsx
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

type MetricTone = "neutral" | "good" | "warning" | "danger";

type MetricItem = {
  label: string;
  value: string;
  detail?: string;
  icon?: LucideIcon;
  tone?: MetricTone;
};

type MetricPanelProps = {
  title?: string;
  description?: string;
  metrics: MetricItem[];
  className?: string;
};

const toneClassName: Record<MetricTone, string> = {
  neutral: "text-slate-700",
  good: "text-emerald-700",
  warning: "text-amber-700",
  danger: "text-rose-700",
};

export function MetricPanel({ title, description, metrics, className }: MetricPanelProps) {
  return (
    <section
      className={cn(
        "rounded-[var(--surface-radius)] border border-border bg-white p-4 shadow-[var(--surface-shadow)]",
        className,
      )}
    >
      {(title || description) && (
        <div className="mb-4">
          {title && <h2 className="text-[13px] font-semibold text-slate-900">{title}</h2>}
          {description && <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>}
        </div>
      )}
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        {metrics.map((metric) => {
          const Icon = metric.icon;
          const tone = metric.tone ?? "neutral";
          return (
            <div key={metric.label} className="min-w-0 rounded-[var(--surface-radius)] border border-border bg-slate-50/60 p-3">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                {Icon && <Icon className="h-3.5 w-3.5" />}
                <span className="truncate">{metric.label}</span>
              </div>
              <div className={cn("mt-1 truncate text-[22px] font-semibold leading-7", toneClassName[tone])}>
                {metric.value}
              </div>
              {metric.detail && <div className="mt-0.5 truncate text-xs text-muted-foreground">{metric.detail}</div>}
            </div>
          );
        })}
      </div>
    </section>
  );
}
```

- [ ] **Step 2: Add ObjectRow**

Create `src/components/ui/ObjectRow.tsx`:

```tsx
import type { ReactNode } from "react";
import { GripVertical } from "lucide-react";
import { cn } from "@/lib/utils";

type ObjectRowMetric = {
  label: string;
  value: string;
  tone?: "neutral" | "good" | "warning" | "danger";
};

type ObjectRowProps = {
  icon?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  badges?: ReactNode;
  metrics?: ObjectRowMetric[];
  actions?: ReactNode;
  selected?: boolean;
  draggable?: boolean;
  className?: string;
  onClick?: () => void;
};

const metricToneClassName: Record<NonNullable<ObjectRowMetric["tone"]>, string> = {
  neutral: "text-slate-700",
  good: "text-emerald-700",
  warning: "text-amber-700",
  danger: "text-rose-700",
};

export function ObjectRow({
  icon,
  title,
  subtitle,
  badges,
  metrics = [],
  actions,
  selected = false,
  draggable = false,
  className,
  onClick,
}: ObjectRowProps) {
  const Component = onClick ? "button" : "div";

  return (
    <Component
      type={onClick ? "button" : undefined}
      onClick={onClick}
      className={cn(
        "group grid min-h-[68px] w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 rounded-[var(--surface-radius)] border bg-white px-3 py-2.5 text-left shadow-[var(--surface-shadow)] transition-colors",
        selected ? "border-slate-300 bg-slate-50" : "border-border hover:bg-slate-50/70",
        onClick && "cursor-pointer",
        className,
      )}
    >
      <div className="flex items-center gap-2">
        {draggable && (
          <span className="flex h-8 w-5 items-center justify-center text-slate-300 group-hover:text-slate-500">
            <GripVertical className="h-4 w-4" />
          </span>
        )}
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[var(--surface-radius)] border border-border bg-white text-slate-700">
          {icon}
        </span>
      </div>

      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <div className="truncate text-[13px] font-semibold text-slate-900">{title}</div>
          {badges && <div className="flex shrink-0 items-center gap-1.5">{badges}</div>}
        </div>
        {subtitle && <div className="mt-0.5 truncate text-xs text-muted-foreground">{subtitle}</div>}
      </div>

      <div className="flex items-center gap-3">
        {metrics.length > 0 && (
          <div className="hidden items-center gap-3 md:flex">
            {metrics.map((metric) => {
              const tone = metric.tone ?? "neutral";
              return (
                <div key={metric.label} className="min-w-[56px] text-right">
                  <div className={cn("truncate text-[13px] font-semibold", metricToneClassName[tone])}>
                    {metric.value}
                  </div>
                  <div className="truncate text-[11px] text-muted-foreground">{metric.label}</div>
                </div>
              );
            })}
          </div>
        )}
        {actions && (
          <div className="flex items-center gap-1 opacity-100 transition-opacity md:opacity-0 md:group-hover:opacity-100 md:group-focus-within:opacity-100">
            {actions}
          </div>
        )}
      </div>
    </Component>
  );
}
```

- [ ] **Step 3: Keep MetricCard as compatibility wrapper**

Do not delete `MetricCard.tsx`. Adjust colors only if Task 2 left old teal/cyan classes. Keep its public props stable because several pages still import it before migration is complete.

- [ ] **Step 4: Export primitives**

Add these exports to `src/components/ui/index.ts`:

```ts
export { MetricPanel } from "./MetricPanel";
export { ObjectRow } from "./ObjectRow";
```

- [ ] **Step 5: Verify and commit**

Run:

```powershell
pnpm.cmd build
```

Stage exact files:

```powershell
git add -- src/components/ui/MetricPanel.tsx src/components/ui/ObjectRow.tsx src/components/ui/MetricCard.tsx src/components/ui/index.ts
git diff --cached --name-only
```

Expected staged files are exactly:

```text
src/components/ui/MetricPanel.tsx
src/components/ui/ObjectRow.tsx
src/components/ui/MetricCard.tsx
src/components/ui/index.ts
```

Commit:

```powershell
git commit -m "feat: add metric panel and object row primitives"
```

---

### Task 4: Narrow Icon Toolbar Shell

**Files:**
- Modify: `src/components/shell/AppShell.tsx`
- Modify: `src/components/shell/PageScaffold.tsx`
- Modify: `src/app/routes.tsx`
- Modify: `src/lib/types/navigation.ts`

- [ ] **Step 1: Add page id type**

In `src/lib/types/navigation.ts`, keep `AppRouteId` for sidebar routes and add:

```ts
export type AppPageId = AppRouteId | "addProvider";
```

- [ ] **Step 2: Rename dashboard route label**

In `src/app/routes.tsx`, change the dashboard route to:

```ts
{
  id: "dashboard",
  label: "工作台",
  description: "当前路由、今日指标和本地代理入口",
  icon: LayoutDashboard,
},
```

Keep all current route ids stable.

- [ ] **Step 3: Replace AppShell sidebar behavior**

In `src/components/shell/AppShell.tsx`:

- Remove `useState`.
- Remove `ChevronLeft`, `ChevronRight`, and the collapse button.
- Import `IconButton` from `@/components/ui`.
- Use `AppPageId` for `activeRouteId` and `onRouteChange`.
- Make the aside fixed width with `style={{ width: shellLayout.sidebarWidth }}`.

The nav button markup should follow this shape:

```tsx
<nav className="flex flex-1 flex-col items-center gap-1 px-2 py-2">
  {appRoutes.map((route) => {
    const Icon = route.icon;
    const active = route.id === activeRouteId;

    return (
      <button
        key={route.id}
        type="button"
        onClick={() => onRouteChange(route.id)}
        title={route.label}
        aria-label={route.label}
        className={cn(
          "flex h-10 w-10 cursor-pointer items-center justify-center rounded-[var(--surface-radius)] transition-colors",
          active
            ? "bg-slate-900 text-white"
            : "text-slate-500 hover:bg-slate-100 hover:text-slate-900",
        )}
      >
        <Icon className="h-4.5 w-4.5" />
      </button>
    );
  })}
</nav>
```

Use this footer status block so `Local Proxy` no longer changes row height when the shell is narrow:

```tsx
<div className="flex flex-col items-center gap-2 border-t border-border px-2 py-3">
  <span
    className="flex h-10 w-10 items-center justify-center rounded-[var(--surface-radius)] border border-border bg-white"
    title="本地代理未启动"
    aria-label="本地代理未启动"
  >
    <Circle className="h-2.5 w-2.5 fill-current text-amber-500" />
  </span>
  <IconButton label="复制本地入口">
    <Copy className="h-4 w-4" />
  </IconButton>
</div>
```

- [ ] **Step 4: Keep only one header layer**

In `src/components/shell/AppShell.tsx`, keep the top application status bar compact:

```tsx
<header className="flex h-[var(--shell-header-height)] shrink-0 items-center justify-end border-b border-border bg-white px-4">
  <div className="flex items-center overflow-hidden rounded-[var(--surface-radius)] border border-border bg-white text-xs text-slate-600">
    ...
  </div>
</header>
```

Do not duplicate the page title here. Page titles belong to `PageScaffold`.

- [ ] **Step 5: Add PageScaffold back/status support**

In `src/components/shell/PageScaffold.tsx`, extend props:

```ts
type PageScaffoldProps = {
  title: string;
  description: string;
  actions?: ReactNode;
  status?: ReactNode;
  backAction?: ReactNode;
  width?: "full" | "settings";
  children?: ReactNode;
};
```

Render header left side as:

```tsx
<div className="flex min-w-0 items-center gap-3">
  {backAction}
  <div className="min-w-0">
    <div className="flex items-center gap-2">
      <h1 className="truncate text-[18px] font-semibold leading-6 text-slate-900">
        {title}
      </h1>
      {status}
    </div>
    <p className="mt-0.5 max-w-3xl truncate text-xs text-muted-foreground">
      {description}
    </p>
  </div>
</div>
```

- [ ] **Step 6: Verify shell visually**

Run:

```powershell
pnpm.cmd build
pnpm.cmd dev -- --port 51030
```

Open `http://127.0.0.1:51030/` or use the in-app browser.

Expected visual checks:

```text
Left toolbar is 64px wide.
No visible 收起/展开 text exists.
Each icon has title/aria-label.
The Local Proxy footer keeps fixed icon geometry.
Page title appears in the content header, not duplicated in the shell header.
```

- [ ] **Step 7: Commit**

Stage exact files:

```powershell
git add -- src/components/shell/AppShell.tsx src/components/shell/PageScaffold.tsx src/app/routes.tsx src/lib/types/navigation.ts
git diff --cached --name-only
```

Commit:

```powershell
git commit -m "feat: convert shell to narrow toolbar"
```

---

### Task 5: Workbench Dashboard With Prominent Metrics

**Files:**
- Modify: `src/app/App.tsx`
- Modify: `src/features/dashboard/DashboardPage.tsx`

- [ ] **Step 1: Add navigation prop in App**

In `src/app/App.tsx`, import `AppPageId` and change state:

```tsx
import type { AppPageId } from "@/lib/types/navigation";

const [activeRouteId, setActiveRouteId] = useState<AppPageId>("dashboard");
```

Pass `onNavigate={setActiveRouteId}` to `DashboardPage`:

```tsx
case "dashboard":
default:
  return <DashboardPage onNavigate={setActiveRouteId} />;
```

- [ ] **Step 2: Update DashboardPage props**

In `src/features/dashboard/DashboardPage.tsx`, add:

```tsx
import type { AppPageId } from "@/lib/types/navigation";

type DashboardPageProps = {
  onNavigate: (pageId: AppPageId) => void;
};

export function DashboardPage({ onNavigate }: DashboardPageProps) {
```

- [ ] **Step 3: Replace metric card wall with dual-core top section**

Use `MetricPanel` and a current route card. The top grid should be:

```tsx
<div className="grid gap-4 xl:grid-cols-[minmax(0,1.35fr)_minmax(360px,0.65fr)]">
  <SectionCard
    title="当前路由"
    description="外部工具会优先使用这条本地 OpenAI-compatible 入口。"
    action={<StatusBadge tone={proxyRunning ? "healthy" : "warning"}>{proxyRunning ? "运行中" : "未启动"}</StatusBadge>}
  >
    ...
  </SectionCard>
  <MetricPanel
    title="今日指标"
    description="只保留判断运行状态最重要的 4 个数字。"
    metrics={[
      { label: "今日请求", value: todayRequests.toLocaleString("zh-CN"), detail: "代理日志", icon: Activity },
      { label: "可用 Key", value: `${enabledKeyCount}`, detail: "启用中", icon: KeyRound, tone: enabledKeyCount > 0 ? "good" : "warning" },
      { label: "失败率", value: failureRateText, detail: "今日请求", icon: AlertTriangle, tone: failureRateTone },
      { label: "今日成本", value: `¥${dashboard.todayCostCny.toFixed(2)}`, detail: "估算", icon: BadgeDollarSign },
    ]}
  />
</div>
```

Define `failureRateText` and `failureRateTone` above the return:

```tsx
const todayFailedRequests = requestLogs.filter((log) => log.status === "failed").length;
const failureRate = todayRequests > 0 ? todayFailedRequests / todayRequests : 0;
const failureRateText = `${(failureRate * 100).toFixed(1)}%`;
const failureRateTone = failureRate > 0.1 ? "danger" : failureRate > 0.03 ? "warning" : "good";
```

- [ ] **Step 4: Add primary Add Provider action**

In the `PageScaffold` actions, keep one primary button:

```tsx
actions={
  <>
    <Button variant="secondary" onClick={() => void handleCopyProxyText(proxyBaseUrl)}>
      <Copy className="h-4 w-4" />
      复制本地入口
    </Button>
    <Button onClick={() => onNavigate("addProvider")}>
      <Plus className="h-4 w-4" />
      添加 Provider
    </Button>
  </>
}
```

Import `Plus` from `lucide-react`.

- [ ] **Step 5: Add light object queue**

Below the dual-core top section, render key/provider queue with `ObjectRow`:

```tsx
<SectionCard title="路由队列" description="轻量显示当前可用对象，详细管理仍在中转站和 Key 池页面。">
  <div className="grid gap-2">
    {keyPoolItems.slice(0, 6).map((key) => (
      <ObjectRow
        key={key.id}
        icon={<KeyRound className="h-4 w-4" />}
        title={key.name}
        subtitle={`${key.stationName} · ${key.stationBaseUrl}`}
        badges={<StatusBadge tone={key.enabled ? "healthy" : "disabled"}>{key.enabled ? "可用" : "停用"}</StatusBadge>}
        metrics={[
          { label: "优先级", value: `${key.priority}` },
          { label: "成功率", value: key.successRate === null ? "-" : `${Math.round(key.successRate * 100)}%`, tone: key.successRate !== null && key.successRate < 0.9 ? "warning" : "neutral" },
          { label: "延迟", value: key.avgLatencyMs === null ? "-" : `${key.avgLatencyMs}ms` },
          { label: "失败", value: `${key.consecutiveFailures}`, tone: key.consecutiveFailures > 0 ? "warning" : "neutral" },
        ]}
      />
    ))}
  </div>
</SectionCard>
```

- [ ] **Step 6: Verify and commit**

Run:

```powershell
pnpm.cmd build
```

Browser checks:

```text
Homepage title is 代理工作台 or 工作台.
Current route and 今日指标 are the strongest first-screen elements.
There is exactly one blue primary button on the page.
The local proxy Base URL copy button remains visually clickable.
The old eight-card metric wall is gone.
```

Stage and commit:

```powershell
git add -- src/app/App.tsx src/features/dashboard/DashboardPage.tsx
git diff --cached --name-only
git commit -m "feat: redesign dashboard as proxy workbench"
```

---

### Task 6: Add Provider Page Flow

**Files:**
- Create: `src/components/ui/PageForm.tsx`
- Create: `src/features/stations/providerPresets.ts`
- Create: `src/features/stations/AddProviderPage.tsx`
- Modify: `src/app/App.tsx`
- Modify: `src/features/stations/StationsPage.tsx`

- [ ] **Step 1: Create PageForm shell**

Create `src/components/ui/PageForm.tsx`:

```tsx
import type { FormHTMLAttributes, ReactNode } from "react";
import { cn } from "@/lib/utils";

type PageFormProps = FormHTMLAttributes<HTMLFormElement> & {
  children: ReactNode;
  footer: ReactNode;
};

export function PageForm({ children, footer, className, ...props }: PageFormProps) {
  return (
    <form className={cn("grid min-h-0 gap-[var(--shell-page-gap)]", className)} {...props}>
      <div className="grid gap-[var(--shell-page-gap)] pb-20">{children}</div>
      <div className="sticky bottom-0 z-10 flex items-center justify-end gap-2 border-t border-border bg-white/95 px-4 py-3 backdrop-blur">
        {footer}
      </div>
    </form>
  );
}
```

Export it from `src/components/ui/index.ts`:

```ts
export { PageForm } from "./PageForm";
```

- [ ] **Step 2: Create provider presets**

Create `src/features/stations/providerPresets.ts`:

```ts
import type { StationType } from "@/lib/types/stations";

export type ProviderPresetId =
  | "custom"
  | "openai-compatible"
  | "sub2api"
  | "newapi"
  | "deepseek"
  | "qwen"
  | "siliconflow"
  | "minimax";

export type ProviderPreset = {
  id: ProviderPresetId;
  name: string;
  description: string;
  stationType: StationType;
  baseUrl: string;
};

export const providerPresets: ProviderPreset[] = [
  {
    id: "custom",
    name: "Custom",
    description: "完全自定义 Provider。",
    stationType: "custom",
    baseUrl: "",
  },
  {
    id: "openai-compatible",
    name: "OpenAI Compatible",
    description: "适用于大多数兼容 /v1 的中转站。",
    stationType: "openai-compatible",
    baseUrl: "https://api.example.com/v1",
  },
  {
    id: "sub2api",
    name: "Sub2API",
    description: "带订阅与采集能力的 Sub2API 站点。",
    stationType: "sub2api",
    baseUrl: "https://sub2api.example.com/v1",
  },
  {
    id: "newapi",
    name: "NewAPI",
    description: "NewAPI 风格站点。",
    stationType: "newapi",
    baseUrl: "https://newapi.example.com/v1",
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    description: "DeepSeek 官方兼容入口。",
    stationType: "openai-compatible",
    baseUrl: "https://api.deepseek.com/v1",
  },
  {
    id: "qwen",
    name: "Qwen",
    description: "通义千问 OpenAI-compatible 入口。",
    stationType: "openai-compatible",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
  },
  {
    id: "siliconflow",
    name: "SiliconFlow",
    description: "硅基流动 OpenAI-compatible 入口。",
    stationType: "openai-compatible",
    baseUrl: "https://api.siliconflow.cn/v1",
  },
  {
    id: "minimax",
    name: "MiniMax",
    description: "MiniMax OpenAI-compatible 入口。",
    stationType: "openai-compatible",
    baseUrl: "https://api.minimax.chat/v1",
  },
];
```

- [ ] **Step 3: Create AddProviderPage**

Create `src/features/stations/AddProviderPage.tsx`. It should:

- Use `PageScaffold` with a back icon button.
- Use `PageForm` instead of `Dialog`.
- Use `providerPresets`.
- Call existing `createStation`.
- Navigate back to `stations` on successful add.

Use this component signature:

```tsx
type AddProviderPageProps = {
  onBack: () => void;
  onCreated: () => void;
};

export function AddProviderPage({ onBack, onCreated }: AddProviderPageProps) {
```

Use this submit payload:

```tsx
await createStation({
  name: form.name.trim(),
  stationType: form.stationType,
  baseUrl: form.baseUrl.trim(),
  apiKey: form.apiKey.trim(),
  enabled: true,
  creditPerCny: 1,
  lowBalanceThresholdCny: form.lowBalanceThresholdCny.trim()
    ? Number(form.lowBalanceThresholdCny)
    : null,
  note: form.note.trim() ? form.note.trim() : null,
});
```

Validation must set visible errors:

```tsx
if (!form.name.trim()) {
  setError("请填写 Provider 名称。");
  return;
}
if (!form.baseUrl.trim()) {
  setError("请填写 Base URL。");
  return;
}
if (!form.apiKey.trim()) {
  setError("请填写 API Key。");
  return;
}
```

- [ ] **Step 4: Route AddProviderPage in App**

In `src/app/App.tsx`, add:

```tsx
import { AddProviderPage } from "@/features/stations/AddProviderPage";
```

Then add switch case:

```tsx
case "addProvider":
  return (
    <AddProviderPage
      onBack={() => setActiveRouteId("stations")}
      onCreated={() => setActiveRouteId("stations")}
    />
  );
```

- [ ] **Step 5: Change StationsPage Add Provider entry**

Add props to `StationsPage`:

```tsx
type StationsPageProps = {
  onAddProvider?: () => void;
};

export function StationsPage({ onAddProvider }: StationsPageProps) {
```

Change the main Add button to:

```tsx
<Button onClick={onAddProvider ?? openCreate}>
  <Plus className="h-4 w-4" />
  添加 Provider
</Button>
```

In `src/app/App.tsx`, pass:

```tsx
return <StationsPage onAddProvider={() => setActiveRouteId("addProvider")} />;
```

- [ ] **Step 6: Keep old station dialog only for edit/detail**

Do not delete edit/detail dialogs in this task. For create, the main entry should be page-level. Existing example/create helpers can remain unused until Task 9 cleanup.

- [ ] **Step 7: Verify and commit**

Run:

```powershell
pnpm.cmd build
```

Browser checks:

```text
Clicking 添加 Provider from 工作台 opens a full page form.
Clicking 添加 Provider from 中转站 opens the same full page form.
Preset selection fills name, base URL, and station type.
Back returns to 中转站.
Add with empty required fields shows visible Chinese validation.
Add with valid fields uses existing createStation fallback in browser preview.
```

Stage and commit:

```powershell
git add -- src/components/ui/PageForm.tsx src/components/ui/index.ts src/features/stations/providerPresets.ts src/features/stations/AddProviderPage.tsx src/app/App.tsx src/features/stations/StationsPage.tsx
git diff --cached --name-only
git commit -m "feat: add page-level provider creation flow"
```

---

### Task 7: Migrate Object Lists To ObjectRow

**Files:**
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/collectors/CollectorsPage.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`

- [ ] **Step 1: Migrate StationsPage list rows**

For each station list item, render `ObjectRow` with:

```tsx
<ObjectRow
  key={station.id}
  draggable
  selected={station.id === selectedStation?.id}
  icon={<StationStatusDot status={station.status} />}
  title={station.name}
  subtitle={`${stationTypeLabels[station.stationType]} · ${station.baseUrl}`}
  badges={
    <>
      <StatusBadge tone={statusTone[station.status]}>{stationStatusLabels[station.status]}</StatusBadge>
      {station.enabled ? <StatusBadge tone="healthy">启用</StatusBadge> : <StatusBadge tone="disabled">停用</StatusBadge>}
    </>
  }
  metrics={[
    { label: "Key", value: `${station.keyCount}` },
    { label: "余额", value: station.balanceCny === null ? "未知" : `¥${station.balanceCny.toFixed(2)}`, tone: station.status === "warning" ? "warning" : "neutral" },
    { label: "延迟", value: station.latencyMs === null ? "-" : `${station.latencyMs}ms` },
  ]}
  actions={
    <>
      <IconButton label={`查看 ${station.name}`} onClick={() => openDetail(station)}>
        <ArrowRight className="h-4 w-4" />
      </IconButton>
      <IconButton label={`编辑 ${station.name}`} onClick={() => openEdit(station)}>
        <Edit3 className="h-4 w-4" />
      </IconButton>
    </>
  }
/>
```

Keep @dnd-kit sorting behavior intact.

- [ ] **Step 2: Migrate KeyPoolPage rows**

Replace key cards or table-like rows with `ObjectRow`. Use key name/title, station name/base URL as subtitle, enabled/status badges, and metrics for priority, group, tier, balance, or latency based on fields already available in `src/lib/types/stationKeys.ts`.

Use icon:

```tsx
<KeyRound className="h-4 w-4" />
```

Use actions:

```tsx
<IconButton label="复制脱敏 Key" disabled>
  <Copy className="h-4 w-4" />
</IconButton>
```

The disabled copy action is acceptable only when the full secret is not available in this view.

- [ ] **Step 3: Migrate CollectorsPage rows**

Use `ObjectRow` for the history snapshots in `CollectorsPage`. Required fields:

```tsx
icon={<Activity className="h-4 w-4" />}
title={itemSummary.adapter ?? sourceLabel(snapshot.source)}
subtitle={`${formatDateTime(snapshot.fetchedAt)} · ${itemSummary.message ?? "暂无摘要"}`}
badges={<StatusBadge tone={toneForConclusion(conclusionLabel(itemSummary, snapshot))}>{conclusionLabel(itemSummary, snapshot)}</StatusBadge>}
metrics={[
  { label: "来源", value: sourceLabel(snapshot.source) },
  { label: "状态", value: snapshot.status },
  { label: "字段", value: countValue(toCollectorSummary(snapshot.summaryJson).recognized?.matchedFieldCount) },
]}
```

Keep collector actions such as detect/collect as `IconButton` or `Button variant="secondary"` depending on whether the label is necessary for clarity.

- [ ] **Step 4: Migrate RoutingPage candidate rows**

Use `ObjectRow` for accepted and rejected route candidates:

```tsx
<ObjectRow
  key={`${candidate.stationId}-${candidate.model}`}
  icon={<GitBranch className="h-4 w-4" />}
  title={candidate.stationName}
  subtitle={`${candidate.keyName} · ${candidate.mappedModel ?? "未映射模型"}`}
  badges={<StatusBadge tone={candidate.accepted ? "healthy" : "disabled"}>{candidate.accepted ? "可用" : "已过滤"}</StatusBadge>}
  metrics={[
    { label: "分数", value: candidate.score.toFixed(1), tone: candidate.accepted ? "good" : "neutral" },
    { label: "原因", value: `${candidate.reasons.length}` },
    { label: "过滤", value: `${candidate.rejectionReasons.length}`, tone: candidate.rejectionReasons.length > 0 ? "warning" : "neutral" },
  ]}
/>
```

- [ ] **Step 5: Verify and commit**

Run:

```powershell
pnpm.cmd build
```

Browser checks:

```text
中转站, Key 池, 采集, 路由 rows share the same object-row grammar.
Hover actions appear without shifting row height.
Dangerous actions still require existing confirmation.
Drag handles appear only on sortable lists.
Every page still has at most one blue primary button.
```

Stage and commit:

```powershell
git add -- src/features/stations/StationsPage.tsx src/features/key-pool/KeyPoolPage.tsx src/features/collectors/CollectorsPage.tsx src/features/routing/RoutingPage.tsx
git diff --cached --name-only
git commit -m "feat: unify object list rows"
```

---

### Task 8: Page Polish And Cyan Cleanup

**Files:**
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/channels/ChannelStatusPage.tsx`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/features/pricing/PricingPage.tsx`
- Modify: `src/styles.css`

- [ ] **Step 1: Search for cyan/teal remnants**

Run:

```powershell
rg "cyan|teal|rounded-\\[1[0-9]px\\]|rounded-\\[18px\\]|bg-teal|text-teal|border-teal" src
```

Expected after cleanup:

```text
No cyan/teal classes remain as default UI theme classes.
Status-specific color classes may remain only if they express a real status.
```

- [ ] **Step 2: Replace page-level cyan/teal classes**

For each match that is not a real status color, replace:

```text
teal/cyan background -> slate or white
teal/cyan text -> slate or accent blue
teal/cyan border -> border-border
large custom radius -> rounded-[var(--surface-radius)]
```

Concrete class replacements:

```text
bg-teal-50 -> bg-slate-50
hover:bg-teal-50 -> hover:bg-slate-50
text-teal-700 -> text-slate-700
border-teal-100 -> border-border
border-cyan-100 -> border-border
bg-cyan-50 -> bg-blue-50
text-cyan-700 -> text-blue-700
rounded-[18px] -> rounded-[var(--surface-radius)]
```

- [ ] **Step 3: Align secondary pages**

For settings, channels, logs, and pricing:

- Keep existing data and actions.
- Use `SectionCard`, `MetricPanel`, `ObjectRow`, `Button`, and `IconButton` where they reduce one-off markup.
- Do not introduce new page-specific card styles unless a current component cannot express the layout.

- [ ] **Step 4: Verify and commit**

Run:

```powershell
pnpm.cmd build
rg "cyan|teal|rounded-\\[1[0-9]px\\]|rounded-\\[18px\\]|bg-teal|text-teal|border-teal" src
```

Expected:

```text
Build passes.
Search output is empty or limited to intentional status-specific code with a written note in the task log.
```

Stage and commit:

```powershell
git add -- src/features/settings/SettingsPage.tsx src/features/channels/ChannelStatusPage.tsx src/features/logs/LogsPage.tsx src/features/pricing/PricingPage.tsx src/styles.css
git diff --cached --name-only
git commit -m "style: polish secondary pages"
```

---

### Task 9: Browser Acceptance And Final Cleanup

**Files:**
- Modify only if verification reveals defects: files touched in Tasks 2-8.
- Do not stage: `.superpowers/`

- [ ] **Step 1: Run full frontend build**

Run:

```powershell
pnpm.cmd build
```

Expected:

```text
tsc --noEmit
vite build
built in
```

- [ ] **Step 2: Start local preview**

Run:

```powershell
pnpm.cmd dev -- --port 51030
```

Expected:

```text
Local: http://127.0.0.1:51030/
```

- [ ] **Step 3: Browser-scan required pages**

Use the in-app browser or Playwright. Visit:

```text
工作台
中转站
Key 池
采集
设置
```

Record checks:

```text
White background is dominant.
No cyan page background is visible.
Left toolbar is narrow and stable.
No sidebar text jitters or shifts during navigation.
Primary blue action count is 0 or 1 per page.
Object rows do not jump when hover actions appear.
Text does not overflow buttons or object rows at desktop width.
```

- [ ] **Step 4: Mobile/narrow viewport smoke**

Use browser viewport around `390x844`.

Expected:

```text
Toolbar remains usable.
Page header actions wrap without overlapping.
ObjectRow metrics hide or stack without horizontal overflow.
Add Provider form keeps bottom action bar visible.
```

- [ ] **Step 5: Clean generated artifacts**

Run:

```powershell
git status --short
```

Expected:

```text
.superpowers/ may be untracked from visual companion work.
Do not stage .superpowers/.
Only intentional source and plan/spec files are staged.
```

If `.superpowers/` should be ignored, ask the user before adding `.superpowers/` to `.gitignore` because that is repo hygiene outside the UI visual implementation itself.

- [ ] **Step 6: Final commit if fixes were needed**

Only if acceptance revealed and fixed defects:

```powershell
git add -- <exact fixed paths>
git diff --cached --name-only
git commit -m "fix: address ui acceptance issues"
```

- [ ] **Step 7: Final report**

Report:

```text
Implemented phases and commit ids.
Build command and result.
Browser pages checked.
Known remaining visual debt, if any.
Untracked files intentionally left unstaged.
```

---

## Plan Self-Review

### Spec Coverage

- White/system gray/Apple blue tokens: Task 2 and Task 8.
- Narrow icon toolbar: Task 4.
- Prominent homepage metrics: Task 5.
- Current route plus today metrics dual core: Task 5.
- Unified ObjectRow for Provider/Key/采集任务/Route: Task 3 and Task 7.
- Page-level Add Provider flow: Task 6.
- Button/Card/Tabs/Dialog semantics: Task 2, Task 3, Task 8.
- Browser page acceptance for 工作台/中转站/Key 池/采集/设置: Task 9.
- No proxy core semantic changes: Preflight Rules and Task 6 submit uses existing `createStation`.

### Placeholder Scan

This plan intentionally avoids placeholder markers and broad catch-all instructions. List page mappings use the current `KeyPoolItem`, `CollectorSnapshot`, and `RouteCandidateExplanation` fields.

### Type Consistency

- `AppRouteId` remains sidebar-only.
- `AppPageId = AppRouteId | "addProvider"` is the internal app page id.
- `ObjectRow` metric tone names match `MetricPanel` tone names except `danger`, which is used consistently for error-like metric emphasis.
- `providerPresets` uses existing `StationType`.
