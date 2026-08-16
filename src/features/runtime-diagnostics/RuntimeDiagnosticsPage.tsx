import { useMemo, useState } from "react";
import { Download, RefreshCw } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, EmptyState, StatusBadge, useToast } from "@/components/ui";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { useQueryClient } from "@tanstack/react-query";
import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { RuntimeDiagnosticsPageDto, RuntimeDiagnosticsQueryDto, RuntimeEventDto } from "@/lib/bridge/generated";
import { runtimeDiagnosticsQueryOptions } from "./queries";

const levels = ["", "error", "warn", "info", "debug"] as const;
const components = ["", "app", "ipc", "persistence", "proxy", "outbound", "collector", "monitoring", "operation", "migration", "frontend", "runtime"] as const;

export function RuntimeDiagnosticsPage() {
  const toast = useToast();
  const queryClient = useQueryClient();
  const [segmentIndex, setSegmentIndex] = useState(0);
  const [lineIndex, setLineIndex] = useState(0);
  const [level, setLevel] = useState<RuntimeDiagnosticsQueryDto["level"]>();
  const [component, setComponent] = useState<RuntimeDiagnosticsQueryDto["component"]>();
  const [eventCode, setEventCode] = useState("");
  const [correlationId, setCorrelationId] = useState("");
  const [interactionId, setInteractionId] = useState("");
  const input = useMemo<RuntimeDiagnosticsQueryDto>(() => ({
    segmentIndex,
    lineIndex,
    level,
    component,
    eventCode: eventCode.trim() || undefined,
    correlationId: correlationId.trim() || undefined,
    interactionId: interactionId.trim() || undefined,
  }), [component, correlationId, eventCode, interactionId, level, lineIndex, segmentIndex]);
  const query = useActivityQuery(runtimeDiagnosticsQueryOptions(input));
  const page = query.data;
  const unsupported = !getActiveBackendClient().runtimeDiagnostics;

  async function exportBundle() {
    try {
      const result = await getActiveBackendClient().runtimeDiagnostics?.exportRuntimeSupportBundle();
      if (result) toast.success("诊断包已导出");
    } catch {
      toast.error("导出诊断包失败", "请确认开发者模式仍处于开启状态");
    }
  }

  function updateFilter(update: () => void) {
    setSegmentIndex(0);
    setLineIndex(0);
    update();
  }

  if (unsupported) {
    return <PageScaffold title="运行诊断"><EmptyState title="当前运行模式不支持诊断" description="请在桌面开发者模式中使用运行诊断。" /></PageScaffold>;
  }

  return (
    <PageScaffold
      title="运行诊断"
      description="仅显示经过安全筛选的结构化运行事件"
      actions={<>
        <Button variant="outline" onClick={() => void queryClient.invalidateQueries({ queryKey: ["runtimeDiagnostics"] })}>
          <RefreshCw className="h-4 w-4" />刷新
        </Button>
        <Button variant="secondary" onClick={() => void exportBundle()}>
          <Download className="h-4 w-4" />导出诊断包
        </Button>
      </>}
    >
      <div className="grid min-h-0 gap-3">
        <div className="flex flex-wrap items-end gap-2 rounded-[var(--surface-radius)] border border-border bg-surface p-3">
          <label className="grid gap-1 text-xs text-muted-foreground">级别<select aria-label="日志级别" className="h-8 rounded border border-border bg-background px-2 text-sm text-foreground" value={level ?? ""} onChange={(event) => updateFilter(() => setLevel((event.target.value || undefined) as RuntimeDiagnosticsQueryDto["level"]))}>{levels.map((item) => <option key={item} value={item}>{item || "全部"}</option>)}</select></label>
          <label className="grid gap-1 text-xs text-muted-foreground">组件<select aria-label="日志组件" className="h-8 rounded border border-border bg-background px-2 text-sm text-foreground" value={component ?? ""} onChange={(event) => updateFilter(() => setComponent((event.target.value || undefined) as RuntimeDiagnosticsQueryDto["component"]))}>{components.map((item) => <option key={item} value={item}>{item || "全部"}</option>)}</select></label>
          <FilterInput label="事件代码" value={eventCode} onChange={(value) => updateFilter(() => setEventCode(value))} />
          <FilterInput label="Correlation ID" value={correlationId} onChange={(value) => updateFilter(() => setCorrelationId(value))} />
          <FilterInput label="Interaction ID" value={interactionId} onChange={(value) => updateFilter(() => setInteractionId(value))} />
        </div>
        {query.isError && <div role="alert" className="border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">运行诊断暂时不可用，请稍后重试。</div>}
        {page?.sinkDegraded && <div className="border border-warning-border bg-warning-surface px-3 py-2 text-sm text-warning-foreground">日志写入处于降级状态，部分事件可能缺失。</div>}
        {page && <RuntimeHealthSummary page={page} />}
        {query.isPending && !page ? <EmptyState title="正在读取运行诊断" /> : page && page.events.length === 0 ? <EmptyState title="没有匹配的运行事件" description={page.issueCount ? "部分日志片段无法安全解析，已被隔离。" : "调整筛选条件后重试。"} /> : page && <RuntimeEventTable events={page.events} />}
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          <span>{page ? `${page.events.length} 条事件 · ${page.issueCount} 个隔离项` : ""}</span>
          <Button size="sm" variant="outline" disabled={page?.nextSegmentIndex == null} onClick={() => { if (page?.nextSegmentIndex == null) return; setSegmentIndex(page.nextSegmentIndex); setLineIndex(page.nextLineIndex ?? 0); }}>更早事件</Button>
        </div>
      </div>
    </PageScaffold>
  );
}

function RuntimeHealthSummary({ page }: { page: RuntimeDiagnosticsPageDto }) {
  return <div className="grid grid-cols-2 gap-2 text-xs sm:grid-cols-4" aria-label="日志健康摘要">
    <SummaryItem label="写入器" value={page.sinkDegraded ? "降级" : "正常"} tone={page.sinkDegraded ? "text-warning-foreground" : "text-success-foreground"} />
    <SummaryItem label="丢弃 / 拒绝" value={`${page.droppedCount} / ${page.rejectedCount}`} />
    <SummaryItem label="恢复片段" value={`${page.recoveryRecovered}/${page.recoveryExamined}（跳过 ${page.recoverySkipped}）`} />
    <SummaryItem label="保留清理" value={`${page.retentionDeleted}/${page.retentionConsidered}（未知 ${page.retentionSkippedUnknown}）`} />
    <SummaryItem label="时钟年龄清理" value={page.clockStable ? "启用" : "暂停"} tone={page.clockStable ? undefined : "text-warning-foreground"} />
    <SummaryItem label="清理失败" value={String(page.retentionDeleteFailures)} tone={page.retentionDeleteFailures ? "text-warning-foreground" : undefined} />
    {page.lastSinkErrorCode ? <SummaryItem label="最近 sink 错误" value={page.lastSinkErrorCode} mono /> : null}
  </div>;
}

function SummaryItem({ label, value, tone, mono }: { label: string; value: string; tone?: string; mono?: boolean }) {
  return <div className="min-w-0 rounded border border-border bg-surface px-3 py-2"><div className="text-muted-foreground">{label}</div><div className={`truncate font-medium ${tone ?? "text-foreground"} ${mono ? "font-mono" : ""}`} title={value}>{value}</div></div>;
}

function FilterInput({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return <label className="grid min-w-40 flex-1 gap-1 text-xs text-muted-foreground">{label}<input aria-label={label} className="h-8 rounded border border-border bg-background px-2 text-sm text-foreground" value={value} maxLength={96} onChange={(event) => onChange(event.target.value)} /></label>;
}

function RuntimeEventTable({ events }: { events: RuntimeEventDto[] }) {
  return <div className="max-h-[calc(100dvh-240px)] min-h-0 overflow-auto rounded-[var(--surface-radius)] border border-border bg-surface" tabIndex={0} aria-label="运行事件列表"><table className="w-full min-w-[900px] border-collapse text-left text-xs"><thead className="sticky top-0 bg-surface-subtle text-muted-foreground"><tr><th className="px-3 py-2">时间</th><th className="px-3 py-2">级别</th><th className="px-3 py-2">组件</th><th className="px-3 py-2">消息键</th><th className="px-3 py-2">事件代码</th><th className="px-3 py-2">结果</th><th className="px-3 py-2">耗时</th></tr></thead><tbody>{events.map((event) => { const level = event.level ?? "unknown"; const tone = level === "error" ? "error" : level === "warn" ? "warning" : "info"; const source = event.manifestSource === "previous" ? "上一 manifest" : "当前 manifest"; return <tr key={`${event.sessionId}-${event.sequence}`} className="border-t border-border"><td className="whitespace-nowrap px-3 py-2 text-muted-foreground">{new Date(event.atMs).toLocaleString()}</td><td className="px-3 py-2"><StatusBadge tone={tone}>{level}</StatusBadge></td><td className="px-3 py-2">{event.component}</td><td className="px-3 py-2 font-mono"><span>{event.messageKey}</span><span className="ml-2 text-muted-foreground" title="事件来自已校验的 manifest 快照">{source}</span></td><td className="px-3 py-2 font-mono"><span>{event.eventCode}</span>{event.deprecatedReplacedBy ? <span className="ml-2 text-muted-foreground" title="已废弃事件的替代代码">→ {event.deprecatedReplacedBy}</span> : null}</td><td className="px-3 py-2">{event.outcome}</td><td className="px-3 py-2">{event.durationMs == null ? "-" : `${event.durationMs} ms`}</td></tr>; })}</tbody></table></div>;
}
