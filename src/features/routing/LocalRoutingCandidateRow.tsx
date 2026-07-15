import type { DraggableAttributes, DraggableSyntheticListeners } from "@dnd-kit/core";
import { GripVertical } from "lucide-react";
import type { ReactNode } from "react";
import { StatusBadge } from "@/components/ui";
import { toTimestampMillis } from "@/lib/time";
import type { LocalRoutingCandidateRow as LocalRoutingCandidate } from "@/lib/types/localRouting";
import type { RouteEndpointKind } from "@/lib/types/routing";
import { cn } from "@/lib/utils";
import {
  buildCooldownDisplay,
  formatBalanceStatus,
  formatMultiplierSource,
  formatPreviewRejectReason,
} from "./localRoutingStatusViewModel";

type LocalRoutingCandidateRowProps = {
  candidate: LocalRoutingCandidate;
  order?: number;
  syncState?: "idle" | "saving" | "synced" | "failed";
  dragDisabled?: boolean;
  dragAttributes?: DraggableAttributes;
  dragListeners?: DraggableSyntheticListeners;
};

const healthLabels: Record<LocalRoutingCandidate["healthState"], string> = {
  ready: "就绪",
  cooldown: "冷却",
  degraded: "降级",
  offline: "离线",
  unknown: "未知",
};

const healthTones: Record<LocalRoutingCandidate["healthState"], "healthy" | "warning" | "error" | "disabled" | "info"> = {
  ready: "healthy",
  cooldown: "warning",
  degraded: "warning",
  offline: "error",
  unknown: "disabled",
};

const syncLabels: Record<NonNullable<LocalRoutingCandidateRowProps["syncState"]>, string | null> = {
  idle: null,
  saving: "保存中",
  synced: "已同步",
  failed: "保存失败",
};

const syncTones: Record<
  Exclude<NonNullable<LocalRoutingCandidateRowProps["syncState"]>, "idle">,
  "healthy" | "warning" | "error"
> = {
  saving: "warning",
  synced: "healthy",
  failed: "error",
};

const endpointLabels: Record<RouteEndpointKind, string> = {
  chat_completions: "聊天补全",
  responses: "Responses",
  models: "模型列表",
  embeddings: "向量",
};

export function LocalRoutingCandidateRow({
  candidate,
  order,
  syncState = "idle",
  dragDisabled = false,
  dragAttributes,
  dragListeners,
}: LocalRoutingCandidateRowProps) {
  const syncLabel = syncLabels[syncState];
  const isSortable = Boolean(dragAttributes || dragListeners);
  const balanceFact = candidate.facts.find((fact) => fact.kind === "balance");
  const cooldownUntilMs =
    candidate.cooldownUntil == null ? null : toTimestampMillis(candidate.cooldownUntil);
  const cooldown = buildCooldownDisplay(candidate.healthState, cooldownUntilMs, Date.now());
  const primaryRejectReason = !candidate.schedulable
    ? "asset_unavailable"
    : (candidate.previewRejectReasons[0] ?? null);
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
    <div
      className={cn(
        "grid min-h-[68px] gap-3 px-3 py-2.5 sm:items-center",
        isSortable
          ? "sm:grid-cols-[24px_minmax(220px,1.6fr)_minmax(120px,.8fr)_minmax(104px,.65fr)_minmax(92px,.55fr)_minmax(88px,.5fr)]"
          : "sm:grid-cols-[minmax(220px,1.6fr)_minmax(120px,.8fr)_minmax(104px,.65fr)_minmax(92px,.55fr)_minmax(88px,.5fr)]",
      )}
    >
      {isSortable ? (
        <button
          type="button"
          aria-label="调整候选顺序"
          title="调整候选顺序"
          tabIndex={dragDisabled ? -1 : 0}
          disabled={dragDisabled}
          className={cn(
            "flex h-7 w-5 items-center justify-center self-start text-muted-foreground/45 sm:self-center",
            dragDisabled ? "cursor-not-allowed" : "cursor-grab active:cursor-grabbing hover:text-muted-foreground",
          )}
          {...dragAttributes}
          {...dragListeners}
        >
          <GripVertical className="h-4 w-4" />
        </button>
      ) : null}
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold text-muted-foreground">#{order ?? candidate.priority + 1}</span>
          <span className="truncate text-[13px] font-semibold text-foreground">
            {candidate.keyName}
          </span>
          {syncLabel && syncState !== "idle" ? (
            <StatusBadge tone={syncTones[syncState]}>{syncLabel}</StatusBadge>
          ) : null}
        </div>
        <div className="mt-0.5 truncate text-xs text-muted-foreground">
          {candidate.stationName} · {endpointLabels[candidate.endpoint] ?? candidate.endpoint}
        </div>
      </div>
      <MetricCell label="参与状态">
        <div className="flex flex-wrap items-center gap-1.5">
          <StatusBadge tone={participationTone}>{participationLabel}</StatusBadge>
          {!candidate.enabled ? <StatusBadge tone="disabled">停用</StatusBadge> : null}
          <StatusBadge tone={healthTones[candidate.healthState]}>
            {healthLabels[candidate.healthState]}
          </StatusBadge>
        </div>
        {!candidate.previewEligible && primaryRejectReason ? (
          <div className="mt-1 text-xs text-warning-foreground">
            {formatPreviewRejectReason(primaryRejectReason)}
          </div>
        ) : null}
        {!candidate.routingGroupMatch ? (
          <div className="mt-1 text-xs text-warning-foreground">分组不匹配</div>
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
