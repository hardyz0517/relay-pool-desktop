import { AlertCircle, Clock3, Filter, Gauge, Power, PowerOff, Route, Search, Server, Upload, UsersRound } from "lucide-react";
import { Button, Dialog, EmptyState, MetricPanel, SectionCard, useToast } from "@/components/ui";
import { useEffect, useState } from "react";
import { Copy } from "lucide-react";
import type { RoutingWorkspaceView } from "@/lib/types/routingWorkspace";
import type { RouteCandidateExplanation, RouteEndpointKind, RouteSimulationResult, RoutingGroupFilter } from "@/lib/types/routing";
import { readError } from "@/lib/errors";
import { getLocalAccessKey } from "@/lib/api/settings";
import { settingsQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { simulateRouteQuery } from "@/lib/queries/routingQueries";
import type { VersionedRoutingDeepLink } from "@/lib/types/routingDeepLinks";
import {
  buildLatestDecisionDisplay,
  formatRoutingDecisionTime,
} from "./localRoutingStatusViewModel";
import {
  LocalRoutingStatusCandidateHeader,
  LocalRoutingStatusCandidateRow,
} from "./LocalRoutingStatusCandidateRow";

type LocalRoutingStatusTabProps = {
  workspace: RoutingWorkspaceView | null;
  maxRateMultiplier?: number | null;
  loading: boolean;
  nowMs: number;
  proxyActionPending: boolean;
  onToggleProxy: () => void;
  importingCCSwitch: boolean;
  onImportToCCSwitch: () => void;
  deepLink?: VersionedRoutingDeepLink | null;
};

const endpointLabels: Record<RouteEndpointKind, string> = {
  chat_completions: "聊天补全",
  responses: "Responses",
  models: "模型列表",
  embeddings: "向量",
};

const routeMetricValueClassName = "text-[20px] leading-6 text-foreground";

export function LocalRoutingStatusTab({
  workspace,
  maxRateMultiplier,
  loading,
  nowMs,
  proxyActionPending,
  onToggleProxy,
  importingCCSwitch,
  onImportToCCSwitch,
  deepLink,
}: LocalRoutingStatusTabProps) {
  const [decisionDetailsOpen, setDecisionDetailsOpen] = useState(false);
  const latestDecisionId = workspace?.latestDecision?.id ?? null;
  const [simulationOpen, setSimulationOpen] = useState(false);
  const [simulationModel, setSimulationModel] = useState("gpt-4o-mini");
  const [simulation, setSimulation] = useState<RouteSimulationResult | null>(null);
  const [simulating, setSimulating] = useState(false);
  const [simulationError, setSimulationError] = useState<string | null>(null);
  useEffect(() => {
    if (deepLink?.kind !== "simulate-model") return;
    setSimulationModel(deepLink.model);
    setSimulationOpen(true);
  }, [deepLink?.sequence]);
  const toast = useToast();
  const settingsQuery = useActivityQuery(settingsQueryOptions());
  const localKeyMasked = settingsQuery.data?.localKeyMasked ?? "未读取";
  async function copyLocalAccessKey() {
    try {
      await navigator.clipboard.writeText(await getLocalAccessKey());
      toast.success("本地访问密钥已复制");
    } catch (error) {
      toast.error("复制失败", readError(error));
    }
  }
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
  const effectiveMaxRateMultiplier =
    maxRateMultiplier === undefined ? workspace.settings.maxRateMultiplier : maxRateMultiplier;
  const multiplierLimitLabel =
    effectiveMaxRateMultiplier == null ? "不限制" : `${effectiveMaxRateMultiplier}x`;
  const multiplierLimitDetail =
    effectiveMaxRateMultiplier == null
      ? "未启用价格倍率硬上限"
      : "高于此倍率不参与自动路由";
  const routingGroupFilterLabel = formatRoutingGroupFilter(workspace.settings.routingGroupFilter);
  const candidateStatusLabel = `${workspace.summary.previewEligibleCandidateCount} / ${workspace.summary.previewExcludedCandidateCount}`;
  const latestDecisionTimeLabel = formatRoutingDecisionTime(latestDecision.decidedAt);
  const candidateHeading =
    workspace.settings.previewKind === "baseline_eligibility" ? "候选基础资格" : "候选资格";

  const selectedSimulationCandidate = simulation?.selectedStationKeyId ? simulation.candidates.find((candidate) => candidate.stationKeyId === simulation.selectedStationKeyId) ?? null : null;
  async function runSimulation() {
    if (simulating) return;
    setSimulating(true); setSimulationError(null);
    try {
      setSimulation(await simulateRouteQuery({ endpoint: workspace!.settings.endpoint, model: simulationModel, stream: true, usesTools: false, usesVision: false, usesReasoning: false, policy: null, maxRateMultiplier: effectiveMaxRateMultiplier, routingGroupFilter: workspace!.settings.routingGroupFilter }));
    } catch (error) { setSimulationError(readError(error)); } finally { setSimulating(false); }
  }

  return (
    <div className="grid gap-4">
      <SectionCard title="本地路由状态">
        <div className="relative flex flex-wrap items-center justify-between gap-3 pr-8">
          <button
            type="button"
            className="absolute -right-1 -top-1 inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-hover hover:text-foreground"
            aria-label="模拟路由"
            title="模拟路由"
            onClick={() => setSimulationOpen(true)}
          >
            <AlertCircle className="h-4 w-4" />
          </button>
          <div className="flex min-w-0 items-center gap-3">
            <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[8px] bg-selected text-primary">
              <Server className="h-5 w-5" />
            </span>
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-1">
                <span className="text-sm font-bold text-foreground">端点：</span>
                <span className="truncate text-sm font-semibold text-foreground">
                  {workspace.settings.bindAddr}:{workspace.settings.port}
                </span>
                <span className="ml-3 text-sm font-bold text-foreground">密钥：</span>
                <code className="rounded bg-surface-subtle px-1.5 py-0.5 text-xs text-info-foreground">{localKeyMasked}</code>
                <button type="button" className="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-hover hover:text-foreground" aria-label="复制本地访问密钥" onClick={() => void copyLocalAccessKey()}>
                  <Copy className="h-3.5 w-3.5" />
                </button>
                <Button
                  disabled={importingCCSwitch}
                  size="sm"
                  variant="secondary"
                  onClick={onImportToCCSwitch}
                >
                  <Upload className="h-3.5 w-3.5" />
                  {importingCCSwitch ? "导入中" : "导入到 CCSwitch"}
                </Button>
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
      </SectionCard>

      <MetricPanel
        title="路由策略概览"
        metrics={[
          {
            label: "倍率上限",
            value: multiplierLimitLabel,
            detail: multiplierLimitDetail,
            icon: Gauge,
            accent: "slate",
            valueClassName: routeMetricValueClassName,
          },
          {
            label: "分组筛选",
            value: routingGroupFilterLabel,
            detail: "当前候选范围",
            icon: Filter,
            accent: "blue",
            valueClassName: routeMetricValueClassName,
          },
          {
            label: "候选状态",
            value: candidateStatusLabel,
            detail: "可参与 / 不参与",
            icon: UsersRound,
            tone: workspace.summary.previewExcludedCandidateCount > 0 ? "warning" : "good",
            valueClassName:
              workspace.summary.previewExcludedCandidateCount > 0
                ? "text-[20px] leading-6 text-warning-foreground"
                : "text-[20px] leading-6 text-success-foreground",
          },
          {
            label: "最近一次路由",
            action: (
              <button type="button" className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-hover" aria-label="查看最近决策原因" title="查看最近决策原因" onClick={() => setDecisionDetailsOpen(true)}>
                <AlertCircle className="h-4 w-4" />
              </button>
            ),
            value: latestDecision.title,
            detail: (
              <span className="inline-flex min-w-0 items-center gap-1.5">
                <Clock3 className="h-3.5 w-3.5 shrink-0" />
                <span className="truncate">{latestDecisionTimeLabel}</span>
              </span>
            ),
            icon: Clock3,
            tone:
              latestDecision.tone === "error"
                ? "danger"
                : latestDecision.tone === "warning"
                  ? "warning"
                  : latestDecision.tone === "healthy"
                    ? "good"
                    : "neutral",
            valueClassName: "text-sm leading-6 text-foreground",
          },
        ]}
      />

      <section aria-labelledby="local-routing-candidates-title">
        <div className="mb-2 flex items-center justify-between gap-3">
          <h2
            id="local-routing-candidates-title"
            className="text-sm font-semibold text-foreground"
          >
            {candidateHeading}
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
          <div className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface">
            <LocalRoutingStatusCandidateHeader />
            <div className="divide-y divide-border">
              {workspace.candidates.map((candidate, index) => (
                <LocalRoutingStatusCandidateRow
                  key={candidate.stationKeyId}
                  candidate={candidate}
                  order={index + 1}
                  nowMs={nowMs}
                />
              ))}
            </div>
          </div>
        )}
      </section>
      <Dialog open={decisionDetailsOpen} title="最近决策原因" description={latestDecisionId ?? "暂无最近决策"} onClose={() => setDecisionDetailsOpen(false)}>
        <div className="grid gap-2 p-4 text-sm">
          <div>状态：{workspace.latestDecision?.status ?? "unavailable"}</div><div>原因：{workspace.latestDecision?.reason || "暂无说明"}</div><div>路由策略：{workspace.latestDecision?.policy ?? "暂无记录"}</div>
        </div>
      </Dialog>
      <Dialog open={simulationOpen} title="模拟路由" description="输入模型名，查看当前规则会选择哪个 密钥" onClose={() => setSimulationOpen(false)}>
        <div className="grid gap-3 p-4">
          <div className="flex gap-2"><div className="relative min-w-0 flex-1"><Search className="pointer-events-none absolute left-2 top-2 h-4 w-4" /><input className="h-8 w-full border border-border pl-8" value={simulationModel} onChange={(e) => setSimulationModel(e.target.value)} /></div><Button size="sm" disabled={simulating} onClick={() => void runSimulation()}><Route className="h-4 w-4" />模拟</Button></div>
          {simulationError ? <div className="text-sm text-danger-foreground">{simulationError}</div> : null}
          {simulation ? <div className="text-sm">{simulation.selectedStationKeyId ? `会选择：${selectedSimulationCandidate?.keyName ?? simulation.selectedStationKeyId}` : (simulation.message || "没有可用候选")}</div> : <EmptyState title="尚未模拟" description="输入模型后运行模拟" />}
        </div>
      </Dialog>
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









