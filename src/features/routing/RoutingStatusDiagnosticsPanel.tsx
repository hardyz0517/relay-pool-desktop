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

type RoutingStatusDiagnosticsPanelProps = {
  snapshot: RoutingWorkspaceSnapshot | null;
  runtimeOverlay: RoutingRuntimeOverlay | null;
  decisions: RecentRouteDecisionsPage | null;
  protectionStatus: RoutingProtectionStatus | null;
  loading: boolean;
  developerModeEnabled: boolean;
  deepLink?: VersionedRoutingDeepLink | null;
  onOpenRequestLog?: (requestLogId: string) => void;
};

export function sortFailureDomainDiagnostics(
  diagnostics: RoutingProtectionStatus["failureDomains"] = [],
) {
  return [...diagnostics].sort(
    (left, right) =>
      right.candidateCount - left.candidateCount ||
      (left.commitment ?? left.providerFamily ?? "").localeCompare(
        right.commitment ?? right.providerFamily ?? "",
      ),
  );
}

export function RoutingStatusDiagnosticsPanel({
  snapshot,
  runtimeOverlay,
  decisions,
  protectionStatus,
  loading,
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
  const availableCount =
    snapshot?.candidates.filter((candidate) => {
      const overlay = overlayByKey.get(candidate.stationKeyId);
      const healthState = overlay?.healthState ?? candidate.healthState;
      return ["ready", "available"].includes(healthState) && candidate.hardRejectionCodes.length === 0;
    }).length ?? 0;
  const domainGroups = useMemo(
    () => sortFailureDomainDiagnostics(protectionStatus?.failureDomains),
    [protectionStatus?.failureDomains],
  );
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
        description="把候选、价格、实时并发、故障域和最近决策合在状态页里看。"
        action={<StatusBadge tone={snapshot.readModelStatus === "available" ? "healthy" : "warning"}>{readModelStatusLabel(snapshot.readModelStatus)}</StatusBadge>}
        contentClassName="grid min-w-0 gap-3"
      >
        <div className="grid min-w-0 gap-2 text-sm sm:grid-cols-4">
          <ReadableMetric label="可用候选" value={`${availableCount}/${snapshot.candidates.length}`} />
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
        title="Provider / 故障域"
        description="按低敏感 commitment 聚合同一容量域，保护状态与运行时容量状态分开显示。"
        contentClassName="grid min-w-0 gap-2"
      >
        {domainGroups.length === 0 ? (
          <EmptyState title="暂无故障域诊断" description="候选缺少可用身份，或保护诊断读模型暂不可用。系统不会猜测故障域归属。" />
        ) : (
          <div className="grid gap-2">
            {domainGroups.slice(0, 8).map((domain) => (
              <div key={`${domain.commitment ?? "unresolved"}:${domain.providerFamily ?? "unknown"}:${domain.deploymentIdentity ?? "unknown"}:${domain.regionIdentity ?? "unknown"}`} className="grid min-w-0 gap-1 rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2 text-xs md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] md:items-center">
                <div className="min-w-0">
                  <div className="truncate font-medium text-foreground">{domain.providerFamily ?? "未配置 provider"}</div>
                  <div className="truncate text-muted-foreground">{domain.deploymentIdentity ?? "未声明 deployment"} / {domain.regionIdentity ?? "未声明 region"}</div>
                </div>
                <div className="min-w-0 text-muted-foreground">
                  <div>{domain.candidateCount} 个候选，{domain.schedulableCandidateCount} 个可调度</div>
                  <div className="truncate" title={domain.commitment ?? undefined}>身份：{failureDomainResolutionLabel(domain.resolution)}</div>
                </div>
                <div className="flex min-w-0 flex-wrap items-center gap-2 md:justify-end">
                  <StatusBadge tone={domain.status === "open" || domain.status === "half_open" || domain.status === "blocked" ? "warning" : domain.status === "unavailable" ? "disabled" : "healthy"}>
                    {protectionStateLabel(domain.status)}
                  </StatusBadge>
                  {domain.recentFailureCode ? <span className="truncate text-muted-foreground">最近：{domain.recentFailureCode}</span> : null}
                </div>
                <div className="truncate text-muted-foreground md:col-span-3" title={domain.explanationKey}>解释：{domain.explanationKey}</div>
              </div>
            ))}
          </div>
        )}
        <ProtectionSummary status={protectionStatus} />
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
        <div className="truncate" title={candidate.failureDomain.commitment ?? undefined}>
          故障域：{formatFailureDomain(candidate.failureDomain)}
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2 md:justify-end">
        <StatusBadge tone={blocked ? "warning" : ["ready", "available"].includes(healthState) ? "healthy" : healthState === "degraded" ? "warning" : "disabled"}>
          {blocked ? "不可参与" : healthStateLabel(healthState)}
        </StatusBadge>
        <span className="text-xs text-muted-foreground">
          本地在途 {inFlight ?? 0}/{formatConcurrencyLimit(candidate.capacity.maxConcurrency)}
        </span>
      </div>
    </div>
  );
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

function formatFailureDomain(
  failureDomain: RoutingWorkspaceCandidate["failureDomain"],
): string {
  if (failureDomain.resolution === "not_configured") return "未配置";
  if (failureDomain.resolution === "invalid_identity") return "身份无效";
  if (failureDomain.resolution === "model_required") return "等待模型";
  const identity = [
    failureDomain.providerFamily,
    failureDomain.deploymentIdentity,
    failureDomain.regionIdentity,
  ]
    .filter(Boolean)
    .join(" / ");
  return identity || "已解析";
}

function failureDomainResolutionLabel(resolution: string) {
  if (resolution === "resolved") return "已解析";
  if (resolution === "model_required") return "等待模型";
  if (resolution === "invalid_identity") return "身份无效";
  return "未配置";
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

function healthStateLabel(state: string) {
  const labels: Record<string, string> = {
    ready: "可参与",
    available: "可参与",
    cooldown: "冷却中",
    degraded: "降级监控",
    offline: "离线",
    unknown: "状态未知",
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
    capacity_domain: "容量域",
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

function formatPrice(candidate: RoutingWorkspaceCandidate) {
  if (candidate.multiplier.multiplier != null) return `${candidate.multiplier.multiplier}x`;
  if (candidate.pricing.basis === "unpriced") {
    return candidate.pricing.reason === "pricing_context_missing" ? "需指定模型" : "缺失";
  }
  if (candidate.pricing.comparisonValue != null) return `${candidate.pricing.comparisonValue}`;
  return candidate.pricing.statusLabel;
}

