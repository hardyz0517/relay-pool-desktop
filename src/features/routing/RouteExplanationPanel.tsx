import { Activity, GitBranch } from "lucide-react";
import { EmptyState, SectionCard, StatusBadge } from "@/components/ui";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";

type RouteExplanationPanelProps = {
  workspace: LocalRoutingWorkspace;
};

const statusTone: Record<NonNullable<LocalRoutingWorkspace["latestDecision"]>["status"], "healthy" | "warning" | "error" | "disabled"> = {
  selected: "healthy",
  fallback: "warning",
  failed: "error",
  unavailable: "disabled",
};

const statusLabel: Record<NonNullable<LocalRoutingWorkspace["latestDecision"]>["status"], string> = {
  selected: "已选择",
  fallback: "已回退",
  failed: "失败",
  unavailable: "不可用",
};

export function RouteExplanationPanel({ workspace }: RouteExplanationPanelProps) {
  const decision = workspace.latestDecision;

  return (
    <SectionCard title="最近一次路由解释" contentClassName="p-0">
      {!decision ? (
        <EmptyState title="暂无决策记录" description="本地入口尚未产生可展示的路由决策。" />
      ) : (
        <div className="grid gap-3 p-4">
          <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
            <div className="flex min-w-0 items-center gap-2">
              <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[10px] bg-slate-100 text-slate-600">
                <GitBranch className="h-4 w-4" />
              </span>
              <div className="min-w-0">
                <div className="truncate text-sm font-semibold text-slate-900">
                  {decision.selectedStationName ?? "未命中站点"}
                </div>
                <div className="truncate text-xs text-muted-foreground">
                  {decision.endpoint} / {decision.model ?? "未指定模型"} / {decision.policy}
                </div>
              </div>
            </div>
            <StatusBadge tone={statusTone[decision.status]}>{statusLabel[decision.status]}</StatusBadge>
          </div>
          <div className="rounded-[var(--surface-radius)] border border-slate-200 bg-slate-50 px-3 py-2 text-xs leading-5 text-slate-600">
            {decision.reason}
          </div>
          <div className="grid gap-2">
            {workspace.recentEvents.slice(0, 3).map((event) => (
              <div key={event.id} className="flex items-start gap-2 text-xs text-slate-600">
                <Activity className="mt-0.5 h-3.5 w-3.5 shrink-0 text-slate-400" />
                <span className="min-w-0 flex-1">{event.message}</span>
                <StatusBadge className="h-5 px-1.5" tone={event.accepted ? "healthy" : "warning"}>
                  {event.accepted ? "通过" : "过滤"}
                </StatusBadge>
              </div>
            ))}
          </div>
        </div>
      )}
    </SectionCard>
  );
}
