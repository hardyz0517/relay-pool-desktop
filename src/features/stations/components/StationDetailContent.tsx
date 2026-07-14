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
import { StationGroupNameBadge, StationGroupRateBadge } from "./StationGroupChip";

export type StationDetailRefreshAction = "balance" | "groups" | "full";
export type StationDetailLoadingAction = StationDetailRefreshAction | "authorize";

export type StationDetailContentProps = {
  viewModel: StationDetailViewModel;
  loadingAction: StationDetailLoadingAction | null;
  sectionError: string | null;
  onBack: () => void;
  onEdit: () => void;
  onAuthorize: () => void;
  onRefresh: (action: StationDetailRefreshAction) => void;
};

const statusToneByDetailTone: Record<DetailTone, StatusTone> = {
  neutral: "info",
  good: "healthy",
  warning: "warning",
  error: "error",
  muted: "disabled",
};

const textToneClassName: Record<DetailTone, string> = {
  neutral: "text-slate-700",
  good: "text-emerald-700",
  warning: "text-amber-700",
  error: "text-rose-700",
  muted: "text-slate-500",
};

const surfaceToneClassName: Record<DetailTone, string> = {
  neutral: "border-border bg-white",
  good: "border-emerald-100 bg-emerald-50/60",
  warning: "border-amber-100 bg-amber-50/70",
  error: "border-rose-100 bg-rose-50/70",
  muted: "border-border bg-slate-50",
};

const usageCardVisualMeta = {
  request: {
    Icon: Activity,
    iconClassName: "bg-green-100 text-green-700",
    valueClassName: "text-green-700",
  },
  consumption: {
    Icon: BadgeDollarSign,
    iconClassName: "bg-purple-100 text-purple-700",
    valueClassName: "text-purple-700",
  },
  todayToken: {
    Icon: BarChart3,
    iconClassName: "bg-amber-100 text-amber-700",
    valueClassName: "text-amber-700",
  },
  totalToken: {
    Icon: Server,
    iconClassName: "bg-indigo-100 text-indigo-700",
    valueClassName: "text-indigo-700",
  },
} satisfies Record<string, { Icon: LucideIcon; iconClassName: string; valueClassName: string }>;

export function StationDetailContent({
  viewModel,
  loadingAction,
  sectionError,
  onBack,
  onEdit,
  onAuthorize,
  onRefresh,
}: StationDetailContentProps) {
  const station = viewModel.station;
  const actionBusy = loadingAction !== null;

  return (
    <PageScaffold
      title="中转站详情"
      stickyHeader
      backAction={
        <IconButton label="返回中转站资产" onClick={onBack}>
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
            <IconButton label={`重新授权 ${station.name}`} disabled={actionBusy} onClick={onAuthorize}>
              <ShieldCheck className={cn("h-4 w-4", loadingAction === "authorize" && "animate-pulse")} />
            </IconButton>
          )}
          <Button variant="ghost" size="sm" onClick={onEdit}>
            <Edit3 className="h-3.5 w-3.5" />
            编辑供应商
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <header className="rounded-[var(--surface-radius)] border border-border bg-white px-4 py-3 shadow-[var(--surface-shadow)]">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
          <div className="min-w-0 space-y-2">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <h2 className="min-w-0 truncate text-xl font-semibold tracking-normal text-slate-900">
                {station.name}
              </h2>
              <StatusBadge tone={statusToneByDetailTone[viewModel.statusTone]}>
                {viewModel.statusLabel}
              </StatusBadge>
            </div>

            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
              <span>{viewModel.stationTypeLabel}</span>
              <span className="max-w-full truncate font-mono text-[11px] text-slate-600">
                {station.websiteUrl}
              </span>
              <span className="inline-flex items-center gap-1">
                <Clock3 className="h-3.5 w-3.5" />
                最近活动 {viewModel.lastActivityLabel}
              </span>
            </div>
          </div>
        </div>
        </header>

        {sectionError && (
          <div className="flex items-start gap-2 rounded-[var(--surface-radius)] border border-rose-200 bg-rose-50 px-3 py-2 text-xs text-rose-700">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            <span>{sectionError}</span>
          </div>
        )}

        <section className="rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]">
        <div className="flex items-center gap-2 border-b border-border px-4 py-3">
          <WalletCards className="h-4 w-4 text-slate-500" />
          <h2 className="text-sm font-semibold text-slate-900">余额</h2>
        </div>
        <div className="grid gap-3 p-4 md:grid-cols-3">
          {viewModel.balanceCards.map((card) => (
            <div
              key={card.label}
              className={cn(
                "min-h-[84px] rounded-[var(--surface-radius)] border px-3 py-2.5",
                surfaceToneClassName[card.tone],
              )}
            >
              <div className="text-xs text-muted-foreground">{card.label}</div>
              <div className={cn("mt-1 truncate text-lg font-semibold", textToneClassName[card.tone])}>
                {card.value}
              </div>
              <div className="mt-1 line-clamp-2 text-xs text-muted-foreground">{card.helper}</div>
            </div>
          ))}
        </div>
      </section>

      <section className="rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]">
        <div className="flex items-center gap-2 border-b border-border px-4 py-3">
          <BarChart3 className="h-4 w-4 text-slate-500" />
          <h2 className="text-sm font-semibold text-slate-900">中转站用量</h2>
        </div>
        <div className="grid gap-3 p-4 md:grid-cols-4">
          {viewModel.usageCards.map((card) => {
            const visual = usageCardVisualFor(card.label);
            return (
              <div
                key={card.label}
                className="flex min-h-[96px] items-center gap-3 rounded-[12px] border border-slate-200 bg-white px-4 py-3 shadow-[0_2px_8px_rgba(15,23,42,0.08)]"
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

      <section className="rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]">
        <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
          <div className="flex min-w-0 items-center gap-2">
            <Layers3 className="h-4 w-4 text-slate-500" />
            <h2 className="text-sm font-semibold text-slate-900">分组与倍率</h2>
          </div>
          <span className="text-xs text-muted-foreground">{viewModel.groupRows.length} 条记录</span>
        </div>
        <div className="p-4">
          {viewModel.groupRows.length === 0 ? (
            <div className="flex min-h-[148px] flex-col items-center justify-center px-4 py-8 text-center">
              <div className="flex h-9 w-9 items-center justify-center rounded-full bg-slate-100 text-slate-500">
                <Layers3 className="h-4 w-4" />
              </div>
              <div className="mt-3 text-sm font-medium text-slate-800">
                {viewModel.groupEmptyMessage}
              </div>
              <p className="mt-1 max-w-md text-xs leading-5 text-muted-foreground">
                点击采集分组倍率或重新采集后，这里会显示站点分组、默认倍率与用户倍率。
              </p>
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="min-w-[760px] w-full border-separate border-spacing-0 text-left text-xs">
                <thead>
                  <tr className="text-muted-foreground">
                    <TableHead className="pl-0">分组</TableHead>
                    <TableHead>
                      <RateHead title="生效倍率" helper="实际采用" />
                    </TableHead>
                    <TableHead>
                      <RateHead title="默认倍率" helper="站点采集" />
                    </TableHead>
                    <TableHead>
                      <RateHead title="用户倍率" helper="手动覆盖" />
                    </TableHead>
                    <TableHead>绑定状态</TableHead>
                    <TableHead>采集来源</TableHead>
                    <TableHead className="pr-0">最近检查</TableHead>
                  </tr>
                </thead>
                <tbody>
                  {viewModel.groupRows.map((row) => (
                    <tr key={row.id} className="border-t border-border transition-colors hover:bg-slate-50/70">
                      <TableCell className="max-w-[220px] pl-0">
                        <StationGroupNameBadge
                          groupName={row.groupName}
                          rawJsonRedacted={row.rawJsonRedacted}
                          effectiveGroupCategory={row.effectiveGroupCategory}
                        />
                        {row.warning && (
                          <div className="mt-1 inline-flex items-center gap-1 text-amber-700">
                            <AlertTriangle className="h-3.5 w-3.5" />
                            {row.warning}
                          </div>
                        )}
                      </TableCell>
                      <TableCell>
                        <StationGroupRateBadge
                          groupName={row.groupName}
                          rawJsonRedacted={row.rawJsonRedacted}
                          effectiveGroupCategory={row.effectiveGroupCategory}
                          label={row.effectiveRate}
                        />
                      </TableCell>
                      <TableCell>{row.defaultRate}</TableCell>
                      <TableCell>{row.userRate}</TableCell>
                      <TableCell>
                        <StatusBadge tone={statusToneByDetailTone[row.tone]}>{row.bindingStatus}</StatusBadge>
                      </TableCell>
                      <TableCell>{row.sourceLabel}</TableCell>
                      <TableCell className="pr-0">{row.lastChecked}</TableCell>
                    </tr>
                  ))}
                </tbody>
              </table>
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

function TableHead({
  className,
  children,
}: {
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <th className={cn("border-b border-border px-3 pb-2 font-medium", className)}>
      {children}
    </th>
  );
}

function TableCell({
  className,
  children,
}: {
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <td className={cn("border-b border-border px-3 py-2.5 align-top text-slate-700", className)}>
      {children}
    </td>
  );
}

function RateHead({ title, helper }: { title: string; helper: string }) {
  return (
    <span className="block leading-tight">
      <span className="block text-slate-600">{title}</span>
      <span className="mt-0.5 block text-[11px] font-normal text-muted-foreground">{helper}</span>
    </span>
  );
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
    <section className="rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]">
      <div className="flex items-center gap-2 border-b border-border px-4 py-3">
        <Icon className="h-4 w-4 text-slate-500" />
        <h2 className="text-sm font-semibold text-slate-900">{title}</h2>
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
