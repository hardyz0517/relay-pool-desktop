import { useState } from "react";
import { Button, EmptyState, SectionCard, StatusBadge } from "@/components/ui";
import { readError } from "@/lib/errors";
import { getStationKeyOperationalDetailQuery, simulateRouteQuery } from "@/lib/queries/routingQueries";
import type {
  RecentRouteDecisionsPage,
  RouteSimulationResult,
  RoutingRuntimeOverlay,
  RoutingWorkspaceCandidate,
  RoutingWorkspaceSnapshot,
  StationKeyOperationalDetail,
} from "@/lib/types/routing";

type RoutingOperationalPreviewPanelProps = {
  snapshot: RoutingWorkspaceSnapshot | null;
  runtimeOverlay: RoutingRuntimeOverlay | null;
  decisions: RecentRouteDecisionsPage | null;
  loading: boolean;
};

export function RoutingOperationalPreviewPanel({
  snapshot,
  runtimeOverlay,
  decisions,
  loading,
}: RoutingOperationalPreviewPanelProps) {
  const [detail, setDetail] = useState<StationKeyOperationalDetail | null>(null);
  const [detailLoadingId, setDetailLoadingId] = useState<string | null>(null);
  const [simulation, setSimulation] = useState<RouteSimulationResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [simulating, setSimulating] = useState(false);

  async function openDetail(stationKeyId: string) {
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

  async function runPreviewSimulation() {
    if (!snapshot || simulating) return;
    setSimulating(true);
    setError(null);
    try {
      setSimulation(
        await simulateRouteQuery({
          endpoint: "chat_completions",
          model: "gpt-4o-mini",
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
      <SectionCard title="Routing read-model preview">
        <div className="text-sm text-muted-foreground">正在读取后端 read model...</div>
      </SectionCard>
    );
  }

  if (!snapshot) {
    return (
      <SectionCard title="Routing read-model preview">
        <EmptyState title="暂无路由 read model" description="刷新后仍无数据时，请先检查站点和 Key 池配置。" />
      </SectionCard>
    );
  }

  return (
    <SectionCard
      title="Routing read-model preview"
      description="所有候选事实、价格 basis、capacity 和 capability 摘要都来自后端 read model。"
      action={<StatusBadge tone={snapshot.readModelStatus === "available" ? "healthy" : "warning"}>{snapshot.readModelStatus}</StatusBadge>}
      contentClassName="grid gap-3"
    >
      <div className="grid gap-2 text-sm sm:grid-cols-3">
        <ReadModelMetric label="Preview policy" value={snapshot.previewPolicyVersion} />
        <ReadModelMetric label="Capacity" value={snapshot.capacityMode} />
        <ReadModelMetric label="Runtime revision" value={runtimeOverlay?.revision.toString() ?? "snapshot-only"} />
      </div>

      <div className="overflow-hidden rounded-[var(--surface-radius)] border border-border">
        {snapshot.candidates.length === 0 ? (
          <EmptyState title="暂无候选" description="当前后端 snapshot 没有可展示候选。" />
        ) : (
          <div className="divide-y divide-border">
            {snapshot.candidates.slice(0, 12).map((candidate) => (
              <CandidatePreviewRow
                key={candidate.stationKeyId}
                candidate={candidate}
                loadingDetail={detailLoadingId === candidate.stationKeyId}
                onOpenDetail={() => void openDetail(candidate.stationKeyId)}
              />
            ))}
          </div>
        )}
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2 rounded-[var(--surface-radius)] border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
        <span>
          最近决策：{decisions?.decisions.length ?? 0} 条；
          preview simulator 不获取容量 lease。
        </span>
        <Button size="sm" variant="secondary" disabled={simulating} onClick={() => void runPreviewSimulation()}>
          {simulating ? "模拟中..." : "运行 preview simulation"}
        </Button>
      </div>

      {error && (
        <div className="rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-xs text-danger-foreground">
          {error}
        </div>
      )}

      {simulation && (
        <div className="grid gap-1 rounded-[var(--surface-radius)] border border-info-border bg-info-surface px-3 py-2 text-xs text-info-foreground">
          <span>previewPolicyVersion: {simulation.previewPolicyVersion}</span>
          <span>capacityMode: {simulation.capacityMode}</span>
          <span>selectedCapacityAcquired: {String(simulation.selectedCapacityAcquired)}</span>
          <span>
            cost basis:{" "}
            {Array.from(new Set(simulation.candidates.map(classifySimulationCostBasis))).join(", ") || "unpriced"}
          </span>
        </div>
      )}

      {detail && (
        <div className="grid gap-2 rounded-[var(--surface-radius)] border border-border bg-surface p-3">
          <div className="flex items-center justify-between gap-2">
            <span className="text-sm font-semibold text-foreground">Operational detail</span>
            <StatusBadge tone={detail.readModelStatus === "available" ? "healthy" : "warning"}>{detail.readModelStatus}</StatusBadge>
          </div>
          <div className="grid gap-1 text-xs text-muted-foreground">
            {detail.facts.slice(0, 8).map((fact) => (
              <div key={`${fact.scope}-${fact.name}-${fact.source}`} className="flex justify-between gap-3">
                <span>{fact.scope}.{fact.name}</span>
                <span className="text-right text-foreground">{fact.value}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </SectionCard>
  );
}

function ReadModelMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-0.5 truncate font-medium text-foreground">{value}</div>
    </div>
  );
}

function CandidatePreviewRow({
  candidate,
  loadingDetail,
  onOpenDetail,
}: {
  candidate: RoutingWorkspaceCandidate;
  loadingDetail: boolean;
  onOpenDetail: () => void;
}) {
  return (
    <div className="grid gap-2 px-3 py-2 text-sm sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
      <div className="min-w-0">
        <div className="truncate font-medium text-foreground">
          {candidate.stationName} / {candidate.keyName}
        </div>
        <div className="mt-0.5 flex flex-wrap gap-1.5 text-xs text-muted-foreground">
          <span>price basis: {formatPriceBasis(candidate.priceBasis)}</span>
          <span>health: {candidate.healthState}</span>
          <span>capacity: {candidate.capacity.mode}</span>
          <span>priority: {candidate.priority}</span>
        </div>
      </div>
      <Button size="sm" variant="secondary" disabled={loadingDetail} onClick={onOpenDetail}>
        {loadingDetail ? "读取中..." : "查看 operational detail"}
      </Button>
    </div>
  );
}

function formatPriceBasis(value: string) {
  if (value === "exact") return "exact";
  if (value === "multiplier_proxy" || value === "balance_only") return "multiplier proxy";
  return "unpriced";
}

function classifySimulationCostBasis(candidate: RouteSimulationResult["candidates"][number]) {
  if (
    typeof candidate.estimatedInputPrice === "number" &&
    typeof candidate.estimatedOutputPrice === "number" &&
    candidate.priceConfidence != null
  ) {
    return "exact";
  }
  if (candidate.effectiveMultiplierSource) {
    return "multiplier proxy";
  }
  return "unpriced";
}
