import type { ReactNode } from "react";
import { StatusBadge } from "@/components/ui";
import { toTimestampMillis } from "@/lib/time";
import type { LocalRoutingCandidateRow as LocalRoutingCandidate } from "@/lib/types/localRouting";
import {
  buildCooldownDisplay,
  formatBalanceStatus,
  formatMultiplierSource,
  formatPreviewRejectReason,
} from "./localRoutingStatusViewModel";

type LocalRoutingStatusCandidateRowProps = {
  candidate: LocalRoutingCandidate;
  order: number;
  nowMs: number;
};

export function LocalRoutingStatusCandidateRow({
  candidate,
  order,
  nowMs,
}: LocalRoutingStatusCandidateRowProps) {
  const cooldownUntilMs =
    candidate.cooldownUntil == null ? null : toTimestampMillis(candidate.cooldownUntil);
  const cooldown = buildCooldownDisplay(candidate.healthState, cooldownUntilMs, nowMs);
  const primaryRejectReason = candidate.previewRejectReasons[0] ?? null;
  const balanceFact = candidate.facts.find((fact) => fact.kind === "balance");
  const multiplierLabel =
    candidate.effectiveMultiplier == null
      ? "未确认"
      : `${candidate.effectiveMultiplier.toFixed(4)}x`;
  const multiplierSourceLabel = formatMultiplierSource(
    candidate.effectiveMultiplierSource,
    candidate.effectiveMultiplierConfidence,
  );
  const balanceLabel = formatBalanceStatus(balanceFact?.value ?? null);

  return (
    <div className="grid min-h-[68px] gap-3 px-3 py-2.5 sm:grid-cols-[minmax(220px,1.6fr)_minmax(120px,.8fr)_minmax(104px,.65fr)_minmax(92px,.55fr)_minmax(88px,.5fr)] sm:items-center">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold text-muted-foreground">#{order}</span>
          <span className="truncate text-[13px] font-semibold text-foreground">
            {candidate.keyName}
          </span>
        </div>
        <div className="mt-0.5 truncate text-xs text-muted-foreground">
          {candidate.stationName} · 聊天补全
        </div>
      </div>
      <MetricCell label="参与状态">
        <StatusBadge tone={candidate.previewEligible ? "healthy" : "warning"}>
          {candidate.previewEligible ? "可参与" : "不参与"}
        </StatusBadge>
        {!candidate.previewEligible && primaryRejectReason ? (
          <div className="mt-1 text-xs text-warning-foreground">
            {formatPreviewRejectReason(primaryRejectReason)}
          </div>
        ) : null}
      </MetricCell>
      <MetricCell label="有效倍率" value={multiplierLabel} detail={multiplierSourceLabel} />
      <MetricCell label="余额" value={balanceLabel} />
      <MetricCell
        label="冷却"
        value={cooldown.label}
        tone={cooldown.active ? "warning" : "neutral"}
      />
    </div>
  );
}

function MetricCell({
  label,
  value,
  detail,
  tone = "neutral",
  children,
}: {
  label: string;
  value?: ReactNode;
  detail?: ReactNode;
  tone?: "neutral" | "warning";
  children?: ReactNode;
}) {
  return (
    <div className="min-w-0">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      {children ?? (
        <div
          className={
            tone === "warning"
              ? "text-[13px] font-semibold text-warning-foreground"
              : "text-[13px] font-semibold text-foreground"
          }
        >
          {value}
        </div>
      )}
      {detail ? <div className="truncate text-[11px] text-muted-foreground">{detail}</div> : null}
    </div>
  );
}
