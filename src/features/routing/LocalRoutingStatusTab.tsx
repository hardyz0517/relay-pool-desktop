import { Activity, Clock3, ScrollText, Server, ShieldCheck } from "lucide-react";
import { Button, EmptyState, SectionCard, StatusBadge } from "@/components/ui";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";
import type { AppRouteId } from "@/lib/types/navigation";
import { LocalRoutingCandidateRow } from "./LocalRoutingCandidateRow";
import { RouteExplanationPanel } from "./RouteExplanationPanel";

type LocalRoutingLinkedPage = Extract<AppRouteId, "channels" | "logs">;

type LocalRoutingStatusTabProps = {
  workspace: LocalRoutingWorkspace | null;
  loading: boolean;
  onOpenPage?: (pageId: LocalRoutingLinkedPage) => void;
};

export function LocalRoutingStatusTab({ workspace, loading, onOpenPage }: LocalRoutingStatusTabProps) {
  function openLinkedPage(pageId: LocalRoutingLinkedPage) {
    if (onOpenPage) {
      onOpenPage(pageId);
      return;
    }
    window.dispatchEvent(new CustomEvent("relay-pool:open-page", { detail: { pageId } }));
  }

  if (loading && !workspace) {
    return (
      <SectionCard title="本地路由状态">
        <div className="text-sm text-muted-foreground">正在加载本地路由工作区...</div>
      </SectionCard>
    );
  }

  if (!workspace) {
    return (
      <SectionCard title="本地路由状态">
        <EmptyState title="暂无本地路由数据" description="刷新后仍为空时，请检查本地路由预览接口。" />
      </SectionCard>
    );
  }

  const currentKey = workspace.latestDecision?.selectedStationName ?? "未选择";

  return (
    <div className="grid gap-3">
      <div className="grid gap-3 lg:grid-cols-[minmax(0,1.15fr)_minmax(280px,0.85fr)]">
        <SectionCard title="本地端点" contentClassName="grid gap-3">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex min-w-0 items-center gap-3">
              <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[10px] bg-teal-50 text-teal-700">
                <Server className="h-5 w-5" />
              </span>
              <div className="min-w-0">
                <div className="truncate text-sm font-semibold text-slate-900">
                  {workspace.settings.bindAddr}:{workspace.settings.port}
                </div>
                <div className="truncate text-xs text-muted-foreground">
                  {workspace.settings.endpoint} / {workspace.settings.policy}
                </div>
              </div>
            </div>
            <StatusBadge tone={workspace.proxyStatus.running ? "healthy" : "disabled"}>
              {workspace.proxyStatus.running ? "运行中" : "未启动"}
            </StatusBadge>
          </div>
          <div className="grid gap-2 text-xs text-slate-600 sm:grid-cols-3">
            <Metric label="可用候选" value={workspace.summary.healthyCandidateCount} />
            <Metric label="降级候选" value={workspace.summary.degradedCandidateCount} />
            <Metric label="冷却候选" value={workspace.summary.cooldownCandidateCount} />
          </div>
        </SectionCard>

        <SectionCard title="当前主 Key" contentClassName="grid gap-3">
          <div className="flex items-center gap-3">
            <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[10px] bg-emerald-50 text-emerald-700">
              <ShieldCheck className="h-5 w-5" />
            </span>
            <div className="min-w-0">
              <div className="truncate text-sm font-semibold text-slate-900">{currentKey}</div>
              <div className="truncate text-xs text-muted-foreground">
                {workspace.latestDecision?.reason ?? "等待下一次路由决策"}
              </div>
            </div>
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Clock3 className="h-3.5 w-3.5" />
            {workspace.summary.lastDecisionAt ?? "暂无决策时间"}
          </div>
        </SectionCard>
      </div>

      <SectionCard title="排障入口" contentClassName="flex flex-wrap gap-2">
        <Button variant="secondary" onClick={() => openLinkedPage("channels")}>
          <Activity className="h-4 w-4" />
          渠道状态
        </Button>
        <Button variant="secondary" onClick={() => openLinkedPage("logs")}>
          <ScrollText className="h-4 w-4" />
          请求日志
        </Button>
      </SectionCard>

      <SectionCard title="候选顺位" contentClassName="grid gap-2">
        {workspace.candidates.length === 0 ? (
          <EmptyState title="暂无候选 Key" description="后续任务会接入可编辑的本地路由候选列表。" />
        ) : (
          workspace.candidates.map((candidate) => (
            <LocalRoutingCandidateRow key={candidate.stationKeyId} candidate={candidate} />
          ))
        )}
      </SectionCard>

      <RouteExplanationPanel workspace={workspace} />
    </div>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-[var(--surface-radius)] border border-slate-200 bg-slate-50 px-3 py-2">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="text-sm font-semibold text-slate-900">{value}</div>
    </div>
  );
}
