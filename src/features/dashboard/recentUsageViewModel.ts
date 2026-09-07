import type { RequestLog } from "@/lib/types/proxy";

export const RECENT_USAGE_LIMIT = 5;

export function selectRecentUsageLogs(
  logs: readonly RequestLog[],
  limit = RECENT_USAGE_LIMIT,
): RequestLog[] {
  const safeLimit = Number.isFinite(limit) ? Math.max(0, Math.trunc(limit)) : 0;
  if (safeLimit === 0) return [];
  return logs.filter((log) => !isRecentUsageInProgress(log)).slice(0, safeLimit);
}

function isRecentUsageInProgress(log: Pick<RequestLog, "status" | "lifecycleStatus">) {
  return log.status === "in_progress" || log.lifecycleStatus === "admitted";
}
