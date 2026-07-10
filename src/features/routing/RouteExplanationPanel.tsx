import { Activity, GitBranch } from "lucide-react";
import { EmptyState, SectionCard, StatusBadge } from "@/components/ui";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";

type RouteExplanationPanelProps = {
  workspace: LocalRoutingWorkspace;
};

type RouteDecisionForExplanation = NonNullable<LocalRoutingWorkspace["latestDecision"]> & {
  selectedReason?: string | null;
  selectedKeyName?: string | null;
  keptForStability?: boolean | null;
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
  const decision = workspace.latestDecision as RouteDecisionForExplanation | null;

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
            {redactExplanationText(decision.selectedReason ?? decision.reason)}
          </div>
          <div className="grid gap-2">
            <div className="text-xs font-semibold text-slate-700">决策字段</div>
            <div className="grid gap-2 sm:grid-cols-2">
              <FieldRow label="接口" value={decision.endpoint} />
              <FieldRow label="模型" value={decision.model ?? "未指定"} />
              <FieldRow label="策略" value={decision.policy} />
              <FieldRow label="选择站点" value={decision.selectedStationName ?? "未命中"} />
              <FieldRow label="选择 Key" value={decision.selectedKeyName ?? decision.selectedStationKeyId ?? "未命中"} />
              <FieldRow label="稳定保持" value={formatKeptForStability(decision.keptForStability)} />
              <FieldRow label="回退次数" value={decision.fallbackCount.toString()} />
              <FieldRow label="决策时间" value={decision.decidedAt} />
            </div>
          </div>
          <div className="grid gap-2">
            {workspace.recentEvents.slice(0, 3).map((event) => (
              <div key={event.id} className="flex items-start gap-2 text-xs text-slate-600">
                <Activity className="mt-0.5 h-3.5 w-3.5 shrink-0 text-slate-400" />
                <span className="min-w-0 flex-1">{redactExplanationText(event.message)}</span>
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

function FieldRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-[var(--surface-radius)] border border-slate-200 bg-white px-3 py-2">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="mt-0.5 truncate text-xs font-medium text-slate-800">{redactExplanationText(value)}</div>
    </div>
  );
}

function formatKeptForStability(value: boolean | null | undefined) {
  if (value == null) {
    return "未提供";
  }
  return value ? "是" : "否";
}

function redactExplanationText(value: string) {
  return value
    .replace(
      /((?:authorization|x-api-key|api[_-]?key|token|cookie)\s*[:=]\s*)(?:bearer\s+|basic\s+)?[^\s;,]+/gi,
      "$1[redacted]",
    )
    .replace(/(?:bearer|basic)\s+[^\s;,]+/gi, "[redacted]");
}
