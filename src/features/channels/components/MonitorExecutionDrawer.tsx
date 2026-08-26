import { useEffect, useRef, useState } from "react";
import { Dialog, StatusBadge, type StatusTone } from "@/components/ui";
import { profileLabel, protocolLabel } from "@/lib/channelMonitorDisplay";
import {
  channelMonitorAttemptsQueryOptions,
  channelMonitorExecutionQueryOptions,
} from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type {
  ChannelMonitorAttemptRecord,
  ChannelMonitorExecutionSummaryV2,
  ChannelMonitorTargetResultRecord,
} from "@/lib/types/channelMonitors";
import { cn } from "@/lib/utils";
import { formatLatency, formatTime } from "../channelStatusViewModel";

type MonitorExecutionDrawerProps = {
  executionId: string | null;
  onClose: () => void;
};

export function MonitorExecutionDrawer({ executionId, onClose }: MonitorExecutionDrawerProps) {
  const [activeExecutionId, setActiveExecutionId] = useState<string | null>(executionId);
  const clearActiveTimerRef = useRef<number | null>(null);

  useEffect(() => () => {
    if (clearActiveTimerRef.current !== null) {
      window.clearTimeout(clearActiveTimerRef.current);
    }
  }, []);

  useEffect(() => {
    if (clearActiveTimerRef.current !== null) {
      window.clearTimeout(clearActiveTimerRef.current);
      clearActiveTimerRef.current = null;
    }

    if (executionId) {
      setActiveExecutionId(executionId);
      return;
    }

    clearActiveTimerRef.current = window.setTimeout(() => {
      setActiveExecutionId(null);
      clearActiveTimerRef.current = null;
    }, 220);
  }, [executionId]);

  const detailQuery = useActivityQuery(channelMonitorExecutionQueryOptions(activeExecutionId));
  const attemptsQuery = useActivityQuery(
    channelMonitorAttemptsQueryOptions({ executionId: activeExecutionId ?? "", limit: 100 }),
  );
  const detail = detailQuery.data;
  const attempts = attemptsQuery.data?.items ?? [];

  return (
    <Dialog
      open={Boolean(executionId)}
      title="监控执行详情"
      description={activeExecutionId ?? undefined}
      onClose={onClose}
      className="max-w-[980px]"
    >
      <div className="space-y-4 p-5">
        {!detail ? (
          <div className="rounded-[var(--surface-radius)] border border-border bg-surface-subtle px-3 py-2 text-sm text-muted-foreground">
            {detailQuery.isPending ? "正在加载执行详情..." : "暂无执行详情"}
          </div>
        ) : (
          <>
            <ExecutionSummary execution={detail.execution} />
            <TargetResultsTable targets={detail.targets} />
            <AttemptList attempts={attempts} loading={attemptsQuery.isPending} />
          </>
        )}
      </div>
    </Dialog>
  );
}

function ExecutionSummary({ execution }: { execution: ChannelMonitorExecutionSummaryV2 }) {
  return (
    <section className="rounded-[var(--surface-radius)] border border-border bg-surface p-4 shadow-[var(--surface-shadow)]">
      <div className="mb-3 flex min-w-0 items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold text-foreground" title={execution.monitorId}>
            监控任务 {shortId(execution.monitorId)}
          </div>
          <div className="mt-1 truncate text-xs text-muted-foreground" title={execution.executionId}>
            执行 ID：{execution.executionId}
          </div>
        </div>
        <StatusBadge tone={statusTone(execution.status)} className="shrink-0">
          {executionStatusLabel(execution.status)}
        </StatusBadge>
      </div>

      <div className="grid gap-2 md:grid-cols-4">
        <SummaryTile label="触发方式" value={triggerKindLabel(execution.triggerKind)} detail={execution.triggerRequestId ? shortId(execution.triggerRequestId) : "无请求 ID"} />
        <SummaryTile label="计划时间" value={formatTime(execution.plannedAtMs)} detail={`开始 ${formatTime(execution.startedAtMs)}`} />
        <SummaryTile label="结束时间" value={formatTime(execution.finishedAtMs)} detail={execution.finishedAtMs ? "已结束" : "执行中"} />
        <SummaryTile
          label="执行结果"
          value={outcomeLabel(execution.summaryOutcome)}
          detail={failureKindLabel(execution.summaryFailureKind)}
          tone={outcomeTone(execution.summaryOutcome)}
        />
      </div>

      <div className="mt-3 grid gap-2 rounded-[8px] border border-border bg-surface-subtle p-2 text-xs md:grid-cols-5">
        <CountPill label="目标" value={execution.targetCount} />
        <CountPill label="正常" value={execution.availableCount} tone="healthy" />
        <CountPill label="降级" value={execution.degradedCount} tone="warning" />
        <CountPill label="错误" value={execution.unavailableCount} tone="error" />
        <CountPill label="跳过" value={execution.skippedCount} tone="disabled" />
      </div>
    </section>
  );
}

function TargetResultsTable({ targets }: { targets: ChannelMonitorTargetResultRecord[] }) {
  return (
    <section>
      <div className="mb-2 text-sm font-medium text-foreground">目标结果</div>
      <div className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface">
        <div className="overflow-x-auto">
          <table className="w-full min-w-[860px] table-fixed border-collapse text-left text-xs">
            <colgroup>
              <col className="w-[15%]" />
              <col className="w-[16%]" />
              <col className="w-[20%]" />
              <col className="w-[17%]" />
              <col className="w-[8%]" />
              <col className="w-[16%]" />
              <col className="w-[8%]" />
            </colgroup>
            <thead className="border-b border-border bg-surface text-muted-foreground">
              <tr>
                <TableHead>密钥</TableHead>
                <TableHead>结果</TableHead>
                <TableHead>模型</TableHead>
                <TableHead>请求档案</TableHead>
                <TableHead>尝试</TableHead>
                <TableHead>健康写回</TableHead>
                <TableHead className="text-right">延迟</TableHead>
              </tr>
            </thead>
            <tbody>
              {targets.map((target) => (
                <tr key={target.targetResultId} className="border-b border-border last:border-b-0">
                  <TableCell>
                    <span className="block truncate" title={target.stationKeyId ?? target.stationId}>
                      {shortId(target.stationKeyId ?? target.stationId)}
                    </span>
                  </TableCell>
                  <TableCell>
                    <div className="flex min-w-0 items-center gap-2">
                      <StatusBadge tone={target.availabilityEligible ? outcomeTone(target.terminalOutcome) : "disabled"} className="h-5 px-1.5">
                        {target.availabilityEligible ? outcomeLabel(target.terminalOutcome) : "已排除"}
                      </StatusBadge>
                      <span className="min-w-0 truncate text-muted-foreground" title={target.exclusionReason ?? target.terminalFailureKind ?? undefined}>
                        {target.availabilityEligible ? failureKindLabel(target.terminalFailureKind) : exclusionReasonLabel(target.exclusionReason)}
                      </span>
                    </div>
                  </TableCell>
                  <TableCell>
                    <div className="truncate text-foreground" title={modelTrace(target)}>
                      {modelTrace(target)}
                    </div>
                  </TableCell>
                  <TableCell>
                    <div className="truncate" title={`${profileLabel(target.clientProfileId)} v${target.clientProfileVersion}`}>
                      {profileLabel(target.clientProfileId)} v{target.clientProfileVersion}
                    </div>
                  </TableCell>
                  <TableCell>{target.attemptCount} 次</TableCell>
                  <TableCell>
                    <div className="truncate" title={healthWritebackText(target)}>
                      {healthWritebackText(target)}
                    </div>
                  </TableCell>
                  <TableCell className="text-right font-medium">{formatLatency(target.latencyMs)}</TableCell>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}

function AttemptList({ attempts, loading }: { attempts: ChannelMonitorAttemptRecord[]; loading: boolean }) {
  return (
    <section>
      <div className="mb-2 text-sm font-medium text-foreground">请求尝试</div>
      <div className="space-y-2">
        {attempts.map((attempt) => (
          <div key={attempt.attemptId} className="rounded-[var(--surface-radius)] border border-border bg-surface-subtle px-3 py-2.5 text-xs">
            <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
              <div className="min-w-0 truncate font-medium text-foreground" title={attempt.model}>
                #{attempt.attemptNumber} · {modelRoleLabel(attempt.modelRole)} · {attempt.model}
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <StatusBadge tone={outcomeTone(attempt.outcome)} className="h-5 px-1.5">
                  {outcomeLabel(attempt.outcome)}
                </StatusBadge>
                <span className="text-muted-foreground">HTTP {attempt.httpStatus ?? "--"}</span>
                <span className="font-medium text-foreground">{formatLatency(attempt.latencyMs)}</span>
              </div>
            </div>
            <div className="mt-1 flex min-w-0 flex-wrap gap-x-3 gap-y-1 text-muted-foreground">
              <span className="truncate">{protocolLabel(attempt.protocolKind)}</span>
              <span className="truncate">{profileLabel(attempt.clientProfileId)} v{attempt.clientProfileVersion}</span>
              <span>{transportModeLabel(attempt.transportMode)}</span>
              {attempt.failureKind && <span>{failureKindLabel(attempt.failureKind)}</span>}
              {attempt.errorSummary && <span className="min-w-0 truncate" title={attempt.errorSummary}>{attempt.errorSummary}</span>}
            </div>
          </div>
        ))}
        {attempts.length === 0 && (
          <div className="rounded-[var(--surface-radius)] border border-border bg-surface-subtle px-3 py-2 text-sm text-muted-foreground">
            {loading ? "正在加载请求尝试..." : "暂无请求尝试记录"}
          </div>
        )}
      </div>
    </section>
  );
}

function SummaryTile({
  label,
  value,
  detail,
  tone,
}: {
  label: string;
  value: string;
  detail: string;
  tone?: StatusTone;
}) {
  return (
    <div className="min-w-0 rounded-[8px] border border-border bg-surface-subtle px-3 py-2">
      <div className="text-[11px] font-medium text-muted-foreground">{label}</div>
      <div className={cn("mt-1 truncate text-sm font-semibold text-foreground", tone && summaryToneClassName(tone))} title={value}>
        {value}
      </div>
      <div className="mt-0.5 truncate text-[11px] text-muted-foreground" title={detail}>
        {detail}
      </div>
    </div>
  );
}

function CountPill({ label, value, tone = "info" }: { label: string; value: number; tone?: StatusTone }) {
  return (
    <div className={cn("flex items-center justify-between gap-2 rounded-[7px] px-2 py-1", countToneClassName(tone))}>
      <span>{label}</span>
      <span className="font-semibold">{value}</span>
    </div>
  );
}

function TableHead({ children, className }: { children: string; className?: string }) {
  return <th className={cn("h-8 whitespace-nowrap px-3 font-medium", className)}>{children}</th>;
}

function TableCell({ children, className }: { children: React.ReactNode; className?: string }) {
  return <td className={cn("px-3 py-2 align-middle text-foreground", className)}>{children}</td>;
}

function modelTrace(target: ChannelMonitorTargetResultRecord) {
  if (!target.effectiveModel || target.effectiveModel === target.requestedModel) {
    return target.requestedModel;
  }
  return `${target.requestedModel} → ${target.effectiveModel}`;
}

function healthWritebackText(target: ChannelMonitorTargetResultRecord) {
  const reason = target.healthWritebackReason ? ` · ${healthWritebackReasonLabel(target.healthWritebackReason)}` : "";
  return `${healthWritebackDecisionLabel(target.healthWritebackDecision)}${reason}`;
}

function exclusionReasonLabel(value: string | null) {
  const labels: Record<string, string> = {
    balance_depleted: "余额耗尽，不计入技术统计",
    subscription_unavailable: "订阅不可用，不计入技术统计",
    quota_exhausted: "配额耗尽，不计入技术统计",
    cancelled: "已取消，不计入技术统计",
    interrupted: "已中断，不计入技术统计",
    local_configuration: "本地配置，不计入技术统计",
    local_budget: "本地预算，不计入技术统计",
    local_internal_before_send: "发送前本地中断，不计入技术统计",
  };
  return value ? labels[value] ?? `${value}，不计入技术统计` : "业务原因，不计入技术统计";
}

function shortId(value: string) {
  if (value.length <= 16) return value;
  return `${value.slice(0, 8)}…${value.slice(-6)}`;
}

function executionStatusLabel(value: string | null | undefined) {
  const labels: Record<string, string> = {
    completed: "已完成",
    running: "运行中",
    queued: "排队中",
    partial: "部分完成",
    skipped: "已跳过",
    cancelled: "已取消",
    interrupted: "已中断",
    failed: "失败",
  };
  return value ? labels[value] ?? value : "--";
}

function triggerKindLabel(value: string | null | undefined) {
  const labels: Record<string, string> = {
    scheduled: "定时执行",
    manual: "手动执行",
    startup_recovery: "启动恢复",
  };
  return value ? labels[value] ?? value : "--";
}

function outcomeLabel(value: string | null | undefined) {
  const labels: Record<string, string> = {
    available: "正常",
    degraded: "降级",
    unavailable: "错误",
    skipped: "跳过",
    missing: "无数据",
  };
  return value ? labels[value] ?? value : "--";
}

function failureKindLabel(value: string | null | undefined) {
  if (!value) return "无异常";
  const labels: Record<string, string> = {
    auth: "鉴权失败",
    network: "网络错误",
    timeout: "请求超时",
    empty_response: "回复正文为空",
    content_mismatch: "答案校验失败",
    protocol_mismatch: "协议不兼容",
    rate_limited: "触发限流",
    server_error: "上游错误",
    invalid_response: "响应异常",
    semantic_mismatch: "语义不匹配",
    budget_exceeded: "预算用尽",
    cancelled: "已取消",
    internal: "内部错误",
    needs_configuration: "配置缺失",
    fallback_used: "使用备用模型",
  };
  return labels[value] ?? value;
}

function modelRoleLabel(value: string | null | undefined) {
  const labels: Record<string, string> = {
    primary: "主模型",
    fallback: "备用模型",
  };
  return value ? labels[value] ?? value : "--";
}

function transportModeLabel(value: string | null | undefined) {
  const labels: Record<string, string> = {
    warm: "预热请求",
    cold: "冷启动请求",
    stream: "流式请求",
    non_stream: "非流式请求",
  };
  return value ? labels[value] ?? value : "--";
}

function healthWritebackDecisionLabel(value: string | null | undefined) {
  const labels: Record<string, string> = {
    disabled: "不写回",
    observe_only: "仅观察",
    skipped: "跳过写回",
    no_change: "无需变更",
    mark_healthy: "标记健康",
    mark_unhealthy: "标记异常",
    authoritative: "权威写回",
  };
  return value ? labels[value] ?? value : "--";
}

function healthWritebackReasonLabel(value: string) {
  const labels: Record<string, string> = {
    no_transition: "状态无变化",
    threshold_not_met: "未达到阈值",
    unavailable: "错误",
    degraded: "降级",
    available: "正常",
  };
  return labels[value] ?? value;
}

function statusTone(value: string | null | undefined): StatusTone {
  if (value === "completed") return "healthy";
  if (value === "failed" || value === "interrupted") return "error";
  if (value === "partial" || value === "cancelled") return "warning";
  return "info";
}

function outcomeTone(value: string | null | undefined): StatusTone {
  if (value === "available") return "healthy";
  if (value === "degraded") return "warning";
  if (value === "unavailable") return "error";
  if (value === "skipped" || value === "missing") return "disabled";
  return "info";
}

function summaryToneClassName(tone: StatusTone) {
  if (tone === "healthy") return "text-success-foreground";
  if (tone === "warning") return "text-warning-foreground";
  if (tone === "error") return "text-danger-foreground";
  if (tone === "disabled") return "text-muted-foreground";
  return "text-info-foreground";
}

function countToneClassName(tone: StatusTone) {
  if (tone === "healthy") return "bg-success-surface text-success-foreground";
  if (tone === "warning") return "bg-warning-surface text-warning-foreground";
  if (tone === "error") return "bg-danger-surface text-danger-foreground";
  if (tone === "disabled") return "bg-muted text-muted-foreground";
  return "bg-info-surface text-info-foreground";
}
