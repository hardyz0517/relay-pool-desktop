import type { ReactNode } from "react";
import { StatusBadge } from "@/components/ui";
import { toTimestampMillis } from "@/lib/time";
import type { RoutingCandidateView as LocalRoutingCandidate } from "@/lib/types/routingWorkspace";
import {
  buildCandidateDisplayFacts,
  buildCandidateHealthDisplay,
  buildCooldownDisplay,
} from "./localRoutingStatusViewModel";

type LocalRoutingStatusCandidateRowProps = {
  candidate: LocalRoutingCandidate;
  order: number;
  nowMs: number;
};

export function LocalRoutingStatusCandidateHeader() {
  return (
    <div className="hidden min-h-9 grid-cols-[minmax(220px,1.6fr)_minmax(110px,.75fr)_minmax(88px,.55fr)_minmax(96px,.6fr)_minmax(80px,.5fr)_minmax(76px,.45fr)_minmax(72px,.45fr)] items-center gap-3 border-b border-border bg-surface-subtle px-3 py-2 text-[11px] font-medium text-muted-foreground md:grid">
      <span>候选密钥</span>
      <span className="text-center">参与状态</span>
      <span className="text-center">健康状态</span>
      <span className="text-center">有效倍率</span>
      <span className="text-center">余额</span>
      <span className="text-center">冷却</span>
      <span className="text-center" title="每秒刷新">当前并发</span>
    </div>
  );
}

export function LocalRoutingStatusCandidateRow({
  candidate,
  order,
  nowMs,
}: LocalRoutingStatusCandidateRowProps) {
  const cooldownUntilMs =
    candidate.cooldownUntil == null ? null : toTimestampMillis(candidate.cooldownUntil);
  const cooldown = buildCooldownDisplay(candidate.healthState, cooldownUntilMs, nowMs);
  const health = buildCandidateHealthDisplay(candidate.healthState);
  const displayFacts = buildCandidateDisplayFacts(candidate);
  const participationTone = !candidate.schedulable
    ? "disabled"
    : candidate.previewEligible
      ? "healthy"
      : "warning";
  const participationLabel = !candidate.schedulable
    ? "已暂停路由"
    : candidate.previewEligible
      ? "可参与"
      : "不参与";

  return (
    <div className="grid min-h-[68px] gap-3 px-3 py-2.5 md:grid-cols-[minmax(220px,1.6fr)_minmax(110px,.75fr)_minmax(88px,.55fr)_minmax(96px,.6fr)_minmax(80px,.5fr)_minmax(76px,.45fr)_minmax(72px,.45fr)] md:items-center">
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
        <StatusBadge tone={participationTone}>{participationLabel}</StatusBadge>
        {!candidate.previewEligible && displayFacts.rejectReasonLabel ? (
          <div className="mt-1 text-xs text-warning-foreground">
            {displayFacts.rejectReasonLabel}
          </div>
        ) : null}
      </MetricCell>
      <MetricCell label="健康状态">
        <StatusBadge tone={health.tone}>{health.label}</StatusBadge>
      </MetricCell>
      <MetricCell label="有效倍率" value={displayFacts.multiplierLabel} detail={displayFacts.multiplierDetail} />
      <MetricCell
        label="余额"
        value={
          displayFacts.balanceUnit ? (
            <>
              <span className="text-success-foreground">{displayFacts.balanceAmountLabel}</span>
              {displayFacts.balanceUnit}
            </>
          ) : (
            displayFacts.balanceLabel
          )
        }
        detail={displayFacts.balanceDetail}
      />
      <MetricCell
        label="冷却"
        value={cooldown.label}
        tone={cooldown.active ? "warning" : "neutral"}
      />
      <MetricCell label="当前并发">
        <StatusBadge tone="disabled">
          {candidate.currentConcurrency == null ? "—" : String(candidate.currentConcurrency)}
        </StatusBadge>
      </MetricCell>
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
    <div className="min-w-0 md:flex md:flex-col md:items-center md:text-center">
      <div className="text-[11px] text-muted-foreground md:hidden">{label}</div>
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
