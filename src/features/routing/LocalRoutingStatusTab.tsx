import { Clock3, Power, PowerOff, Server } from "lucide-react";
import { Button, EmptyState, SectionCard, StatusBadge } from "@/components/ui";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";
import type { RouteEndpointKind, RoutingGroupFilter } from "@/lib/types/routing";
import {
  buildLatestDecisionDisplay,
  formatRoutingDecisionTime,
} from "./localRoutingStatusViewModel";
import { LocalRoutingStatusCandidateRow } from "./LocalRoutingStatusCandidateRow";

type LocalRoutingStatusTabProps = {
  workspace: LocalRoutingWorkspace | null;
  loading: boolean;
  nowMs: number;
  proxyActionPending: boolean;
  onToggleProxy: () => void;
};

const endpointLabels: Record<RouteEndpointKind, string> = {
  chat_completions: "聊天补全",
  responses: "Responses",
  models: "模型列表",
  embeddings: "向量",
};

const latestDecisionToneClass = {
  neutral: "text-slate-700",
  healthy: "text-emerald-700",
  warning: "text-amber-700",
  error: "text-rose-700",
};

export function LocalRoutingStatusTab({
  workspace,
  loading,
  nowMs,
  proxyActionPending,
  onToggleProxy,
}: LocalRoutingStatusTabProps) {
  if (loading && !workspace) {
    return (
      <SectionCard title="本地路由状态">
        <div className="text-sm text-muted-foreground">正在加载本地路由状态...</div>
      </SectionCard>
    );
  }

  if (!workspace) {
    return (
      <SectionCard title="本地路由状态">
        <EmptyState
          title="暂无本地路由数据"
          description="刷新后仍无数据，请检查本地路由配置。"
        />
      </SectionCard>
    );
  }

  const latestDecision = buildLatestDecisionDisplay(
    workspace.proxyStatus.running,
    workspace.latestDecision,
  );
  const multiplierLimitLabel =
    workspace.settings.maxRateMultiplier == null
      ? "未设置"
      : `${workspace.settings.maxRateMultiplier}x`;

  return (
    <div className="grid gap-4">
      <SectionCard title="本地路由状态">
        <div className="grid gap-3">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex min-w-0 items-center gap-3">
              <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[8px] bg-teal-50 text-teal-700">
                <Server className="h-5 w-5" />
              </span>
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="truncate text-sm font-semibold text-slate-900">
                    {workspace.settings.bindAddr}:{workspace.settings.port}
                  </span>
                  <StatusBadge tone={workspace.proxyStatus.running ? "healthy" : "disabled"}>
                    {workspace.proxyStatus.running ? "运行中" : "未启动"}
                  </StatusBadge>
                </div>
                <div className="mt-0.5 truncate text-xs text-muted-foreground">
                  {formatEndpoint(workspace.settings.endpoint)} · 自动路由
                </div>
              </div>
            </div>
            <Button
              disabled={proxyActionPending}
              variant={workspace.proxyStatus.running ? "danger" : "primary"}
              onClick={onToggleProxy}
            >
              {workspace.proxyStatus.running ? (
                <PowerOff className="h-4 w-4" />
              ) : (
                <Power className="h-4 w-4" />
              )}
              {workspace.proxyStatus.running ? "停止路由" : "启动路由"}
            </Button>
          </div>

          <dl className="grid gap-x-6 gap-y-3 border-t border-slate-100 pt-3 sm:grid-cols-4">
            <StatusMetric label="倍率上限" value={multiplierLimitLabel} />
            <StatusMetric
              label="分组筛选"
              value={formatRoutingGroupFilter(workspace.settings.routingGroupFilter)}
            />
            <StatusMetric
              label="可参与"
              value={workspace.summary.previewEligibleCandidateCount}
              tone="good"
            />
            <StatusMetric
              label="不参与"
              value={workspace.summary.previewExcludedCandidateCount}
              tone={workspace.summary.previewExcludedCandidateCount > 0 ? "warning" : "neutral"}
            />
          </dl>

          <div className="flex flex-wrap items-center justify-between gap-3 border-t border-slate-100 pt-3">
            <div className="min-w-0">
              <div className="text-[11px] font-medium text-muted-foreground">最近一次路由</div>
              <div className="mt-0.5 truncate text-sm font-semibold text-slate-900">
                {latestDecision.title}
              </div>
              <div className="mt-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                <Clock3 className="h-3.5 w-3.5" />
                {formatRoutingDecisionTime(latestDecision.decidedAt)}
              </div>
            </div>
            {latestDecision.badge ? (
              <span
                className={`text-xs font-semibold ${latestDecisionToneClass[latestDecision.tone]}`}
              >
                {latestDecision.badge}
              </span>
            ) : null}
          </div>
        </div>
      </SectionCard>

      <section aria-labelledby="local-routing-candidates-title">
        <div className="mb-2 flex items-center justify-between gap-3">
          <h2
            id="local-routing-candidates-title"
            className="text-sm font-semibold text-slate-900"
          >
            候选顺序预览
          </h2>
          <span className="text-xs text-muted-foreground">
            {workspace.summary.candidateCount} 个密钥
          </span>
        </div>
        {workspace.candidates.length === 0 ? (
          <EmptyState
            title="暂无候选密钥"
            description="当前配置下没有可预览的路由密钥。"
          />
        ) : (
          <div className="overflow-hidden rounded-[var(--surface-radius)] border border-slate-200 bg-white divide-y divide-slate-100">
            {workspace.candidates.map((candidate, index) => (
              <LocalRoutingStatusCandidateRow
                key={candidate.stationKeyId}
                candidate={candidate}
                order={index + 1}
                nowMs={nowMs}
              />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function formatEndpoint(endpoint: RouteEndpointKind) {
  return endpointLabels[endpoint] ?? endpoint;
}

function formatRoutingGroupFilter(filter: RoutingGroupFilter) {
  if (filter === "all_groups") return "全部分组";
  if (filter === "ungrouped_only") return "未绑定分组";
  if ("group_type" in filter) return `${filter.group_type} 分组`;
  if ("group_binding_id" in filter) return "指定绑定";
  if ("group_id_hash" in filter) return "指定分组";
  return "全部分组";
}

function StatusMetric({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: number | string;
  tone?: "neutral" | "good" | "warning";
}) {
  const valueClass =
    tone === "good" ? "text-emerald-700" : tone === "warning" ? "text-amber-700" : "text-slate-900";
  return (
    <div>
      <dt className="text-[11px] text-muted-foreground">{label}</dt>
      <dd className={`mt-0.5 text-sm font-semibold ${valueClass}`}>{value}</dd>
    </div>
  );
}
