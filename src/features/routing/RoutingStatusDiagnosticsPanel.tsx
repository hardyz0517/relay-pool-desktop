import { useEffect, useMemo, useState } from "react";
import { Activity } from "lucide-react";
import { Button, EmptyState, SectionCard, StatusBadge } from "@/components/ui";
import type {
  RecentRouteDecisionsPage,
  RouteCandidateExplanation,
  RouteEndpointKind,
  RouteSimulationResult,
  RoutingRuntimeOverlay,
  RoutingWorkspaceCandidate,
  RoutingWorkspaceSnapshot,
} from "@/lib/types/routing";
import type { VersionedRoutingDeepLink } from "@/lib/types/routingDeepLinks";

type RoutingStatusDiagnosticsPanelProps = {
  snapshot: RoutingWorkspaceSnapshot | null;
  runtimeOverlay: RoutingRuntimeOverlay | null;
  decisions: RecentRouteDecisionsPage | null;
  loading: boolean;
  deepLink?: VersionedRoutingDeepLink | null;
  onOpenRequestLog?: (requestLogId: string) => void;
};

export function RoutingStatusDiagnosticsPanel({
  snapshot,
  runtimeOverlay,
  decisions,
  loading,
  deepLink,
  onOpenRequestLog,
}: RoutingStatusDiagnosticsPanelProps) {
  const [stationScopeId, setStationScopeId] = useState<string | null>(null);
  const [highlightedStationKeyId, setHighlightedStationKeyId] = useState<string | null>(null);

  const overlayByKey = useMemo(
    () => new Map((runtimeOverlay?.candidates ?? []).map((candidate) => [candidate.stationKeyId, candidate])),
    [runtimeOverlay?.candidates],
  );
  const scopedCandidates = useMemo(() => {
    if (!stationScopeId) return snapshot?.candidates ?? [];
    return snapshot?.candidates.filter((candidate) => candidate.stationId === stationScopeId) ?? [];
  }, [snapshot?.candidates, stationScopeId]);
  const unpricedCount =
    snapshot?.candidates.filter(
      (candidate) =>
        candidate.pricing.basis === "unpriced" &&
        candidate.multiplier.multiplier == null &&
        candidate.pricing.comparisonValue == null,
    ).length ?? 0;
  const availableCount =
    snapshot?.candidates.filter((candidate) => {
      const overlay = overlayByKey.get(candidate.stationKeyId);
      return (overlay?.healthState ?? candidate.healthState) === "available" && candidate.hardRejectionCodes.length === 0;
    }).length ?? 0;
  useEffect(() => {
    if (!deepLink || !snapshot) return;
    if (deepLink.kind === "station") {
      setStationScopeId(deepLink.stationId);
      setHighlightedStationKeyId(null);
    } else if (deepLink.kind === "station-key") {
      const candidate = snapshot.candidates.find((item) => item.stationKeyId === deepLink.stationKeyId);
      setStationScopeId(candidate?.stationId ?? null);
      setHighlightedStationKeyId(deepLink.stationKeyId);
    }
  }, [deepLink?.sequence, snapshot?.generatedAtMs]);

  if (loading && !snapshot) {
    return (
      <SectionCard title="路由诊断">
        <div className="text-sm text-muted-foreground">正在读取路由状态...</div>
      </SectionCard>
    );
  }

  if (!snapshot) {
    return (
      <SectionCard title="路由诊断">
        <EmptyState
          title="暂无路由诊断数据"
          description="刷新后仍无数据时，请先检查站点、Key 池和本地路由配置。"
        />
      </SectionCard>
    );
  }

  return (
    <section className="grid min-w-0 gap-4" aria-labelledby="routing-diagnostics-title">
      <SectionCard
        title="路由诊断"
        description="把候选、价格、实时并发、模拟路由和最近决策合在状态页里看。"
        action={<StatusBadge tone={snapshot.readModelStatus === "available" ? "healthy" : "warning"}>{snapshot.readModelStatus}</StatusBadge>}
        contentClassName="grid min-w-0 gap-3"
      >
        <div className="grid min-w-0 gap-2 text-sm sm:grid-cols-4">
          <ReadableMetric label="可用候选" value={`${availableCount}/${snapshot.candidates.length}`} />
          <ReadableMetric label="价格缺口" value={`${unpricedCount}`} tone={unpricedCount > 0 ? "warning" : "healthy"} />
          <ReadableMetric label="实时并发" value={formatRuntimeCapacity(snapshot.candidates, overlayByKey)} />
          <ReadableMetric label="最近决策" value={`${decisions?.decisions.length ?? 0}`} />
        </div>

        {stationScopeId ? (
          <div className="flex min-w-0 flex-wrap items-center justify-between gap-2 rounded-[var(--surface-radius)] border border-info-border bg-info-surface px-3 py-2 text-xs text-info-foreground">
            <span className="min-w-0 break-words">
              已按来源筛选：{scopedCandidates.length} 个候选
            </span>
            <Button size="sm" variant="ghost" onClick={() => {
              setStationScopeId(null);
              setHighlightedStationKeyId(null);
            }}>
              清除筛选
            </Button>
          </div>
        ) : null}

        {scopedCandidates.length === 0 ? (
          <EmptyState title="没有候选 Key" description="当前配置下没有可参与路由的 Key。" />
        ) : (
          <div className="grid gap-2">
            {scopedCandidates.slice(0, 6).map((candidate) => (
              <CandidateLine
                key={candidate.stationKeyId}
                candidate={candidate}
                overlay={overlayByKey.get(candidate.stationKeyId)}
                highlighted={candidate.stationKeyId === highlightedStationKeyId}
              />
            ))}
            {scopedCandidates.length > 6 ? (
              <div className="text-xs text-muted-foreground">
                还有 {scopedCandidates.length - 6} 个候选，完整排序请到“编辑”页调整。
              </div>
            ) : null}
          </div>
        )}
      </SectionCard>

      {/* <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(20rem,0.8fr)]">
        <SectionCard
          title="模拟路由"
          description="输入模型名，查看当前规则会选择哪个 Key；这里不会真的占用并发。"
          contentClassName="grid min-w-0 gap-3"
        >
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <div className="relative min-w-0 flex-1 basis-[14rem]">
              <Search className="pointer-events-none absolute left-2.5 top-2 h-4 w-4 text-muted-foreground" />
              <input
                className="h-8 w-full rounded-[var(--surface-radius)] border border-border bg-surface pl-8 pr-3 text-sm outline-none transition-colors focus:border-ring focus:ring-2 focus:ring-ring/20"
                value={model}
                onChange={(event) => setModel(event.target.value)}
                placeholder="模型，例如 gpt-4o-mini"
              />
            </div>
            <Button size="sm" variant="secondary" disabled={simulating} onClick={() => void runSimulation()}>
              <Route className="h-4 w-4" />
              {simulating ? "模拟中..." : "模拟"}
            </Button>
          </div>

          {error ? <div className="text-sm text-danger-foreground">{error}</div> : null}
          {simulation ? (
            <SimulationSummary simulation={simulation} selectedCandidate={selectedCandidate} />
          ) : (
            <EmptyState title="尚未模拟" description="输入模型后运行模拟，就能看到会选谁以及主要原因。" />
          )}
        </SectionCard>

      </div> */}
    </section>
  );
}

function ReadableMetric({
  label,
  value,
  tone = "info",
}: {
  label: string;
  value: string;
  tone?: "healthy" | "warning" | "info";
}) {
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 flex items-center gap-2">
        <Activity className="h-4 w-4 text-muted-foreground" />
        <span className={tone === "warning" ? "font-semibold text-warning-foreground" : tone === "healthy" ? "font-semibold text-success-foreground" : "font-semibold text-foreground"}>
          {value}
        </span>
      </div>
    </div>
  );
}

function CandidateLine({
  candidate,
  overlay,
  highlighted = false,
}: {
  candidate: RoutingWorkspaceCandidate;
  overlay?: RoutingRuntimeOverlay["candidates"][number];
  highlighted?: boolean;
}) {
  const healthState = overlay?.healthState ?? candidate.healthState;
  const inFlight = overlay?.inFlight ?? candidate.capacity.inFlight;
  const blocked = candidate.hardRejectionCodes.length > 0;

  return (
    <div className={`grid min-w-0 gap-2 rounded-[var(--surface-radius)] border px-3 py-2 text-sm md:grid-cols-[minmax(0,1.2fr)_minmax(0,0.9fr)_auto] md:items-center ${highlighted ? "border-info-border bg-info-surface" : "border-border bg-surface"}`}>
      <div className="min-w-0">
        <div className="truncate font-medium text-foreground">{candidate.keyName}</div>
        <div className="truncate text-xs text-muted-foreground">{candidate.stationName}</div>
      </div>
      <div className="min-w-0 text-xs text-muted-foreground">
        <div className="truncate">分组：{candidate.group?.displayName ?? "未分组"}</div>
        <div className="truncate">价格：{formatPrice(candidate)}</div>
      </div>
      <div className="flex flex-wrap items-center gap-2 md:justify-end">
        <StatusBadge tone={blocked ? "warning" : healthState === "available" ? "healthy" : "disabled"}>
          {blocked ? "不可参与" : healthState === "unknown" ? "可参与" : healthState}
        </StatusBadge>
        <span className="text-xs text-muted-foreground">
          本地在途 {inFlight ?? 0}/{formatConcurrencyLimit(candidate.capacity.maxConcurrency)}
        </span>
      </div>
    </div>
  );
}

function SimulationSummary({
  simulation,
  selectedCandidate,
}: {
  simulation: RouteSimulationResult;
  selectedCandidate: RouteCandidateExplanation | null;
}) {
  if (!simulation.selectedStationKeyId) {
    return (
      <div className="rounded-[var(--surface-radius)] border border-warning-border bg-warning-surface px-3 py-2 text-sm text-warning-foreground">
        没有可用候选：{simulation.message || simulation.plannerErrorCode || "当前规则未选出 Key"}
      </div>
    );
  }

  return (
    <div className="grid gap-2 rounded-[var(--surface-radius)] border border-success-border bg-success-surface px-3 py-2 text-sm">
      <div className="font-medium text-success-foreground">
        会选择：{selectedCandidate?.keyName ?? simulation.selectedStationKeyId}
      </div>
      <div className="text-xs text-success-foreground/90">
        {selectedCandidate?.stationName ? `${selectedCandidate.stationName} · ` : null}
        上游模型：{simulation.mappedModel ?? selectedCandidate?.mappedModel ?? "保持原模型"}
      </div>
      {(selectedCandidate?.reasons.length ?? 0) > 0 ? (
        <div className="text-xs text-success-foreground/90">
          原因：{selectedCandidate?.reasons.slice(0, 3).join("、")}
        </div>
      ) : null}
    </div>
  );
}

function formatRuntimeCapacity(
  candidates: RoutingWorkspaceCandidate[],
  overlayByKey: Map<string, RoutingRuntimeOverlay["candidates"][number]>,
) {
  const totals = candidates.reduce(
    (acc, candidate) => {
      const overlay = overlayByKey.get(candidate.stationKeyId);
      acc.inFlight += overlay?.inFlight ?? candidate.capacity.inFlight ?? 0;
      if (candidate.capacity.maxConcurrency <= 0) {
        acc.unlimited = true;
      } else if (!acc.countedStations.has(candidate.stationId)) {
        acc.max += candidate.capacity.maxConcurrency;
        acc.countedStations.add(candidate.stationId);
      }
      return acc;
    },
    { inFlight: 0, max: 0, unlimited: false, countedStations: new Set<string>() },
  );
  return `${totals.inFlight}/${totals.unlimited ? "∞" : totals.max}`;
}

function formatConcurrencyLimit(limit: number) {
  return limit > 0 ? String(limit) : "∞";
}

function formatPrice(candidate: RoutingWorkspaceCandidate) {
  if (candidate.multiplier.multiplier != null) return `${candidate.multiplier.multiplier}x`;
  if (candidate.pricing.basis === "unpriced") {
    return candidate.pricing.reason === "pricing_context_missing" ? "需指定模型" : "缺失";
  }
  if (candidate.pricing.comparisonValue != null) return `${candidate.pricing.comparisonValue}`;
  return candidate.pricing.statusLabel;
}

