import { useEffect, useMemo, useState } from "react";
import { Activity, FileText, GitBranch, Route, Search } from "lucide-react";
import {
  Button,
  DataTableLite,
  EmptyState,
  SectionCard,
  StatusBadge,
  type DataTableColumn,
} from "@/components/ui";
import { readError } from "@/lib/errors";
import {
  getRequestDecisionTraceQuery,
  getStationKeyOperationalDetailQuery,
  simulateRouteQuery,
} from "@/lib/queries/routingQueries";
import type {
  RecentRouteDecisionsPage,
  RequestDecisionTrace,
  RouteSimulationResult,
  RoutingRuntimeOverlay,
  RoutingWorkspaceCandidate,
  RoutingWorkspaceSnapshot,
  RouteEndpointKind,
  StationKeyOperationalDetail,
} from "@/lib/types/routing";
import type { VersionedRoutingDeepLink } from "@/lib/types/routingDeepLinks";

type RoutingOperationalPreviewPanelProps = {
  snapshot: RoutingWorkspaceSnapshot | null;
  runtimeOverlay: RoutingRuntimeOverlay | null;
  decisions: RecentRouteDecisionsPage | null;
  loading: boolean;
  deepLink?: VersionedRoutingDeepLink | null;
  onOpenRequestLog?: (requestLogId: string) => void;
};

export function RoutingOperationalPreviewPanel({
  snapshot,
  runtimeOverlay,
  decisions,
  loading,
  deepLink,
  onOpenRequestLog,
}: RoutingOperationalPreviewPanelProps) {
  const [selectedStationKeyId, setSelectedStationKeyId] = useState<string | null>(null);
  const [stationScopeId, setStationScopeId] = useState<string | null>(null);
  const [selectedRequestLogId, setSelectedRequestLogId] = useState<string | null>(null);
  const [detail, setDetail] = useState<StationKeyOperationalDetail | null>(null);
  const [trace, setTrace] = useState<RequestDecisionTrace | null>(null);
  const [detailLoadingId, setDetailLoadingId] = useState<string | null>(null);
  const [traceLoadingId, setTraceLoadingId] = useState<string | null>(null);
  const [simulation, setSimulation] = useState<RouteSimulationResult | null>(null);
  const [simulationModel, setSimulationModel] = useState("gpt-4o-mini");
  const [error, setError] = useState<string | null>(null);
  const [simulating, setSimulating] = useState(false);

  const overlayByKey = useMemo(
    () => new Map((runtimeOverlay?.candidates ?? []).map((candidate) => [candidate.stationKeyId, candidate])),
    [runtimeOverlay?.candidates],
  );
  const scopedCandidates = useMemo(
    () => stationScopeId
      ? snapshot?.candidates.filter((candidate) => candidate.stationId === stationScopeId) ?? []
      : snapshot?.candidates ?? [],
    [snapshot?.candidates, stationScopeId],
  );
  const stationScopeName = useMemo(
    () => snapshot?.candidates.find((candidate) => candidate.stationId === stationScopeId)?.stationName ?? stationScopeId,
    [snapshot?.candidates, stationScopeId],
  );

  useEffect(() => {
    if (!deepLink) return;
    if (deepLink.kind === "station") {
      openStationScope(deepLink.stationId);
    } else if (deepLink.kind === "station-key") {
      void openDetail(deepLink.stationKeyId);
    } else if (deepLink.kind === "request") {
      void openTrace(deepLink.requestLogId);
    } else if (deepLink.kind === "simulate-model") {
      if (!snapshot) return;
      setSimulationModel(deepLink.model);
      void runPreviewSimulation(deepLink.model, deepLink.endpoint ?? "chat_completions");
    }
  }, [deepLink?.sequence, snapshot?.generatedAtMs]);

  const columns = useMemo<DataTableColumn<RoutingWorkspaceCandidate>[]>(
    () => [
      {
        key: "candidate",
        header: "Key / Station",
        className: "w-[16rem] max-w-[16rem]",
        render: (candidate) => (
          <div className="grid min-w-0 gap-0.5">
            <button
              type="button"
              className="min-w-0 truncate text-left font-medium text-foreground hover:text-primary"
              onClick={(event) => {
                event.stopPropagation();
                void openDetail(candidate.stationKeyId);
              }}
            >
              {candidate.keyName}
            </button>
            <span className="truncate text-xs text-muted-foreground">{candidate.stationName}</span>
          </div>
        ),
      },
      {
        key: "group",
        header: "Group",
        render: () => <StatusBadge tone="disabled">backend detail</StatusBadge>,
      },
      {
        key: "price",
        header: "价格/倍率",
        render: (candidate) => (
          <div className="grid min-w-0 gap-0.5">
            <span>{formatPriceBasis(candidate.priceBasis)}</span>
            <span className="text-xs text-muted-foreground">{candidate.balanceStatus ?? "balance unknown"}</span>
          </div>
        ),
      },
      {
        key: "capability",
        header: "能力证据",
        className: "w-[18rem] max-w-[18rem]",
        render: (candidate) => (
          <div className="flex max-w-[18rem] flex-wrap gap-1">
            {capabilityLabels(candidate).map((label) => (
              <StatusBadge key={label} className="h-5 px-1.5" tone="info">
                {label}
              </StatusBadge>
            ))}
          </div>
        ),
      },
      {
        key: "health",
        header: "Key / Endpoint",
        render: (candidate) => {
          const overlay = overlayByKey.get(candidate.stationKeyId);
          return (
            <div className="grid min-w-0 gap-0.5">
              <StatusBadge tone={healthTone(overlay?.healthState ?? candidate.healthState)}>
                {overlay?.healthState ?? candidate.healthState}
              </StatusBadge>
              <span className="text-xs text-muted-foreground">endpoint rev {candidate.endpointRevision}</span>
            </div>
          );
        },
      },
      {
        key: "capacity",
        header: "In-flight / Max",
        render: (candidate) => {
          const overlay = overlayByKey.get(candidate.stationKeyId);
          return `${overlay?.inFlight ?? candidate.capacity.inFlight ?? "?"}/${candidate.capacity.maxConcurrency}`;
        },
      },
      {
        key: "lastDispatch",
        header: "最近调度",
        render: (candidate) => (
          <span className="text-xs text-muted-foreground">
            {overlayByKey.has(candidate.stationKeyId) ? `runtime rev ${runtimeOverlay?.revision ?? 0}` : "snapshot only"}
          </span>
        ),
      },
    ],
    [overlayByKey, runtimeOverlay?.revision],
  );

  async function openDetail(stationKeyId: string) {
    setSelectedStationKeyId(stationKeyId);
    setDetailLoadingId(stationKeyId);
    setError(null);
    try {
      setDetail(await getStationKeyOperationalDetailQuery(stationKeyId));
    } catch (detailError) {
      setError(readError(detailError));
    } finally {
      setDetailLoadingId(null);
    }
  }

  function openStationScope(stationId: string) {
    setStationScopeId(stationId);
    setSelectedStationKeyId(null);
    setDetail(null);
    setError(null);
  }

  async function openTrace(requestLogId: string) {
    setSelectedRequestLogId(requestLogId);
    setTraceLoadingId(requestLogId);
    setError(null);
    try {
      setTrace(await getRequestDecisionTraceQuery(requestLogId));
    } catch (traceError) {
      setError(readError(traceError));
    } finally {
      setTraceLoadingId(null);
    }
  }

  async function runPreviewSimulation(model = simulationModel, endpoint: RouteEndpointKind = "chat_completions") {
    if (!snapshot || simulating) return;
    setSimulating(true);
    setError(null);
    try {
      setSimulation(
        await simulateRouteQuery({
          endpoint,
          model,
          stream: true,
          usesTools: false,
          usesVision: false,
          usesReasoning: false,
          policy: snapshot.productionPolicy,
          maxRateMultiplier: snapshot.maxRateMultiplier,
          routingGroupFilter: snapshot.routingGroupFilter,
        }),
      );
    } catch (simulationError) {
      setError(readError(simulationError));
    } finally {
      setSimulating(false);
    }
  }

  if (loading && !snapshot) {
    return (
      <SectionCard title="综合路由工作台">
        <div className="text-sm text-muted-foreground">正在读取后端 routing read model...</div>
      </SectionCard>
    );
  }

  if (!snapshot) {
    return (
      <SectionCard title="综合路由工作台">
        <EmptyState title="暂无路由 read model" description="刷新后仍无数据时，请先检查站点和 Key 池配置。" />
      </SectionCard>
    );
  }

  return (
    <div className="grid min-w-0 gap-4">
      <SectionCard
        title="综合路由工作台"
        description="候选、运行时 overlay、最近决策、detail 和 simulation 使用独立后端 read model。"
        action={<StatusBadge tone={snapshot.readModelStatus === "available" ? "healthy" : "warning"}>{snapshot.readModelStatus}</StatusBadge>}
        contentClassName="grid min-w-0 gap-3"
      >
        <div className="grid min-w-0 gap-2 text-sm sm:grid-cols-5">
          <ReadModelMetric label="候选" value={stationScopeId ? `${scopedCandidates.length}/${snapshot.candidates.length}` : `${snapshot.page.returned}/${snapshot.page.limit}`} />
          <ReadModelMetric label="Preview policy" value={snapshot.previewPolicyVersion} />
          <ReadModelMetric label="Capacity" value={snapshot.capacityMode} />
          <ReadModelMetric label="Runtime rev" value={runtimeOverlay?.revision.toString() ?? "snapshot-only"} />
          <ReadModelMetric label="价格缺口" value={`${snapshot.candidates.filter((candidate) => candidate.priceBasis === "unpriced").length}`} />
        </div>

        {stationScopeId ? (
          <div className="flex min-w-0 flex-wrap items-center justify-between gap-2 rounded-[var(--surface-radius)] border border-info-border bg-info-surface px-3 py-2 text-xs text-info-foreground">
            <span className="min-w-0 break-words">
              Station scope: {stationScopeName ?? stationScopeId} · {scopedCandidates.length} candidates from backend snapshot
            </span>
            <Button size="sm" variant="ghost" onClick={() => setStationScopeId(null)}>
              清除过滤
            </Button>
          </div>
        ) : null}

        {scopedCandidates.length === 0 ? (
          <EmptyState title="暂无候选" description="当前后端 snapshot 没有可展示候选。" />
        ) : (
          <DataTableLite
            columns={columns}
            rows={scopedCandidates}
            getRowKey={(candidate) => candidate.stationKeyId}
            selectedKey={selectedStationKeyId ?? undefined}
            onRowClick={(candidate) => void openDetail(candidate.stationKeyId)}
            className="max-h-[420px] min-w-0 [&_table]:min-w-[980px]"
          />
        )}
      </SectionCard>

      <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(23rem,0.7fr)]">
        <SectionCard
          title="路由模拟器"
          description="调用同一后端 planner；capacity 明确为 snapshot-only，不获取真实 lease。"
          action={<StatusBadge tone="info">snapshot only</StatusBadge>}
          contentClassName="grid min-w-0 gap-3"
        >
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <div className="relative min-w-0 flex-1 basis-[14rem]">
              <Search className="pointer-events-none absolute left-2.5 top-2 h-4 w-4 text-muted-foreground" />
              <input
                className="h-8 w-full rounded-[var(--surface-radius)] border border-border bg-surface pl-8 pr-3 text-sm outline-none transition-colors focus:border-ring focus:ring-2 focus:ring-ring/20"
                value={simulationModel}
                onChange={(event) => setSimulationModel(event.target.value)}
                placeholder="模型，例如 gpt-4o-mini"
              />
            </div>
            <Button size="sm" variant="secondary" disabled={simulating} onClick={() => void runPreviewSimulation()}>
              <Route className="h-4 w-4" />
              {simulating ? "模拟中..." : "模拟此模型"}
            </Button>
          </div>
          {simulation ? <SimulationResult simulation={simulation} /> : <EmptyState title="尚未模拟" description="输入模型后运行 simulation，查看后端 planner 的候选解释。" />}
        </SectionCard>

        <SectionCard
          title="Operational detail"
          description={selectedStationKeyId ? `Station Key: ${selectedStationKeyId}` : "从候选表或 Key 池 deep link 打开"}
          action={detailLoadingId ? <StatusBadge tone="info">读取中</StatusBadge> : null}
          contentClassName="grid min-w-0 gap-2"
        >
          {detail ? <OperationalDetailPanel detail={detail} /> : <EmptyState title="暂无详情" description="选择一个候选查看事实来源、revision、新鲜度和影响。" />}
        </SectionCard>
      </div>

      <SectionCard
        title="最近决策与 decision timeline"
        description="timeline 区分 planning round、fallback、downstream delivery 和 cost aggregate；旧日志显示 legacy summary。"
        action={<StatusBadge tone={decisions?.readModelStatus === "available" ? "healthy" : "warning"}>{decisions?.readModelStatus ?? "unavailable"}</StatusBadge>}
        contentClassName="grid min-w-0 gap-3"
      >
        <div className="grid min-w-0 gap-2 lg:grid-cols-[minmax(0,1fr)_minmax(22rem,0.8fr)]">
          <div className="overflow-hidden rounded-[var(--surface-radius)] border border-border">
            {(decisions?.decisions.length ?? 0) === 0 ? (
              <EmptyState title="暂无最近决策" description="有请求经过本地代理后，这里会显示 request scope。" />
            ) : (
              <div className="divide-y divide-border">
                {decisions?.decisions.map((decision) => (
                  <button
                    key={decision.requestLogId}
                    type="button"
                    className="grid w-full min-w-0 gap-1 px-3 py-2 text-left text-sm hover:bg-hover"
                    onClick={() => void openTrace(decision.requestLogId)}
                  >
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                      <span className="truncate font-medium text-foreground">{decision.model ?? decision.endpoint}</span>
                      <StatusBadge tone={decision.status === "success" ? "healthy" : "warning"}>{decision.status}</StatusBadge>
                      {traceLoadingId === decision.requestLogId ? <StatusBadge tone="info">读取中</StatusBadge> : null}
                    </div>
                    <div className="text-xs text-muted-foreground">
                      fallback {decision.fallbackCount} · cost {decision.costStatus ?? "unknown"} · {decision.costCurrency ?? "currency unknown"}
                    </div>
                  </button>
                ))}
              </div>
            )}
          </div>
          {trace ? (
            <DecisionTracePanel
              trace={trace}
              selectedRequestLogId={selectedRequestLogId}
              onOpenRequestLog={onOpenRequestLog}
            />
          ) : (
            <EmptyState title="暂无 timeline" description="选择一条最近决策，或从使用记录 deep link 进入。" />
          )}
        </div>
      </SectionCard>

      {error && (
        <div className="break-words rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-xs text-danger-foreground">
          {error}
        </div>
      )}
    </div>
  );
}

function ReadModelMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-0.5 truncate font-medium text-foreground">{value}</div>
    </div>
  );
}

function OperationalDetailPanel({ detail }: { detail: StationKeyOperationalDetail }) {
  return (
    <div className="grid min-w-0 gap-2">
      <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
        <span className="min-w-0 break-words text-sm font-semibold text-foreground">
          endpoint rev {detail.endpointRevision}
        </span>
        <StatusBadge tone={detail.readModelStatus === "available" ? "healthy" : "warning"}>{detail.readModelStatus}</StatusBadge>
      </div>
      <div className="grid gap-1 text-xs text-muted-foreground">
        {detail.facts.map((fact) => (
          <div key={`${fact.scope}-${fact.name}-${fact.source}`} className="grid min-w-0 gap-1 rounded-lg border border-border bg-surface-subtle px-2 py-1.5">
            <div className="flex min-w-0 flex-wrap justify-between gap-3">
              <span className="min-w-0 break-words">{fact.scope}.{fact.name}</span>
              <span className="min-w-0 break-words text-right text-foreground">{fact.value}</span>
            </div>
            <div className="break-words">
              source {fact.source} · freshness {fact.freshness}
              {fact.reason ? ` · ${fact.reason}` : ""}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function SimulationResult({ simulation }: { simulation: RouteSimulationResult }) {
  return (
    <div className="grid min-w-0 gap-2 rounded-[var(--surface-radius)] border border-info-border bg-info-surface px-3 py-2 text-xs text-info-foreground">
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <StatusBadge tone={simulation.selectedCapacityAcquired ? "warning" : "info"}>
          {simulation.capacityMode}
        </StatusBadge>
        <span className="min-w-0 break-words">policy {simulation.policy}</span>
        <span className="min-w-0 break-words">selected {simulation.selectedStationKeyId ?? "none"}</span>
      </div>
      <div className="break-words">{simulation.message}</div>
      <div>
        candidates {simulation.candidates.length} · accepted{" "}
        {simulation.candidates.filter((candidate) => candidate.accepted).length} · rejected{" "}
        {simulation.candidates.filter((candidate) => !candidate.accepted).length}
      </div>
    </div>
  );
}

function DecisionTracePanel({
  trace,
  selectedRequestLogId,
  onOpenRequestLog,
}: {
  trace: RequestDecisionTrace;
  selectedRequestLogId: string | null;
  onOpenRequestLog?: (requestLogId: string) => void;
}) {
  const rows =
    trace.timeline.length > 0
      ? trace.timeline
      : [
          {
            ordinal: 1,
            kind: "unavailable" as const,
            status: "unavailable" as const,
            title: trace.status === "legacy_summary" ? "Legacy summary" : "Trace unavailable",
            summary: trace.legacySummary
              ? `policy ${trace.legacySummary.routePolicy ?? "unknown"} · fallback ${trace.legacySummary.fallbackCount} · selected ${trace.legacySummary.stationKeyId ?? "none"}`
              : trace.reason,
            detailCode: trace.reason,
            routePolicy: trace.legacySummary?.routePolicy ?? null,
            routeReason: trace.legacySummary?.routeReason ?? null,
            stationKeyId: trace.legacySummary?.stationKeyId ?? null,
            stationId: trace.legacySummary?.stationId ?? null,
            attemptCount: null,
            fallbackCount: trace.legacySummary?.fallbackCount ?? null,
            durationMs: null,
            costStatus: null,
            estimatedTotalCost: null,
            costCurrency: null,
          },
        ];

  return (
    <div className="grid min-w-0 gap-2 overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold text-foreground">
            {selectedRequestLogId ?? trace.requestLogId}
          </div>
          <div className="text-xs text-muted-foreground">{trace.traceVersion}</div>
        </div>
        <div className="flex min-w-0 flex-wrap items-center justify-end gap-2">
          {onOpenRequestLog ? (
            <Button
              size="sm"
              variant="secondary"
              onClick={() => onOpenRequestLog(trace.requestLogId)}
            >
              <FileText className="h-4 w-4" />
              查看使用记录
            </Button>
          ) : null}
          <StatusBadge tone={trace.status === "legacy_summary" ? "info" : "warning"}>{trace.status}</StatusBadge>
        </div>
      </div>
      <div className="grid min-w-0 gap-2">
        {rows.map((row) => (
          <div
            key={`${row.ordinal}-${row.kind}`}
            className="grid min-w-0 gap-1 rounded-lg border border-border bg-surface-subtle px-2 py-1.5 text-xs"
          >
            <div className="flex min-w-0 flex-wrap items-center gap-1.5 font-medium text-foreground">
              {row.kind === "planning_round" || row.kind === "fallback" ? (
                <GitBranch className="h-3.5 w-3.5 text-muted-foreground" />
              ) : (
                <Activity className="h-3.5 w-3.5 text-muted-foreground" />
              )}
              <span className="min-w-0 break-words">{row.title}</span>
              <StatusBadge tone={timelineStatusTone(row.status)} className="h-5 px-1.5">
                {formatTimelineStatus(row.status)}
              </StatusBadge>
            </div>
            <div className="break-words text-muted-foreground">{row.summary}</div>
            <div className="flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
              <span>{formatTimelineKind(row.kind)}</span>
              <span className="break-all">{row.detailCode}</span>
              {row.durationMs != null ? <span>{row.durationMs} ms</span> : null}
              {row.attemptCount != null ? <span>{row.attemptCount} attempt(s)</span> : null}
              {row.fallbackCount != null ? <span>{row.fallbackCount} fallback(s)</span> : null}
              {row.stationKeyId ? <span className="break-all">key {row.stationKeyId}</span> : null}
              {row.costStatus ? (
                <span>
                  cost {row.costStatus}
                  {row.estimatedTotalCost != null
                    ? ` ${row.estimatedTotalCost.toFixed(6)} ${row.costCurrency ?? ""}`.trimEnd()
                    : ""}
                </span>
              ) : null}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function timelineStatusTone(status: RequestDecisionTrace["timeline"][number]["status"]) {
  if (status === "available") return "healthy";
  if (status === "legacy_summary") return "info";
  if (status === "skipped") return "disabled";
  return "warning";
}

function formatTimelineStatus(status: RequestDecisionTrace["timeline"][number]["status"]) {
  if (status === "legacy_summary") return "legacy";
  return status.replace(/_/g, " ");
}

function formatTimelineKind(kind: RequestDecisionTrace["timeline"][number]["kind"]) {
  return kind.replace(/_/g, " ");
}

function capabilityLabels(candidate: RoutingWorkspaceCandidate) {
  const labels: string[] = [];
  if (candidate.capabilitySummary.chatCompletions) labels.push("chat");
  if (candidate.capabilitySummary.responses) labels.push("responses");
  if (candidate.capabilitySummary.embeddings) labels.push("embeddings");
  if (candidate.capabilitySummary.stream) labels.push("stream");
  if (candidate.capabilitySummary.tools) labels.push("tools");
  if (candidate.capabilitySummary.vision) labels.push("vision");
  if (candidate.capabilitySummary.reasoning) labels.push("reasoning");
  return labels.length > 0 ? labels : ["capability unknown"];
}

function healthTone(value: string) {
  if (value === "ready") return "healthy";
  if (value === "cooldown" || value === "degraded") return "warning";
  return "disabled";
}

function formatPriceBasis(value: string) {
  if (value === "exact") return "exact";
  if (value === "multiplier_proxy" || value === "balance_only") return "multiplier proxy";
  if (value === "unpriced") return "unpriced";
  return value;
}
