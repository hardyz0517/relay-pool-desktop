# Station Detail Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a full-page, read-only station detail surface where balance appears first, group/rate facts are the primary body, and refresh/collection actions are the only station-detail mutations.

**Architecture:** Add a page route state named `stationDetail`, keep `StationsPage` as the list owner, and move station detail rendering into a new page component. `StationDetailPage` owns data loading and refresh actions; `StationDetailContent` receives a prepared view model and callbacks so the same content can be reused in another surface if a display-mode setting is added in a separate iteration.

**Tech Stack:** React 18, TypeScript, Vite, Tailwind CSS, Tauri invoke APIs already wrapped under `src/lib/api`, lucide-react icons, existing local UI components.

---

## File Structure

- Create `src/features/stations/stationDetailViewModels.ts`
  - Responsibility: derive display rows and section metadata from `Station`, `BalanceSnapshot`, `StationGroupBinding`, `GroupRateRecord`, `CollectorRun`, `CollectorSnapshot`, `StationCredentials`, `StationKey`, and `ChangeEvent`.
  - Export plain TypeScript helpers only; no React hooks or network calls.
- Create `src/features/stations/StationDetailPage.tsx`
  - Responsibility: load station detail data, keep stale data visible while refresh tasks run, show load/error states, and wire refresh/back/edit callbacks.
- Create `src/features/stations/components/StationDetailContent.tsx`
  - Responsibility: render the identity header, balance cards, group/rate table, and secondary diagnostics from a view model.
- Modify `src/lib/types/navigation.ts`
  - Add `stationDetail` to `AppPageId`.
- Modify `src/app/App.tsx`
  - Add selected detail station state, route `stationDetail`, and navigation callbacks between station list, detail page, and edit-provider page.
- Modify `src/features/stations/StationsPage.tsx`
  - Add `onOpenStation?: (stationId: string) => void`.
  - Use the full page for the normal row-click path when `onOpenStation` is provided.
  - Keep the existing drawer code as a local fallback path only when the page callback is absent.

## Task 1: Add The Station Detail View Model

**Files:**
- Create: `src/features/stations/stationDetailViewModels.ts`

- [ ] **Step 1: Create the exported detail types**

Add this file with the imports and exported model shapes:

```typescript
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { CollectorSnapshot } from "@/lib/types/collector";
import type { CollectorRun } from "@/lib/types/collectorRuns";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationKey } from "@/lib/types/stationKeys";
import type { StationCredentials } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";

export type DetailTone = "neutral" | "good" | "warning" | "error" | "muted";

export type StationDetailBalanceCard = {
  label: string;
  value: string;
  helper: string;
  tone: DetailTone;
};

export type StationDetailGroupRow = {
  id: string;
  groupName: string;
  groupId: string;
  effectiveRate: string;
  defaultRate: string;
  userRate: string;
  bindingStatus: string;
  source: string;
  lastChecked: string;
  tone: DetailTone;
  warning: string | null;
};

export type StationDetailDiagnosticItem = {
  label: string;
  value: string;
  tone: DetailTone;
};

export type StationDetailViewModel = {
  station: Station;
  stationTypeLabel: string;
  statusLabel: string;
  statusTone: DetailTone;
  lastActivityLabel: string;
  balanceCards: StationDetailBalanceCard[];
  groupRows: StationDetailGroupRow[];
  groupEmptyMessage: string;
  loginItems: StationDetailDiagnosticItem[];
  collectorItems: StationDetailDiagnosticItem[];
  snapshotItems: StationDetailDiagnosticItem[];
  changeItems: StationDetailDiagnosticItem[];
};
```

- [ ] **Step 2: Add formatting helpers**

Append these helpers in the same file:

```typescript
const stationTypeLabels: Record<string, string> = {
  sub2api: "Sub2API",
  newapi: "NewAPI",
  openai_compatible: "OpenAI Compatible",
};

const stationStatusLabels: Record<string, string> = {
  healthy: "健康",
  warning: "需关注",
  error: "异常",
  disabled: "已停用",
  unchecked: "未检查",
};

const bindingStatusLabels: Record<string, string> = {
  available: "可用",
  bound: "已绑定",
  missing: "缺失",
  disabled: "已停用",
  manual_legacy: "手动遗留",
};

export function formatDetailDate(value: string | null | undefined) {
  if (!value) {
    return "未记录";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "未记录";
  }
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function formatMoney(value: number | null | undefined, currency = "CNY") {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "未采集";
  }
  return `${currency} ${value.toFixed(2)}`;
}

function formatRate(value: number | null | undefined) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "-";
  }
  return `${value.toFixed(2)}x`;
}

function latestByTime<T>(items: T[], getTimeValue: (item: T) => string | null | undefined) {
  return items.reduce<T | null>((latest, item) => {
    if (!latest) {
      return item;
    }
    const latestTime = new Date(getTimeValue(latest) ?? "").getTime();
    const itemTime = new Date(getTimeValue(item) ?? "").getTime();
    return itemTime > latestTime ? item : latest;
  }, null);
}
```

- [ ] **Step 3: Add balance and group row builders**

Append:

```typescript
function buildBalanceCards(station: Station, balances: BalanceSnapshot[]): StationDetailBalanceCard[] {
  const latestBalance = latestByTime(
    balances.filter((item) => item.stationId === station.id && item.scope === "station"),
    (item) => item.updatedAt,
  );
  const currentValue = latestBalance?.value ?? station.balanceCny;
  const threshold = latestBalance?.lowBalanceThreshold ?? station.lowBalanceThresholdCny;
  const balanceTone: DetailTone =
    latestBalance?.status === "low" || latestBalance?.status === "depleted" || station.status === "warning"
      ? "warning"
      : latestBalance?.status === "depleted" || station.status === "error"
        ? "error"
        : currentValue == null
          ? "muted"
          : "good";

  return [
    {
      label: "当前余额",
      value: formatMoney(currentValue, latestBalance?.currency ?? "CNY"),
      helper: latestBalance ? `来源 ${latestBalance.source}` : "来自站点记录",
      tone: balanceTone,
    },
    {
      label: "低余额阈值",
      value: formatMoney(threshold, latestBalance?.currency ?? "CNY"),
      helper: threshold == null ? "未设置阈值" : "低于该值需要关注",
      tone: threshold == null ? "muted" : "neutral",
    },
    {
      label: "余额更新时间",
      value: formatDetailDate(latestBalance?.collectedAt ?? station.lastCheckedAt),
      helper: latestBalance ? `置信度 ${(latestBalance.confidence * 100).toFixed(0)}%` : "等待采集",
      tone: latestBalance ? "neutral" : "muted",
    },
  ];
}

function buildGroupRows(bindings: StationGroupBinding[], rates: GroupRateRecord[]): StationDetailGroupRow[] {
  const latestRateByBinding = new Map<string, GroupRateRecord>();
  for (const rate of rates) {
    if (!rate.groupBindingId) {
      continue;
    }
    const current = latestRateByBinding.get(rate.groupBindingId);
    if (!current || new Date(rate.checkedAt).getTime() > new Date(current.checkedAt).getTime()) {
      latestRateByBinding.set(rate.groupBindingId, rate);
    }
  }

  return bindings
    .filter((binding) => binding.bindingKind === "station_group")
    .map((binding) => {
      const rate = latestRateByBinding.get(binding.id);
      const effectiveRate = binding.effectiveRateMultiplier ?? rate?.effectiveRateMultiplier ?? null;
      const warning =
        binding.bindingStatus === "missing"
          ? "分组已缺失"
          : effectiveRate == null
            ? "倍率未采集"
            : effectiveRate === 0
              ? "倍率为 0"
              : null;

      return {
        id: binding.id,
        groupName: binding.groupName || "未命名分组",
        groupId: binding.groupIdHash ?? binding.groupKeyHash,
        effectiveRate: formatRate(effectiveRate),
        defaultRate: formatRate(binding.defaultRateMultiplier ?? rate?.defaultRateMultiplier),
        userRate: formatRate(binding.userRateMultiplier ?? rate?.userRateMultiplier),
        bindingStatus: bindingStatusLabels[binding.bindingStatus] ?? binding.bindingStatus,
        source: binding.rateSource ?? rate?.source ?? "unknown",
        lastChecked: formatDetailDate(binding.lastCheckedAt ?? rate?.checkedAt),
        tone: warning ? "warning" : "neutral",
        warning,
      };
    });
}
```

- [ ] **Step 4: Add the public builder**

Append:

```typescript
export function buildStationDetailViewModel({
  station,
  balances,
  groupBindings,
  groupRates,
  collectorRuns,
  latestSnapshot,
  credentials,
  stationKeys,
  changes,
}: {
  station: Station;
  balances: BalanceSnapshot[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  collectorRuns: CollectorRun[];
  latestSnapshot: CollectorSnapshot | null;
  credentials: StationCredentials | null;
  stationKeys: StationKey[];
  changes: ChangeEvent[];
}): StationDetailViewModel {
  const activeChanges = changes.filter(
    (event) =>
      event.stationId === station.id &&
      event.status !== "dismissed" &&
      event.status !== "resolved",
  );
  const latestRun = latestByTime(collectorRuns, (run) => run.finishedAt ?? run.startedAt);
  const stationKeyEnabledCount = stationKeys.filter((key) => key.enabled).length;
  const groupRows = buildGroupRows(groupBindings, groupRates);

  return {
    station,
    stationTypeLabel: stationTypeLabels[station.stationType] ?? station.stationType,
    statusLabel: station.enabled ? stationStatusLabels[station.status] ?? station.status : "已停用",
    statusTone: station.enabled ? (station.status === "healthy" ? "good" : station.status === "error" ? "error" : "warning") : "muted",
    lastActivityLabel: formatDetailDate(station.lastCheckedAt ?? latestRun?.finishedAt ?? latestSnapshot?.fetchedAt),
    balanceCards: buildBalanceCards(station, balances),
    groupRows,
    groupEmptyMessage: "还没有采集到分组和倍率。点击“采集分组倍率”后，这里会展示中转站可见分组、绑定状态和倍率来源。",
    loginItems: [
      { label: "登录账号", value: credentials?.loginUsername || "未配置", tone: credentials?.loginUsername ? "neutral" : "muted" },
      { label: "登录状态", value: credentials?.passwordPresent ? "已保存密码" : "未保存密码", tone: credentials?.passwordPresent ? "neutral" : "muted" },
      { label: "站点密钥", value: `${stationKeyEnabledCount}/${stationKeys.length || station.keyCount} 启用`, tone: stationKeyEnabledCount > 0 ? "neutral" : "warning" },
    ],
    collectorItems: [
      { label: "最近任务", value: latestRun ? `${latestRun.taskType} / ${latestRun.status}` : "未运行", tone: latestRun ? "neutral" : "muted" },
      { label: "最近完成", value: formatDetailDate(latestRun?.finishedAt ?? null), tone: latestRun?.finishedAt ? "neutral" : "muted" },
      { label: "失败数", value: latestRun ? String(latestRun.failureCount) : "0", tone: latestRun && latestRun.failureCount > 0 ? "warning" : "neutral" },
    ],
    snapshotItems: [
      { label: "快照来源", value: latestSnapshot?.source ?? "未生成", tone: latestSnapshot ? "neutral" : "muted" },
      { label: "快照状态", value: latestSnapshot?.status ?? "未生成", tone: latestSnapshot ? "neutral" : "muted" },
      { label: "快照时间", value: formatDetailDate(latestSnapshot?.fetchedAt), tone: latestSnapshot ? "neutral" : "muted" },
    ],
    changeItems: activeChanges.slice(0, 4).map((event) => ({
      label: event.severity === "critical" ? "严重" : event.severity === "warning" ? "警告" : "信息",
      value: event.title,
      tone: event.severity === "critical" ? "error" : event.severity === "warning" ? "warning" : "neutral",
    })),
  };
}
```

- [ ] **Step 5: Run TypeScript/Vite build**

Run: `pnpm.cmd build`

Expected: the build can fail at this point only if the new type imports do not match existing exported type names. If it fails, fix the import/type name in `src/features/stations/stationDetailViewModels.ts` and rerun until `tsc --noEmit` and `vite build` complete.

- [ ] **Step 6: Commit Task 1**

```powershell
git add -- src/features/stations/stationDetailViewModels.ts
git commit -m "feat: add station detail view model"
```

## Task 2: Build The Pure Detail Content Component

**Files:**
- Create: `src/features/stations/components/StationDetailContent.tsx`

- [ ] **Step 1: Create the component shell and props**

Add:

```tsx
import { ArrowLeft, Edit3, RefreshCw, RotateCw } from "lucide-react";
import { Button, EmptyState, StatusBadge } from "@/components/ui";
import { cn } from "@/lib/utils";
import type { StationDetailViewModel } from "@/features/stations/stationDetailViewModels";

export type StationDetailRefreshAction = "balance" | "groups" | "full";

export type StationDetailContentProps = {
  viewModel: StationDetailViewModel;
  loadingAction: StationDetailRefreshAction | null;
  sectionError: string | null;
  onBack: () => void;
  onEdit: () => void;
  onRefresh: (action: StationDetailRefreshAction) => void;
};

export function StationDetailContent({
  viewModel,
  loadingAction,
  sectionError,
  onBack,
  onEdit,
  onRefresh,
}: StationDetailContentProps) {
  return (
    <div className="flex min-h-full flex-col gap-4">
      <StationAssetHeader
        viewModel={viewModel}
        loadingAction={loadingAction}
        onBack={onBack}
        onEdit={onEdit}
        onRefresh={onRefresh}
      />
      {sectionError && (
        <div className="rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800">
          {sectionError}
        </div>
      )}
      <StationBalanceOverview viewModel={viewModel} />
      <StationGroupRatePanel viewModel={viewModel} loadingAction={loadingAction} onRefresh={onRefresh} />
      <StationDiagnosticsSection viewModel={viewModel} />
    </div>
  );
}
```

- [ ] **Step 2: Add the identity header**

Append:

```tsx
function StationAssetHeader({
  viewModel,
  loadingAction,
  onBack,
  onEdit,
  onRefresh,
}: {
  viewModel: StationDetailViewModel;
  loadingAction: StationDetailRefreshAction | null;
  onBack: () => void;
  onEdit: () => void;
  onRefresh: (action: StationDetailRefreshAction) => void;
}) {
  const { station } = viewModel;

  return (
    <section className="rounded-lg border border-border bg-white px-4 py-4 shadow-sm">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <button
            type="button"
            className="mb-3 inline-flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-slate-950"
            onClick={onBack}
          >
            <ArrowLeft className="h-4 w-4" />
            返回中转站资产
          </button>
          <div className="flex flex-wrap items-center gap-2">
            <h1 className="truncate text-xl font-semibold text-slate-950">{station.name}</h1>
            <StatusBadge tone={viewModel.statusTone === "good" ? "healthy" : viewModel.statusTone === "error" ? "error" : viewModel.statusTone === "warning" ? "warning" : "disabled"}>
              {viewModel.statusLabel}
            </StatusBadge>
          </div>
          <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted-foreground">
            <span>{viewModel.stationTypeLabel}</span>
            <span className="max-w-[420px] truncate">{station.baseUrl}</span>
            <span>最近活动 {viewModel.lastActivityLabel}</span>
          </div>
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <Button variant="outline" disabled={Boolean(loadingAction)} onClick={() => onRefresh("balance")}>
            <RefreshCw className={cn("h-4 w-4", loadingAction === "balance" && "animate-spin")} />
            刷新余额
          </Button>
          <Button variant="outline" disabled={Boolean(loadingAction)} onClick={() => onRefresh("groups")}>
            <RefreshCw className={cn("h-4 w-4", loadingAction === "groups" && "animate-spin")} />
            采集分组倍率
          </Button>
          <Button variant="secondary" disabled={Boolean(loadingAction)} onClick={() => onRefresh("full")}>
            <RotateCw className={cn("h-4 w-4", loadingAction === "full" && "animate-spin")} />
            重新采集
          </Button>
          <Button variant="ghost" onClick={onEdit}>
            <Edit3 className="h-4 w-4" />
            编辑供应商
          </Button>
        </div>
      </div>
    </section>
  );
}
```

- [ ] **Step 3: Add balance, group/rate, and diagnostics sections**

Append:

```tsx
function StationBalanceOverview({ viewModel }: { viewModel: StationDetailViewModel }) {
  return (
    <section className="grid gap-3 md:grid-cols-3">
      {viewModel.balanceCards.map((card) => (
        <div key={card.label} className="rounded-lg border border-border bg-white p-4 shadow-sm">
          <div className="text-sm text-muted-foreground">{card.label}</div>
          <div className={cn("mt-2 text-2xl font-semibold", toneClass(card.tone))}>{card.value}</div>
          <div className="mt-2 text-xs text-muted-foreground">{card.helper}</div>
        </div>
      ))}
    </section>
  );
}

function StationGroupRatePanel({
  viewModel,
  loadingAction,
  onRefresh,
}: {
  viewModel: StationDetailViewModel;
  loadingAction: StationDetailRefreshAction | null;
  onRefresh: (action: StationDetailRefreshAction) => void;
}) {
  return (
    <section className="rounded-lg border border-border bg-white shadow-sm">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-3">
        <div>
          <h2 className="text-base font-semibold text-slate-950">分组与倍率</h2>
          <p className="mt-1 text-sm text-muted-foreground">中转站可见分组、绑定状态和倍率来源。</p>
        </div>
        <Button variant="outline" disabled={Boolean(loadingAction)} onClick={() => onRefresh("groups")}>
          <RefreshCw className={cn("h-4 w-4", loadingAction === "groups" && "animate-spin")} />
          采集分组倍率
        </Button>
      </div>
      {viewModel.groupRows.length === 0 ? (
        <div className="p-6">
          <EmptyState title="暂无分组倍率" description={viewModel.groupEmptyMessage} />
        </div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full min-w-[780px] text-left text-sm">
            <thead className="bg-slate-50 text-xs text-muted-foreground">
              <tr>
                <th className="px-4 py-3 font-medium">分组</th>
                <th className="px-4 py-3 font-medium">有效倍率</th>
                <th className="px-4 py-3 font-medium">默认倍率</th>
                <th className="px-4 py-3 font-medium">用户倍率</th>
                <th className="px-4 py-3 font-medium">绑定状态</th>
                <th className="px-4 py-3 font-medium">来源</th>
                <th className="px-4 py-3 font-medium">采集时间</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {viewModel.groupRows.map((row) => (
                <tr key={row.id} className="align-top">
                  <td className="px-4 py-3">
                    <div className="font-medium text-slate-950">{row.groupName}</div>
                    <div className="mt-1 max-w-[240px] truncate text-xs text-muted-foreground">{row.groupId}</div>
                    {row.warning && <div className="mt-1 text-xs text-amber-700">{row.warning}</div>}
                  </td>
                  <td className={cn("px-4 py-3 font-medium", toneClass(row.tone))}>{row.effectiveRate}</td>
                  <td className="px-4 py-3 text-slate-700">{row.defaultRate}</td>
                  <td className="px-4 py-3 text-slate-700">{row.userRate}</td>
                  <td className="px-4 py-3 text-slate-700">{row.bindingStatus}</td>
                  <td className="px-4 py-3 text-slate-700">{row.source}</td>
                  <td className="px-4 py-3 text-slate-700">{row.lastChecked}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function StationDiagnosticsSection({ viewModel }: { viewModel: StationDetailViewModel }) {
  return (
    <section className="grid gap-3 lg:grid-cols-4">
      <DiagnosticCard title="登录与密钥" items={viewModel.loginItems} />
      <DiagnosticCard title="采集任务" items={viewModel.collectorItems} />
      <DiagnosticCard title="最新快照" items={viewModel.snapshotItems} />
      <DiagnosticCard title="相关变化" items={viewModel.changeItems.length > 0 ? viewModel.changeItems : [{ label: "状态", value: "暂无未处理变化", tone: "muted" }]} />
    </section>
  );
}

function DiagnosticCard({
  title,
  items,
}: {
  title: string;
  items: Array<{ label: string; value: string; tone: "neutral" | "good" | "warning" | "error" | "muted" }>;
}) {
  return (
    <div className="rounded-lg border border-border bg-white p-4 shadow-sm">
      <h3 className="text-sm font-semibold text-slate-950">{title}</h3>
      <div className="mt-3 space-y-2">
        {items.map((item) => (
          <div key={`${item.label}-${item.value}`} className="flex items-start justify-between gap-3 text-sm">
            <span className="text-muted-foreground">{item.label}</span>
            <span className={cn("text-right font-medium", toneClass(item.tone))}>{item.value}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function toneClass(tone: "neutral" | "good" | "warning" | "error" | "muted") {
  if (tone === "good") {
    return "text-emerald-700";
  }
  if (tone === "warning") {
    return "text-amber-700";
  }
  if (tone === "error") {
    return "text-rose-700";
  }
  if (tone === "muted") {
    return "text-muted-foreground";
  }
  return "text-slate-950";
}
```

- [ ] **Step 4: Run build and fix component import names**

Run: `pnpm.cmd build`

Expected: `tsc --noEmit` succeeds. If `Button` or `StatusBadge` export names differ in `src/components/ui/index.ts`, adjust only the imports in `src/features/stations/components/StationDetailContent.tsx` to match the existing barrel exports.

- [ ] **Step 5: Commit Task 2**

```powershell
git add -- src/features/stations/components/StationDetailContent.tsx
git commit -m "feat: add station detail content"
```

## Task 3: Build The Data-Loading Page

**Files:**
- Create: `src/features/stations/StationDetailPage.tsx`

- [ ] **Step 1: Add imports, props, and state**

Create:

```tsx
import { useCallback, useEffect, useMemo, useState } from "react";
import { Button, EmptyState } from "@/components/ui";
import { useToast } from "@/components/ui/ToastProvider";
import { collectStationTask } from "@/lib/api/collector";
import { getLatestCollectorSnapshot } from "@/lib/api/collector";
import { listCollectorRuns } from "@/lib/api/collectorRuns";
import { listChangeEvents } from "@/lib/api/changeEvents";
import { listBalanceSnapshots } from "@/lib/api/economics";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import { getStationCredentials, listStationKeys } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { CollectorSnapshot, CollectorTaskType } from "@/lib/types/collector";
import type { CollectorRun } from "@/lib/types/collectorRuns";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationCredentials, StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import { buildStationDetailViewModel } from "@/features/stations/stationDetailViewModels";
import { StationDetailContent, type StationDetailRefreshAction } from "@/features/stations/components/StationDetailContent";

type StationDetailData = {
  station: Station | null;
  balances: BalanceSnapshot[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  collectorRuns: CollectorRun[];
  latestSnapshot: CollectorSnapshot | null;
  credentials: StationCredentials | null;
  stationKeys: StationKey[];
  changes: ChangeEvent[];
};

const emptyDetailData: StationDetailData = {
  station: null,
  balances: [],
  groupBindings: [],
  groupRates: [],
  collectorRuns: [],
  latestSnapshot: null,
  credentials: null,
  stationKeys: [],
  changes: [],
};

export type StationDetailPageProps = {
  stationId: string | null;
  onBack: () => void;
  onEditProvider: (stationId: string) => void;
};
```

- [ ] **Step 2: Add the page component and data loader**

Append:

```tsx
export function StationDetailPage({ stationId, onBack, onEditProvider }: StationDetailPageProps) {
  const toast = useToast();
  const [data, setData] = useState<StationDetailData>(emptyDetailData);
  const [loading, setLoading] = useState(true);
  const [pageError, setPageError] = useState<string | null>(null);
  const [sectionError, setSectionError] = useState<string | null>(null);
  const [loadingAction, setLoadingAction] = useState<StationDetailRefreshAction | null>(null);

  const loadDetail = useCallback(async (id: string, mode: "initial" | "silent" = "silent") => {
    if (mode === "initial") {
      setLoading(true);
      setPageError(null);
    }
    const [
      stations,
      credentials,
      stationKeys,
      groupBindings,
      groupRates,
      collectorRuns,
      latestSnapshot,
      balances,
      changes,
    ] = await Promise.all([
      listStations(),
      getStationCredentials(id),
      listStationKeys(id),
      listStationGroupBindings(id),
      listGroupRateRecords(id),
      listCollectorRuns(id),
      getLatestCollectorSnapshot(id),
      listBalanceSnapshots(),
      listChangeEvents(),
    ]);
    const station = stations.find((item) => item.id === id) ?? null;
    if (!station) {
      throw new Error("未找到中转站");
    }
    setData({
      station,
      credentials,
      stationKeys,
      groupBindings,
      groupRates,
      collectorRuns,
      latestSnapshot,
      balances: balances.filter((item) => item.stationId === id),
      changes: changes.filter((item) => item.stationId === id),
    });
    setLoading(false);
  }, []);
```

- [ ] **Step 3: Add effects and refresh actions**

Append inside `StationDetailPage`, after `loadDetail`:

```tsx
  useEffect(() => {
    if (!stationId) {
      setLoading(false);
      setPageError("未选择中转站");
      return;
    }

    let alive = true;
    setLoading(true);
    setPageError(null);
    loadDetail(stationId, "initial")
      .catch((error) => {
        if (!alive) {
          return;
        }
        const message = readError(error);
        setPageError(message);
        toast.error("读取中转站详情失败", message);
      })
      .finally(() => {
        if (alive) {
          setLoading(false);
        }
      });

    return () => {
      alive = false;
    };
  }, [loadDetail, stationId, toast]);

  const viewModel = useMemo(() => {
    if (!data.station) {
      return null;
    }
    return buildStationDetailViewModel(data);
  }, [data]);

  async function handleRefresh(action: StationDetailRefreshAction) {
    if (!stationId || loadingAction) {
      return;
    }
    const taskType: CollectorTaskType = action === "groups" ? "groups" : action === "full" ? "full" : "balance";
    setLoadingAction(action);
    setSectionError(null);
    try {
      await collectStationTask(stationId, taskType);
      await loadDetail(stationId, "silent");
      toast.success(action === "balance" ? "余额已刷新" : action === "groups" ? "分组倍率已采集" : "中转站信息已重新采集");
    } catch (error) {
      const message = readError(error);
      setSectionError(message);
      toast.error("采集失败", message);
    } finally {
      setLoadingAction(null);
    }
  }
```

- [ ] **Step 4: Add render states**

Append the final return block and helper:

```tsx
  if (loading && !viewModel) {
    return (
      <div className="grid min-h-[360px] place-items-center">
        <div className="text-sm text-muted-foreground">正在读取中转站详情...</div>
      </div>
    );
  }

  if (pageError || !viewModel) {
    return (
      <div className="grid min-h-[420px] place-items-center">
        <EmptyState
          title="无法打开中转站详情"
          description={pageError ?? "详情数据不存在。"}
          action={<Button onClick={onBack}>返回中转站资产</Button>}
        />
      </div>
    );
  }

  return (
    <StationDetailContent
      viewModel={viewModel}
      loadingAction={loadingAction}
      sectionError={sectionError}
      onBack={onBack}
      onEdit={() => onEditProvider(viewModel.station.id)}
      onRefresh={(action) => void handleRefresh(action)}
    />
  );
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : "操作失败";
}
```

- [ ] **Step 5: Run build and fix API import paths**

Run: `pnpm.cmd build`

Expected: `tsc --noEmit` and `vite build` complete.

- [ ] **Step 6: Commit Task 3**

```powershell
git add -- src/features/stations/StationDetailPage.tsx
git commit -m "feat: load station detail page data"
```

## Task 4: Wire Page Navigation

**Files:**
- Modify: `src/lib/types/navigation.ts`
- Modify: `src/app/App.tsx`

- [ ] **Step 1: Extend `AppPageId`**

Change `src/lib/types/navigation.ts` to:

```typescript
export type AppPageId = AppRouteId | "addProvider" | "editProvider" | "stationDetail";
```

- [ ] **Step 2: Import the detail page and add selected state**

In `src/app/App.tsx`, add the import:

```typescript
import { StationDetailPage } from "@/features/stations/StationDetailPage";
```

Add state after `editingStationId`:

```typescript
const [detailStationId, setDetailStationId] = useState<string | null>(null);
```

- [ ] **Step 3: Add open handlers**

Add these functions inside `App`:

```typescript
function openStationDetail(stationId: string) {
  setDetailStationId(stationId);
  setActiveRouteId("stationDetail");
}

function returnToStations() {
  setEditingStationId(null);
  setDetailStationId(null);
  setActiveRouteId("stations");
}
```

Update `openEditProvider` to keep the detail station id only as navigation memory:

```typescript
function openEditProvider(stationId: string) {
  setEditingStationId(stationId);
  setActiveRouteId("editProvider");
}
```

- [ ] **Step 4: Keep the shell highlight on 中转站资产**

Change `activeShellRouteId` to:

```typescript
const activeShellRouteId: AppRouteId =
  activeRouteId === "addProvider" || activeRouteId === "editProvider" || activeRouteId === "stationDetail"
    ? "stations"
    : activeRouteId;
```

- [ ] **Step 5: Add the route branch**

Add this `case` before `case "stations"`:

```tsx
case "stationDetail":
  return (
    <StationDetailPage
      stationId={detailStationId}
      onBack={returnToStations}
      onEditProvider={openEditProvider}
    />
  );
```

Update the existing `stations` branch:

```tsx
case "stations":
  return (
    <StationsPage
      onAddProvider={() => setActiveRouteId("addProvider")}
      onEditProvider={openEditProvider}
      onOpenStation={openStationDetail}
    />
  );
```

Update add/edit callbacks to use `returnToStations`:

```tsx
<AddProviderPage
  onBack={returnToStations}
  onCreated={returnToStations}
/>
```

```tsx
<AddProviderPage
  stationId={editingStationId}
  onBack={returnToStations}
  onUpdated={returnToStations}
/>
```

- [ ] **Step 6: Update `useMemo` dependencies**

The dependency array should include all route callbacks and state used by the switch:

```typescript
}, [activeRouteId, detailStationId, editingStationId]);
```

If TypeScript reports missing dependencies because handlers are wrapped in `useCallback`, include those handler names instead of adding ESLint-only churn; this repository currently does not enforce React Hooks linting in `pnpm.cmd build`.

- [ ] **Step 7: Run build**

Run: `pnpm.cmd build`

Expected: `tsc --noEmit` and `vite build` complete.

- [ ] **Step 8: Commit Task 4**

```powershell
git add -- src/lib/types/navigation.ts src/app/App.tsx
git commit -m "feat: route station detail page"
```

## Task 5: Make Station Row Click Open The Page

**Files:**
- Modify: `src/features/stations/StationsPage.tsx`

- [ ] **Step 1: Extend list props**

Change `StationsPageProps`:

```typescript
type StationsPageProps = {
  onAddProvider?: () => void;
  onEditProvider?: (stationId: string) => void;
  onOpenStation?: (stationId: string) => void;
};
```

Change the component signature:

```typescript
export function StationsPage({ onAddProvider, onEditProvider, onOpenStation }: StationsPageProps) {
```

- [ ] **Step 2: Route normal row open to the page callback**

Change `openDetail` to:

```typescript
const openDetail = useCallback((station: Station) => {
  if (onOpenStation) {
    setDialogMode(null);
    setDetailStationId(null);
    setDrawerStationId(null);
    setDrawerVisible(false);
    setDrawerClosing(false);
    setError(null);
    onOpenStation(station.id);
    return;
  }

  const restoringCurrentDrawer = drawerStationId === station.id;
  setDialogMode("detail");
  setDetailStationId(station.id);
  setDrawerStationId(station.id);
  setDrawerClosing(false);
  if (restoringCurrentDrawer) {
    setDrawerVisible(true);
  }
  setError(null);
  void refreshExtras(station.id);
}, [drawerStationId, onOpenStation, refreshExtras]);
```

- [ ] **Step 3: Remove primary drawer-only actions from the page path**

Keep the existing drawer markup in place as fallback. Do not add any new key create/edit/delete entry to the new page. Verify the row action buttons still stop propagation:

```tsx
onClick={(event) => {
  event.stopPropagation();
  onRefreshBalance(station);
}}
```

The normal click on the row must call `onOpen(station)` and therefore reach `onOpenStation` from `App`.

- [ ] **Step 4: Run build**

Run: `pnpm.cmd build`

Expected: `tsc --noEmit` and `vite build` complete.

- [ ] **Step 5: Commit Task 5**

```powershell
git add -- src/features/stations/StationsPage.tsx
git commit -m "feat: open station details as a page"
```

## Task 6: Browser Smoke And Final Cleanup

**Files:**
- Verify: `src/app/App.tsx`
- Verify: `src/features/stations/StationsPage.tsx`
- Verify: `src/features/stations/StationDetailPage.tsx`
- Verify: `src/features/stations/components/StationDetailContent.tsx`
- Verify: `src/features/stations/stationDetailViewModels.ts`

- [ ] **Step 1: Start Vite**

Run:

```powershell
pnpm.cmd dev -- --port 1430
```

Expected: Vite prints a local URL containing `http://127.0.0.1:1430/`.

- [ ] **Step 2: Smoke station list to detail**

Open `http://127.0.0.1:1430/`.

Expected:
- The app loads.
- Navigate to `中转站资产`.
- Click a station row.
- The visible page contains `返回中转站资产`, `当前余额`, and `分组与倍率`.
- No right-side overlay or dialog is visible after row click.

- [ ] **Step 3: Smoke information hierarchy**

On the station detail page, verify:
- The balance cards are above the `分组与倍率` panel.
- `分组与倍率` has the largest table/panel footprint in the page body.
- `登录与密钥`, `采集任务`, `最新快照`, and `相关变化` appear below the group/rate panel.
- There are no inline account, password, Base URL, API key, threshold, note, key create, key edit, key delete, or station delete controls on the detail page.

- [ ] **Step 4: Smoke refresh actions**

Click `刷新余额`, `采集分组倍率`, and `重新采集` one at a time.

Expected:
- The clicked action shows a spinning icon or disabled loading state.
- Existing balance and group/rate content remains visible while the request runs.
- A browser-preview fallback may show a toast because Tauri invoke is unavailable; the page must keep the previous content visible.

- [ ] **Step 5: Smoke edit and back navigation**

Click `编辑供应商`.

Expected:
- The app opens the edit-provider page.
- The page title indicates editing a supplier.
- Returning from that page goes back to `中转站资产`.

Return to a station detail page and click `返回中转站资产`.

Expected:
- The station list is visible.
- The left navigation highlight stays on `中转站资产`.

- [ ] **Step 6: Run final build**

Run: `pnpm.cmd build`

Expected: `tsc --noEmit` and `vite build` complete.

- [ ] **Step 7: Capture final status**

Run:

```powershell
git status --short -- . ':(exclude).pnpm-store/**'
```

Expected: only files changed by this station detail implementation and pre-existing unrelated dirty files are listed. Stage only exact station-detail implementation paths.

- [ ] **Step 8: Commit final verification adjustments**

If smoke required small visual or type adjustments, commit only the exact paths touched by those adjustments:

```powershell
git add -- src/app/App.tsx src/lib/types/navigation.ts src/features/stations/StationsPage.tsx src/features/stations/StationDetailPage.tsx src/features/stations/components/StationDetailContent.tsx src/features/stations/stationDetailViewModels.ts
git commit -m "fix: polish station detail page smoke"
```

If Step 8 has no file changes, do not create an empty commit.

## Out Of Scope Guardrails

- Do not add a station-detail display-mode setting in this implementation.
- Do not move Key Pool management into station detail.
- Do not add inline editing for login account, password, Base URL, API key, low-balance threshold, station note, or station key records.
- Do not add destructive delete actions to the detail page header.
- Do not change Rust/Tauri commands or services unless an existing API wrapper is missing at build time. If Rust is changed, run `cargo check --manifest-path src-tauri/Cargo.toml` and commit the Rust paths separately.
- Do not use `git add .`, `git add -A`, or `git commit -a`.

## Final Verification Commands

Run these before claiming completion:

```powershell
pnpm.cmd build
git status --short -- . ':(exclude).pnpm-store/**'
```

Browser smoke must verify the page path, information hierarchy, refresh loading behavior, edit navigation, and back navigation described in Task 6.
