import type { DraggableAttributes, DraggableSyntheticListeners } from "@dnd-kit/core";
import { KeyRound } from "lucide-react";
import { ObjectRow, StatusBadge } from "@/components/ui";
import type { LocalRoutingCandidateRow as LocalRoutingCandidate } from "@/lib/types/localRouting";

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

export function LocalRoutingCandidateRow({
  candidate,
  order,
  syncState = "idle",
  dragDisabled = false,
  dragAttributes,
  dragListeners,
}: LocalRoutingCandidateRowProps) {
  const facts = candidate.facts.slice(0, 3).map((fact) => fact.label).join(" / ");
  const syncLabel = syncLabels[syncState];
  const isSortable = Boolean(dragAttributes || dragListeners);

  return (
    <ObjectRow
      draggable={isSortable}
      dragHandleProps={
        isSortable
          ? {
              attributes: dragAttributes,
              listeners: dragListeners,
              disabled: dragDisabled,
            }
          : undefined
      }
      className="min-h-[72px]"
      icon={<KeyRound className="h-4 w-4" />}
      title={candidate.keyName}
      subtitle={`${candidate.stationName} / ${candidate.endpoint}${facts ? ` / ${facts}` : ""}`}
      badges={
        <>
          <StatusBadge tone={candidate.enabled ? "healthy" : "disabled"}>
            {candidate.enabled ? "启用" : "停用"}
          </StatusBadge>
          <StatusBadge tone={healthTones[candidate.healthState]}>
            {healthLabels[candidate.healthState]}
          </StatusBadge>
          {syncLabel && syncState !== "idle" ? (
            <StatusBadge tone={syncTones[syncState]}>{syncLabel}</StatusBadge>
          ) : null}
        </>
      }
      metrics={[
        { label: "顺位", value: order ?? candidate.priority + 1, tone: "neutral" },
        { label: "评分", value: candidate.score == null ? "-" : candidate.score.toFixed(1), tone: candidate.score == null ? "neutral" : "good" },
        { label: "冷却", value: candidate.cooldownUntil ? "进行中" : "无", tone: candidate.cooldownUntil ? "warning" : "neutral" },
      ]}
    />
  );
}
