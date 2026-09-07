import { describe, expect, it } from "vitest";
import type { RequestLog } from "@/lib/types/proxy";
import { RECENT_USAGE_LIMIT, selectRecentUsageLogs } from "./recentUsageViewModel";

describe("selectRecentUsageLogs", () => {
  it("keeps the newest settled records and skips in-progress requests", () => {
    const logs = [
      requestLog("in-progress-status", { status: "in_progress", lifecycleStatus: "admitted" }),
      requestLog("success-1", { status: "success", lifecycleStatus: "completed" }),
      requestLog("admitted-lifecycle", { status: "success", lifecycleStatus: "admitted" }),
      requestLog("failed-1", { status: "failed", lifecycleStatus: "failed" }),
      requestLog("fallback-1", { status: "fallback", lifecycleStatus: "partial_success" }),
      requestLog("in-progress-2", { status: "in_progress", lifecycleStatus: null }),
      requestLog("success-2", { status: "success", lifecycleStatus: "interrupted" }),
    ];

    expect(selectRecentUsageLogs(logs).map((log) => log.id)).toEqual([
      "success-1",
      "failed-1",
      "fallback-1",
      "success-2",
    ]);
  });

  it("returns at most five settled records in the original recent-first order", () => {
    const logs = Array.from({ length: 8 }, (_, index) =>
      requestLog(`settled-${index + 1}`, { status: "success", lifecycleStatus: "completed" }),
    );

    expect(selectRecentUsageLogs(logs).map((log) => log.id)).toEqual([
      "settled-1",
      "settled-2",
      "settled-3",
      "settled-4",
      "settled-5",
    ]);
    expect(RECENT_USAGE_LIMIT).toBe(5);
  });

  it("returns an empty list when every recent record is still processing", () => {
    expect(
      selectRecentUsageLogs([
        requestLog("active-1", { status: "in_progress", lifecycleStatus: "admitted" }),
        requestLog("active-2", { status: "in_progress", lifecycleStatus: "admitted" }),
      ]),
    ).toEqual([]);
  });
});

function requestLog(
  id: string,
  fields: Pick<RequestLog, "status" | "lifecycleStatus">,
): RequestLog {
  return { id, ...fields } as RequestLog;
}
