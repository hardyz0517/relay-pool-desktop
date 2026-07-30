import { Dialog, KeyValueRow, PropertyList, StatusBadge } from "@/components/ui";
import {
  channelMonitorAttemptsQueryOptions,
  channelMonitorExecutionQueryOptions,
} from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { formatLatency, formatTime } from "../channelStatusViewModel";

type MonitorExecutionDrawerProps = {
  executionId: string | null;
  onClose: () => void;
};

export function MonitorExecutionDrawer({ executionId, onClose }: MonitorExecutionDrawerProps) {
  const detailQuery = useActivityQuery(channelMonitorExecutionQueryOptions(executionId));
  const attemptsQuery = useActivityQuery(
    channelMonitorAttemptsQueryOptions({ executionId: executionId ?? "", limit: 100 }),
  );
  const detail = detailQuery.data;

  return (
    <Dialog
      open={Boolean(executionId)}
      title="监控执行详情"
      description={executionId ?? undefined}
      onClose={onClose}
      className="max-w-[980px]"
    >
      <div className="space-y-4 p-5">
        {!detail ? (
          <div className="text-sm text-muted-foreground">
            {detailQuery.isPending ? "正在加载 execution..." : "暂无 execution 详情"}
          </div>
        ) : (
          <>
            <section className="rounded-[var(--surface-radius)] border border-border bg-surface-subtle p-4">
              <div className="mb-3 flex items-center justify-between gap-3">
                <div className="font-medium text-foreground">{detail.execution.monitorId}</div>
                <StatusBadge tone={detail.execution.status === "completed" ? "healthy" : "info"}>
                  {detail.execution.status}
                </StatusBadge>
              </div>
              <PropertyList>
                <KeyValueRow label="触发" value={`${detail.execution.triggerKind} · ${detail.execution.triggerRequestId ?? "--"}`} />
                <KeyValueRow label="计划/开始/结束" value={`${formatTime(detail.execution.plannedAtMs)} / ${formatTime(detail.execution.startedAtMs)} / ${formatTime(detail.execution.finishedAtMs)}`} />
                <KeyValueRow label="目标统计" value={`total ${detail.execution.targetCount} · available ${detail.execution.availableCount} · degraded ${detail.execution.degradedCount} · unavailable ${detail.execution.unavailableCount} · skipped ${detail.execution.skippedCount}`} />
                <KeyValueRow label="摘要" value={`${detail.execution.summaryOutcome ?? "--"} · ${detail.execution.summaryFailureKind ?? "--"}`} />
              </PropertyList>
            </section>

            <section>
              <div className="mb-2 text-sm font-medium text-foreground">Target Results</div>
              <div className="overflow-hidden rounded-[var(--surface-radius)] border border-border">
                <table className="w-full min-w-[860px] border-separate border-spacing-0 text-left text-xs">
                  <thead className="bg-surface-subtle text-muted-foreground">
                    <tr>
                      <th className="border-b border-border px-3 py-2">Key</th>
                      <th className="border-b border-border px-3 py-2">Outcome</th>
                      <th className="border-b border-border px-3 py-2">Model</th>
                      <th className="border-b border-border px-3 py-2">Profile</th>
                      <th className="border-b border-border px-3 py-2">Attempts</th>
                      <th className="border-b border-border px-3 py-2">Health writeback</th>
                      <th className="border-b border-border px-3 py-2">Latency</th>
                    </tr>
                  </thead>
                  <tbody>
                    {detail.targets.map((target) => (
                      <tr key={target.targetResultId}>
                        <td className="border-b border-border px-3 py-2">{target.stationKeyId ?? target.stationId}</td>
                        <td className="border-b border-border px-3 py-2">
                          {target.terminalOutcome}
                          {target.terminalFailureKind ? <span className="text-muted-foreground"> · {target.terminalFailureKind}</span> : null}
                        </td>
                        <td className="border-b border-border px-3 py-2">
                          {target.requestedModel}
                          {target.effectiveModel ? <span className="text-muted-foreground"> → {target.effectiveModel}</span> : null}
                        </td>
                        <td className="border-b border-border px-3 py-2">{target.clientProfileId}@{target.clientProfileVersion}</td>
                        <td className="border-b border-border px-3 py-2">{target.attemptCount}</td>
                        <td className="border-b border-border px-3 py-2">
                          {target.healthWritebackDecision}
                          {target.healthWritebackReason ? <span className="text-muted-foreground"> · {target.healthWritebackReason}</span> : null}
                        </td>
                        <td className="border-b border-border px-3 py-2">{formatLatency(target.latencyMs)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </section>

            <section>
              <div className="mb-2 text-sm font-medium text-foreground">Attempts</div>
              <div className="space-y-2">
                {(attemptsQuery.data?.items ?? []).map((attempt) => (
                  <div key={attempt.attemptId} className="rounded-[var(--surface-radius)] border border-border bg-surface-subtle px-3 py-2 text-xs">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <span className="font-medium text-foreground">
                        #{attempt.attemptNumber} · {attempt.modelRole} · {attempt.model}
                      </span>
                      <span className="text-muted-foreground">
                        {attempt.outcome} · HTTP {attempt.httpStatus ?? "--"} · {formatLatency(attempt.latencyMs)}
                      </span>
                    </div>
                    <div className="mt-1 text-muted-foreground">
                      {attempt.protocolKind} · {attempt.clientProfileId}@{attempt.clientProfileVersion} · {attempt.transportMode}
                      {attempt.failureKind ? ` · ${attempt.failureKind}` : ""}
                      {attempt.errorSummary ? ` · ${attempt.errorSummary}` : ""}
                    </div>
                  </div>
                ))}
                {attemptsQuery.data?.items.length === 0 && (
                  <div className="text-sm text-muted-foreground">暂无 attempt 历史。</div>
                )}
              </div>
            </section>
          </>
        )}
      </div>
    </Dialog>
  );
}
