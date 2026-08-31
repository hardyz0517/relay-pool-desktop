import { useEffect, useMemo, useState } from "react";
import { Activity } from "lucide-react";
import { Button, EmptyState, SectionCard, StatusBadge } from "@/components/ui";
import type {
  RecentRouteDecisionsPage,
  RoutingRuntimeOverlay,
  RoutingProtectionStatus,
  RoutingWorkspaceCandidate,
  RoutingWorkspaceSnapshot,
} from "@/lib/types/routing";
import type { VersionedRoutingDeepLink } from "@/lib/types/routingDeepLinks";
import { userVisibleProtectionEntries } from "./routingProtectionPresentation";
import { buildRoutingCandidateDiagnosticsDisplay } from "./routingDiagnosticsPresentation";
import { buildParticipationDisplay } from "./localRoutingStatusViewModel";

type RoutingStatusDiagnosticsPanelProps = {
  snapshot: RoutingWorkspaceSnapshot | null;
  runtimeOverlay: RoutingRuntimeOverlay | null;
  decisions: RecentRouteDecisionsPage | null;
  protectionStatus: RoutingProtectionStatus | null;
  loading: boolean;
  error?: string | null;
  developerModeEnabled: boolean;
  deepLink?: VersionedRoutingDeepLink | null;
  onOpenRequestLog?: (requestLogId: string) => void;
};

export function RoutingStatusDiagnosticsPanel({
  snapshot,
  runtimeOverlay,
  decisions,
  protectionStatus,
  loading,
  error = null,
  developerModeEnabled,
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

  if (!developerModeEnabled) return null;

  if (loading && !snapshot) {
    return (
      <SectionCard title="路由诊断">
        <div className="text-sm text-muted-foreground">正在读取路由状态...</div>
      </SectionCard>

    );
  }

  if (error && !snapshot) {
    return (
      <SectionCard title="路由诊断">
        <div className="grid gap-1 text-sm" role="alert">
          <div className="font-medium text-danger-foreground">无法读取路由诊断</div>
          <div className="break-words text-xs text-muted-foreground">{error}</div>
        </div>
      </SectionCard>
    );
  }

  if (!snapshot) {
    return (
      <SectionCard title="路由诊断">
        <EmptyState
          title="暂无路由诊断数据"
          description="刷新后仍无数据时，请先检查站点、密钥池和本地路由配置。"
        />
      </SectionCard>
    );
  }

  return (
    <section className="grid min-w-0 gap-4" aria-labelledby="routing-diagnostics-title">
      <SectionCard
        title="路由诊断"
        description="把候选、价格、实时并发和最近决策合在状态页里看。"
        action={<div className="flex flex-wrap items-center justify-end gap-2">
          <StatusBadge tone={snapshot.readModelStatus === "available" ? "healthy" : "warning"}>{readModelStatusLabel(snapshot.readModelStatus)}</StatusBadge>
          <StatusBadge tone={snapshot.plannerEvaluation === "available" ? "healthy" : "warning"}>{plannerEvaluationLabel(snapshot.plannerEvaluation)}</StatusBadge>
          <StatusBadge tone={snapshot.availabilityStatus === "available" ? "healthy" : "warning"}>{availabilityStatusLabel(snapshot.availabilityStatus)}</StatusBadge>
        </div>}
        contentClassName="grid min-w-0 gap-3"
      >
        {error ? (
          <div className="rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-xs text-danger-foreground" role="alert">
            路由诊断刷新失败，当前显示的是上一次成功读取的数据：{error}
          </div>
        ) : null}
        {snapshot.circuitReadModelStatus !== "available" || snapshot.aggregates.persistenceUnavailableCircuits > 0 ? (
          <div className="rounded-[var(--surface-radius)] border border-warning-border bg-warning-surface px-3 py-2 text-xs text-warning-foreground" role="status">
            熔断读模型暂不可用{snapshot.circuitReadModelCode ? `：${snapshot.circuitReadModelCode}` : ""}，候选不会按旧健康数据推断为可参与。
          </div>
        ) : null}
        <RoutingProjectionStatus snapshot={snapshot} />
        <div className="grid min-w-0 gap-2 text-sm sm:grid-cols-4">
          <ReadableMetric
            label="可参与候选"
            value={`${snapshot.aggregates.eligibleCandidates + snapshot.aggregates.conditionallyEligibleCandidates}/${snapshot.aggregates.totalCandidates}`}
          />
          <ReadableMetric label="价格缺口" value={`${unpricedCount}`} tone={unpricedCount > 0 ? "warning" : "healthy"} />
          <ReadableMetric label="实时并发" value={formatRuntimeCapacity(snapshot.candidates, overlayByKey)} />
          <ReadableMetric label="最近决策" value={`${decisions?.decisions.length ?? 0}`} />
        </div>

        {decisions?.decisions.length ? (
          <div className="grid gap-1 border-t border-border pt-2" aria-label="最近路由决策">
            {decisions.decisions.slice(0, 3).map((decision) => (
              <div key={decision.requestLogId} className="flex min-w-0 flex-wrap items-center justify-between gap-2 text-xs">
                <span className="min-w-0 truncate text-muted-foreground">
                  {decision.model ?? "未指定模型"} · {decision.status} · {decision.routeReason ?? "未记录原因"}
                </span>
                {onOpenRequestLog ? (
                  <Button size="sm" variant="ghost" onClick={() => onOpenRequestLog(decision.requestLogId)}>
                    查看请求
                  </Button>
                ) : null}
              </div>
            ))}
          </div>
        ) : null}

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
          <EmptyState title="没有候选密钥" description="当前配置下没有可参与路由的密钥。" />
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
                还有 {scopedCandidates.length - 6} 个候选，完整排序请到“设置”页调整。
              </div>
            ) : null}
          </div>
        )}
      </SectionCard>

      <SectionCard
        title="密钥熔断诊断"
        description="熔断器按每把站点密钥独立作用，恢复请求必须通过同层评分门。"
        contentClassName="grid min-w-0 gap-2"
      >
        {scopedCandidates.some((candidate) => candidate.diagnostics) ? (
          <div className="grid min-w-0 gap-2">
            {scopedCandidates.slice(0, 8).map((candidate) => (
              <CircuitLine key={candidate.stationKeyId} candidate={candidate} />
            ))}
          </div>
        ) : (
          <ProtectionSummary status={protectionStatus} />
        )}
      </SectionCard>

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
  const inFlight = overlay?.inFlight ?? candidate.capacity.inFlight;
  const participation = buildParticipationDisplay(
    candidate.participationStatus,
    candidate.participationReason,
  );
  const diagnostics = buildRoutingCandidateDiagnosticsDisplay(candidate);

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
        <StatusBadge tone={participation.tone}>{participation.label}</StatusBadge>
        <span className="text-xs text-muted-foreground">
          {capacityStatusLabel(candidate.capacity.status)} · 本地在途 {inFlight ?? 0}/{formatConcurrencyLimit(candidate.capacity.maxConcurrency)}
        </span>
      </div>
      {diagnostics ? (
        <div className="grid min-w-0 gap-x-4 gap-y-1 border-t border-border pt-2 text-xs text-muted-foreground sm:grid-cols-2 md:col-span-3 xl:grid-cols-4">
          <span className="min-w-0 break-words font-medium text-foreground">{diagnostics.score}</span>
          <span className="min-w-0 break-words">{diagnostics.qualityMetadata}</span>
          <span className="min-w-0 break-words sm:col-span-2">{diagnostics.sourceReliability}</span>
          <span className="min-w-0 break-words">{diagnostics.recentWindow}</span>
          <span className="min-w-0 break-words">{diagnostics.historicalWindow}</span>
          <span className="min-w-0 break-words sm:col-span-2">{diagnostics.latencySummary}</span>
          <span className="min-w-0 break-words">{diagnostics.sampleCounts}</span>
          <span className="min-w-0 break-words">{diagnostics.idleRealRoute}</span>
        </div>
      ) : (
        <div className="border-t border-border pt-2 text-xs text-muted-foreground md:col-span-3">
          V3 评分诊断暂不可用。
        </div>
      )}
    </div>
  );
}

function RoutingProjectionStatus({ snapshot }: { snapshot: RoutingWorkspaceSnapshot }) {
  const revisionKnown =
    snapshot.policyRevision != null ||
    snapshot.qualityRevision != null;
  const projectionKnown =
    snapshot.qualityProjectionBacklog != null ||
    snapshot.qualityProjectionLagSeconds != null ||
    snapshot.qualityStale != null;
  if (!revisionKnown && !projectionKnown && !snapshot.runtimeGenerationId) return null;

  const stale = snapshot.qualityStale === true;
  return (
    <div
      className={`grid min-w-0 gap-2 rounded-[var(--surface-radius)] border px-3 py-2 text-xs sm:grid-cols-2 xl:grid-cols-4 ${stale ? "border-warning-border bg-warning-surface text-warning-foreground" : "border-border bg-surface text-muted-foreground"}`}
      aria-label="路由投影状态"
    >
      <span className="min-w-0 break-words">
        运行代际：{snapshot.runtimeGenerationId ?? "尚未激活"}
      </span>
      <span className="min-w-0 break-words">
        revision：策略 {formatRevision(snapshot.policyRevision)} · 质量 {formatRevision(snapshot.qualityRevision)} · 熔断 gate r{snapshot.circuitRevision.processGateRevision} / durable r{snapshot.circuitRevision.persistenceHealthRevision}
      </span>
      <span className="min-w-0 break-words">
        质量投影：{stale ? "陈旧" : snapshot.qualityStale === false ? "最新" : "状态未知"} · 积压 {formatOptionalCount(snapshot.qualityProjectionBacklog)}
      </span>
      <span className="min-w-0 break-words">
        投影延迟：{formatProjectionLag(snapshot.qualityProjectionLagSeconds)}
      </span>
    </div>
  );
}

function CircuitLine({ candidate }: { candidate: RoutingWorkspaceCandidate }) {
  const diagnostics = buildRoutingCandidateDiagnosticsDisplay(candidate);
  if (!diagnostics) return null;
  const circuit = candidate.diagnostics?.circuit;
  const unavailable = circuit?.persistenceStatus === "unavailable";
  const warning = circuit?.state === "open" || circuit?.state === "half_open";
  return (
    <div className="grid min-w-0 gap-2 rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2 text-xs md:grid-cols-[minmax(0,0.8fr)_minmax(0,1fr)_minmax(0,1.2fr)] md:items-start">
      <div className="min-w-0">
        <div className="truncate font-medium text-foreground">{candidate.keyName}</div>
        <div className="truncate text-muted-foreground">{candidate.stationName}</div>
      </div>
      <div className="grid min-w-0 gap-1 text-muted-foreground">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <StatusBadge tone={unavailable ? "disabled" : warning ? "warning" : "healthy"}>{diagnostics.circuitState}</StatusBadge>
        </div>
        <span className="break-words">{diagnostics.circuitDetail}</span>
      </div>
      <div className="grid min-w-0 gap-1 text-muted-foreground">
        <span className="break-words">{diagnostics.halfOpenLease}</span>
        <span className="break-words">{diagnostics.scoreGate}</span>
      </div>
    </div>
  );
}

function plannerEvaluationLabel(status: RoutingWorkspaceSnapshot["plannerEvaluation"] | undefined) {
  return status === "available" ? "基线评估可用" : "基线评估暂不可用";
}

function availabilityStatusLabel(status: RoutingWorkspaceSnapshot["availabilityStatus"]) {
  const labels: Record<RoutingWorkspaceSnapshot["availabilityStatus"], string> = {
    available: "存在可用密钥",
    capacity_exhausted: "本地容量已耗尽",
    capacity_state_unavailable: "容量状态不可用",
    all_keys_unavailable: "全部密钥不可用",
  };
  return labels[status];
}

function capacityStatusLabel(status: RoutingWorkspaceCandidate["capacity"]["status"]) {
  const labels: Record<RoutingWorkspaceCandidate["capacity"]["status"], string> = {
    available: "容量可用",
    exhausted: "容量耗尽",
    state_unavailable: "容量状态不可用",
    unknown: "容量状态未知",
  };
  return labels[status];
}

function ProtectionSummary({ status }: { status: RoutingProtectionStatus | null }) {
  if (!status) return <div className="text-xs text-muted-foreground">保护状态暂不可用。</div>;
  const active = userVisibleProtectionEntries(status).filter((entry) => entry.state !== "no_protection");
  return (
    <div className="grid gap-1 border-t border-border pt-2 text-xs text-muted-foreground">
      <div className="flex flex-wrap gap-x-3 gap-y-1">
        <span>保护读模型：{readModelStatusLabel(status.readModelStatus)}</span>
        <span>状态条目：{active.length}</span>
      </div>
      {active.slice(0, 4).map((entry) => (
        <div key={`${entry.persistenceKind ?? "none"}:${entry.scope}`} className="flex min-w-0 flex-wrap gap-x-2 gap-y-1">
          <span>{protectionScopeKindLabel(entry.scopeKind)}</span>
          <StatusBadge tone={entry.state === "open" || entry.state === "half_open" ? "warning" : entry.state === "unavailable" ? "disabled" : "healthy"}>
            {protectionStateLabel(entry.state)}
          </StatusBadge>
          <span>{protectionExplanationLabel(entry.explanationKey)}</span>
          {entry.recentFailureCode ? <span>最近失败：{entry.recentFailureCode}</span> : null}
        </div>
      ))}
    </div>
  );
}

function protectionStateLabel(state: string) {
  const labels: Record<string, string> = {
    no_protection: "无保护",
    degraded: "监控中",
    cooldown: "冷却中",
    blocked: "已阻断",
    open: "保护开启",
    half_open: "半开探测",
    unavailable: "不可用",
  };
  return labels[state] ?? "状态未知";
}

function readModelStatusLabel(status: string) {
  return status === "available" ? "数据正常" : "数据不可用";
}

function protectionScopeKindLabel(kind: string | null) {
  const labels: Record<string, string> = {
    credential: "凭据",
    account: "账号",
    endpoint: "端点",
    model: "模型",
    station_key: "站点密钥",
  };
  return kind ? labels[kind] ?? "路由作用域" : "路由作用域";
}

function protectionExplanationLabel(key: string) {
  const labels: Record<string, string> = {
    "routing.protection.none_active": "当前没有保护条目",
    "routing.protection.closed_monitoring": "未打开保护，持续监控中",
    "routing.protection.degraded": "保护降级，持续监控中",
    "routing.protection.cooldown": "保护冷却中，暂时抑制候选",
    "routing.protection.blocked": "保护已阻断候选",
    "routing.protection.open": "保护已打开，暂时抑制候选",
    "routing.protection.half_open": "保护半开，正在恢复探测",
    "routing.protection.unavailable": "保护明细暂不可用",
  };
  return labels[key] ?? "暂无可读说明";
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

function formatRevision(value: number | null | undefined) {
  return value == null ? "未知" : `r${value}`;
}

function formatOptionalCount(value: number | null | undefined) {
  return value == null ? "未知" : value.toLocaleString("zh-CN");
}

function formatProjectionLag(value: number | null | undefined) {
  if (value == null) return "未知";
  if (value < 60) return `${value} 秒`;
  return `${Math.floor(value / 60)} 分 ${value % 60} 秒`;
}

function formatPrice(candidate: RoutingWorkspaceCandidate) {
  if (candidate.multiplier.multiplier != null) return `${candidate.multiplier.multiplier}x`;
  if (candidate.pricing.basis === "unpriced") {
    return candidate.pricing.reason === "pricing_context_missing" ? "需指定模型" : "缺失";
  }
  if (candidate.pricing.comparisonValue != null) return `${candidate.pricing.comparisonValue}`;
  return candidate.pricing.statusLabel;
}

