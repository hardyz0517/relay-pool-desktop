import {
  Activity,
  AlertTriangle,
  ArrowLeft,
  BadgeDollarSign,
  BarChart3,
  Clock3,
  Database,
  Edit3,
  KeyRound,
  Layers3,
  RefreshCw,
  Route,
  RotateCw,
  Server,
  ShieldCheck,
  WalletCards,
  type LucideIcon,
} from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, IconButton, StatusBadge, type StatusTone } from "@/components/ui";
import { cn } from "@/lib/utils";
import type {
  DetailTone,
  StationDetailDiagnosticItem,
  StationDetailViewModel,
} from "../stationDetailViewModels";
import { StationGroupNameBadge, StationGroupRateBadge } from "@/components/group/StationGroupChip";

export type StationDetailRefreshAction = "balance" | "groups" | "full";
export type StationDetailLoadingAction = StationDetailRefreshAction | "authorize";

export type StationDetailContentProps = {
  viewModel: StationDetailViewModel;
  loadingAction: StationDetailLoadingAction | null;
  sectionError: string | null;
  onBack: () => void;
  backLabel?: string;
  onEdit: () => void;
  onOpenWebsite?: () => void;
  onOpenRechargeCenter: () => void;
  onOpenRoutingDeepLink?: () => void;
  onAuthorize: () => void;
  onRefresh: (action: StationDetailRefreshAction) => void;
  publishedStatusSection?: React.ReactNode;
};

const statusToneByDetailTone: Record<DetailTone, StatusTone> = {
  neutral: "info",
  good: "healthy",
  warning: "warning",
  error: "error",
  muted: "disabled",
};

const textToneClassName: Record<DetailTone, string> = {
  neutral: "text-foreground",
  good: "text-success-foreground",
  warning: "text-warning-foreground",
  error: "text-danger-foreground",
  muted: "text-muted-foreground",
};

const usageCardVisualMeta = {
  balance: {
    Icon: WalletCards,
    iconClassName: "bg-success-surface text-success-foreground",
    valueClassName: "text-success-foreground",
  },
  request: {
    Icon: Activity,
    iconClassName: "bg-success-surface text-success-foreground",
    valueClassName: "text-success-foreground",
  },
  consumption: {
    Icon: BadgeDollarSign,
    iconClassName: "bg-platform-image-surface text-platform-image-foreground",
    valueClassName: "text-platform-image-foreground",
  },
  todayToken: {
    Icon: BarChart3,
    iconClassName: "bg-warning-surface text-warning-foreground",
    valueClassName: "text-warning-foreground",
  },
  totalToken: {
    Icon: Server,
    iconClassName: "bg-platform-gemini-surface text-platform-gemini-foreground",
    valueClassName: "text-platform-gemini-foreground",
  },
  concurrency: {
    Icon: Activity,
    iconClassName: "bg-platform-gemini-surface text-platform-gemini-foreground",
    valueClassName: "text-platform-gemini-foreground",
  },
} satisfies Record<string, { Icon: LucideIcon; iconClassName: string; valueClassName: string }>;

export function StationDetailContent({
  viewModel,
  loadingAction,
  sectionError,
  onBack,
  backLabel = "返回中转站资产",
  onEdit,
  onOpenWebsite,
  onOpenRechargeCenter,
  onOpenRoutingDeepLink,
  onAuthorize,
  onRefresh,
  publishedStatusSection,
}: StationDetailContentProps) {
  const station = viewModel.station;
  const actionBusy = loadingAction !== null;

  return (
    <PageScaffold
      title="中转站详情"
      stickyHeader
      backAction={
        <IconButton label={backLabel} onClick={onBack}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
      actions={
        <>
          <Button
            variant="secondary"
            size="sm"
            disabled={actionBusy}
            onClick={() => onRefresh("balance")}
          >
            <RefreshCw className={cn("h-3.5 w-3.5", loadingAction === "balance" && "animate-spin")} />
            刷新余额
          </Button>
          <Button variant="secondary" size="sm" onClick={onOpenRechargeCenter}>
            <WalletCards className="h-3.5 w-3.5" />
            充值中心
          </Button>
          <Button
            variant="secondary"
            size="sm"
            disabled={actionBusy}
            onClick={() => onRefresh("groups")}
          >
            <Layers3 className={cn("h-3.5 w-3.5", loadingAction === "groups" && "animate-pulse")} />
            采集分组倍率
          </Button>
          <Button
            variant="primary"
            size="sm"
            disabled={actionBusy}
            onClick={() => onRefresh("full")}
          >
            <RotateCw className={cn("h-3.5 w-3.5", loadingAction === "full" && "animate-spin")} />
            重新采集
          </Button>
          {station.stationType === "sub2api" && (
            <Button
              variant="secondary"
              size="sm"
              aria-label={`重新授权 ${station.name}`}
              title={`重新授权 ${station.name}`}
              disabled={actionBusy}
              onClick={onAuthorize}
            >
              <ShieldCheck className={cn("h-4 w-4", loadingAction === "authorize" && "animate-pulse")} />
              窗口授权
            </Button>
          )}
          <Button variant="ghost" size="sm" onClick={onEdit}>
            <Edit3 className="h-3.5 w-3.5" />
            编辑供应商
          </Button>
          {onOpenRoutingDeepLink ? (
            <Button variant="ghost" size="sm" onClick={onOpenRoutingDeepLink}>
              <Route className="h-3.5 w-3.5" />
              查看路由影响
            </Button>
          ) : null}
        </>
      }
    >
      <div className="space-y-4">
        <header className="rounded-[var(--surface-radius)] border border-border bg-surface px-4 py-3 shadow-[var(--surface-shadow)]">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
          <div className="min-w-0 space-y-2">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <h2 className="min-w-0 truncate text-xl font-semibold tracking-normal text-foreground">
                {station.name}
              </h2>
              <StatusBadge tone={statusToneByDetailTone[viewModel.statusTone]}>
                {viewModel.statusLabel}
              </StatusBadge>
            </div>

            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
              <span>{viewModel.stationTypeLabel}</span>
              <button
                type="button"
                aria-label={`在浏览器打开 ${station.name}`}
                title={station.websiteUrl}
                className="max-w-full truncate font-mono text-[11px] text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
                onClick={onOpenWebsite}
                disabled={!onOpenWebsite}
              >
                {station.websiteUrl}
              </button>
              <span className="inline-flex items-center gap-1">
                <Clock3 className="h-3.5 w-3.5" />
                最近活动 {viewModel.lastActivityLabel}
              </span>
            </div>
          </div>
        </div>
        </header>

        {sectionError && (
          <div className="flex items-start gap-2 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-xs text-danger-foreground">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            <span>{sectionError}</span>
          </div>
        )}

      <section className="rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]">
        <div className="flex items-center gap-2 border-b border-border px-4 py-3">
          <BarChart3 className="h-4 w-4 text-muted-foreground" />
          <h2 className="text-sm font-semibold text-foreground">中转站指标</h2>
        </div>
        <div className="grid gap-3 p-4 md:grid-cols-3">
          {viewModel.metricCards.map((card) => {
            const visual = usageCardVisualFor(card.label);
            return (
              <div
                key={card.label}
                className="flex min-h-[96px] items-center gap-3 rounded-[12px] border border-border bg-surface px-4 py-3 shadow-surface"
              >
                <div
                  className={cn(
                    "flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px]",
                    visual.iconClassName,
                  )}
                >
                  <visual.Icon className="h-4 w-4" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-xs text-muted-foreground">{card.label}</div>
                  <div className={cn("mt-0.5 truncate text-[22px] font-semibold leading-7", visual.valueClassName)}>
                    {card.value}
                  </div>
                  <div className="mt-0.5 truncate text-xs text-muted-foreground">{card.helper}</div>
                </div>
              </div>
            );
          })}
        </div>
      </section>

      {publishedStatusSection}

      <section className="rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]">
        <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
          <div className="flex min-w-0 items-center gap-2">
            <Layers3 className="h-4 w-4 text-muted-foreground" />
            <h2 className="text-sm font-semibold text-foreground">分组与倍率</h2>
          </div>
          <span className="text-xs text-muted-foreground">{viewModel.groupRows.length} 条记录</span>
        </div>
        <div className="p-4">
          {viewModel.groupRows.length === 0 ? (
            <div className="flex min-h-[148px] flex-col items-center justify-center px-4 py-8 text-center">
              <div className="flex h-9 w-9 items-center justify-center rounded-full bg-muted text-muted-foreground">
                <Layers3 className="h-4 w-4" />
              </div>
              <div className="mt-3 text-sm font-medium text-foreground">
                {viewModel.groupEmptyMessage}
              </div>
              <p className="mt-1 max-w-md text-xs leading-5 text-muted-foreground">
                点击采集分组倍率或重新采集后，这里会显示站点分组、当前倍率与倍率来源。
              </p>
            </div>
          ) : (
            <div className="relative">
              <div
                aria-hidden="true"
                className="pointer-events-none absolute inset-y-0 left-1/2 z-10 hidden w-px -translate-x-1/2 bg-border/80 md:block"
              />
              <div className="grid grid-cols-1 md:grid-cols-2" role="list">
                {viewModel.groupRows.map((row) => (
                  <div
                    key={row.id}
                    className="min-w-0 border-b border-border/80 px-0 py-2 md:px-4 md:[&:nth-child(odd)]:pl-0 md:[&:nth-child(even)]:pr-0"
                    role="listitem"
                  >
                    <div className="flex min-w-0 items-center justify-between gap-3">
                      <div className="min-w-0 flex-1">
                        <StationGroupNameBadge
                          groupName={row.groupName}
                          rawJsonRedacted={row.rawJsonRedacted}
                          effectiveGroupCategory={row.effectiveGroupCategory}
                        />
                      </div>
                      <StationGroupRateBadge
                        groupName={row.groupName}
                        rawJsonRedacted={row.rawJsonRedacted}
                        effectiveGroupCategory={row.effectiveGroupCategory}
                        label={row.effectiveRate}
                      />
                    </div>
                    <div className="mt-0.5 flex min-w-0 items-center justify-between gap-3 text-[11px] leading-4">
                      <span
                        className="min-w-0 flex-1 truncate text-muted-foreground"
                        title={row.description ?? undefined}
                      >
                        {row.description ?? ""}
                      </span>
                      <div className="flex min-w-0 shrink-0 items-center justify-end gap-1 text-right text-muted-foreground">
                        <span className="truncate">
                          {row.rateSource} · {row.lastChecked}
                        </span>
                        {row.warning ? (
                          <span className="shrink-0 text-warning-foreground" title={row.warning}>
                            <AlertTriangle className="h-3.5 w-3.5" aria-label={row.warning} />
                          </span>
                        ) : null}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </section>

      <div className="grid gap-4">
        <DiagnosticSection icon={KeyRound} title="登录与密钥" items={viewModel.loginItems} />
        <DiagnosticSection icon={RefreshCw} title="采集任务" items={viewModel.collectorItems} />
        <DiagnosticSection icon={Database} title="最新快照" items={viewModel.snapshotItems} />
        <DiagnosticSection icon={AlertTriangle} title="相关变化" items={viewModel.changeItems} />
      </div>
      </div>
    </PageScaffold>
  );
}

function usageCardVisualFor(label: string) {
  if (label.includes("余额")) {
    return usageCardVisualMeta.balance;
  }
  if (label.includes("并发")) {
    return usageCardVisualMeta.concurrency;
  }
  if (label.includes("请求")) {
    return usageCardVisualMeta.request;
  }
  if (label.includes("消费")) {
    return usageCardVisualMeta.consumption;
  }
  if (label.includes("今日 Token")) {
    return usageCardVisualMeta.todayToken;
  }
  if (label.includes("累计 Token")) {
    return usageCardVisualMeta.totalToken;
  }
  return usageCardVisualMeta.request;
}

function DiagnosticSection({
  icon: Icon,
  title,
  items,
}: {
  icon: typeof KeyRound;
  title: string;
  items: StationDetailDiagnosticItem[];
}) {
  return (
    <section className="rounded-[var(--surface-radius)] border border-border bg-surface shadow-[var(--surface-shadow)]">
      <div className="flex items-center gap-2 border-b border-border px-4 py-3">
        <Icon className="h-4 w-4 text-muted-foreground" />
        <h2 className="text-sm font-semibold text-foreground">{title}</h2>
      </div>
      <dl className="divide-y divide-border px-4">
        {items.map((item) => (
          <div key={`${item.label}-${item.value}`} className="grid grid-cols-[112px_minmax(0,1fr)] gap-3 py-2.5 text-xs">
            <dt className="text-muted-foreground">{item.label}</dt>
            <dd className={cn("min-w-0 break-words font-medium", textToneClassName[item.tone])}>
              {item.value}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
