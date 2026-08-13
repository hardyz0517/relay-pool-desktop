import type { DraggableAttributes, DraggableSyntheticListeners } from "@dnd-kit/core";
import { GripVertical } from "lucide-react";
import type { ReactNode } from "react";
import { StatusBadge } from "@/components/ui";
import { toTimestampMillis } from "@/lib/time";
import type { RoutingCandidateView as LocalRoutingCandidate } from "@/lib/types/routingWorkspace";
import type { RouteEndpointKind } from "@/lib/types/routing";
import { cn } from "@/lib/utils";
import {
  buildCandidateDisplayFacts,
  buildCandidateHealthDisplay,
  buildCooldownDisplay,
} from "./localRoutingStatusViewModel";

type LocalRoutingCandidateRowProps = {
  candidate: LocalRoutingCandidate;
  order?: number;
  syncState?: "idle" | "saving" | "synced" | "failed";
  dragDisabled?: boolean;
  dragAttributes?: DraggableAttributes;
  dragListeners?: DraggableSyntheticListeners;
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

export function LocalRoutingCandidateHeader() {
  return (
    <div className="hidden min-h-9 grid-cols-[24px_minmax(220px,1.6fr)_minmax(110px,.75fr)_minmax(88px,.55fr)_minmax(96px,.6fr)_minmax(80px,.5fr)_minmax(76px,.45fr)] items-center gap-3 border-b border-border bg-surface-subtle px-3 py-2 text-[11px] font-medium text-muted-foreground lg:grid">
      <span aria-hidden="true" />
      <span>候选密钥</span>
      <span>参与状态</span>
      <span>健康状态</span>
      <span>有效倍率</span>
      <span>余额</span>
      <span>冷却</span>
    </div>
  );
}

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
  const cooldownUntilMs =
    candidate.cooldownUntil == null ? null : toTimestampMillis(candidate.cooldownUntil);
  const cooldown = buildCooldownDisplay(candidate.healthState, cooldownUntilMs, Date.now());
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
    <div
      className={cn(
        "grid min-h-[68px] gap-3 px-3 py-2.5 lg:items-center",
        isSortable
          ? "lg:grid-cols-[24px_minmax(220px,1.6fr)_minmax(110px,.75fr)_minmax(88px,.55fr)_minmax(96px,.6fr)_minmax(80px,.5fr)_minmax(76px,.45fr)]"
          : "lg:grid-cols-[minmax(220px,1.6fr)_minmax(110px,.75fr)_minmax(88px,.55fr)_minmax(96px,.6fr)_minmax(80px,.5fr)_minmax(76px,.45fr)]",
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
            "flex h-7 w-5 items-center justify-center self-start text-muted-foreground/45 lg:self-center",
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
        </div>
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
      <MetricCell label="余额" value={displayFacts.balanceLabel} detail={displayFacts.balanceDetail} />
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
      <div className="text-[11px] text-muted-foreground lg:hidden">{label}</div>
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
