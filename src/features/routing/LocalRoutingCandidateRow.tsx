import { KeyRound } from "lucide-react";
import { ObjectRow, StatusBadge } from "@/components/ui";
import type { LocalRoutingCandidateRow as LocalRoutingCandidate } from "@/lib/types/localRouting";

type LocalRoutingCandidateRowProps = {
  candidate: LocalRoutingCandidate;
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

export function LocalRoutingCandidateRow({ candidate }: LocalRoutingCandidateRowProps) {
  const facts = candidate.facts.slice(0, 3).map((fact) => fact.label).join(" / ");

  return (
    <ObjectRow
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
        </>
      }
      metrics={[
        { label: "顺位", value: candidate.priority + 1, tone: "neutral" },
        { label: "评分", value: candidate.score == null ? "-" : candidate.score.toFixed(1), tone: candidate.score == null ? "neutral" : "good" },
        { label: "冷却", value: candidate.cooldownUntil ? "进行中" : "无", tone: candidate.cooldownUntil ? "warning" : "neutral" },
      ]}
    />
  );
}
