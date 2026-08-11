import type { AlertingIncident } from "@/lib/types/alerting";

const TASK_ORDER = ["balance", "groups", "detect", "full"] as const;
const TASK_LABELS: Record<(typeof TASK_ORDER)[number], string> = {
  balance: "余额",
  groups: "分组",
  detect: "站点检测",
  full: "完整采集",
};

export function collectorFailureTaskLabel(
  incident: Pick<AlertingIncident, "eventType" | "collectorFailedTaskTypes">,
) {
  if (incident.eventType !== "collector_failed") return null;

  const values = new Set(incident.collectorFailedTaskTypes);
  const taskTypes = TASK_ORDER.filter((taskType) => values.has(taskType));
  if (taskTypes.length === 0) return null;

  if (taskTypes.every((taskType) => taskType === "balance" || taskType === "groups")) {
    return `${taskTypes.map((taskType) => TASK_LABELS[taskType]).join("、")}采集`;
  }
  return taskTypes.map((taskType) => TASK_LABELS[taskType]).join("、");
}
