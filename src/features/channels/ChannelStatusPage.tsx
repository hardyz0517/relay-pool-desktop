import { useEffect, useMemo, useState } from "react";
import { Radio, RefreshCw, Server, Timer } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, EmptyState, MetricCard, SegmentedControl, StatusBadge } from "@/components/ui";
import { listRequestLogs } from "@/lib/api/proxy";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import type { RequestLog } from "@/lib/types/proxy";
import type { KeyPoolItem, StationKeyStatus } from "@/lib/types/stationKeys";
import { stationTypeLabels } from "@/lib/types/stations";
import { cn } from "@/lib/utils";

type RecentOutcome = "success" | "warning" | "failed" | "unknown";

type ChannelHealth = {
  id: string;
  keyName: string;
  stationName: string;
  stationType: string;
  modelSummary: string;
  status: StationKeyStatus;
  latencyMs: number | null;
  availabilityPercent: number | null;
  lastCheckedAt: string | null;
  lastUsedAt: string | null;
  lastError: string | null;
  recentOutcomes: RecentOutcome[];
};

const statusTone: Record<StationKeyStatus, "healthy" | "warning" | "error" | "disabled" | "info"> = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
  unchecked: "info",
};

const statusLabel: Record<StationKeyStatus, string> = {
  healthy: "健康",
  warning: "警告",
  error: "错误",
  disabled: "禁用",
  unchecked: "未检查",
};

const outcomeClassName: Record<RecentOutcome, string> = {
  success: "bg-emerald-500/85",
  warning: "bg-amber-400/90",
  failed: "bg-rose-500/90",
  unknown: "bg-slate-300",
};

export function ChannelStatusPage() {
  const [keys, setKeys] = useState<KeyPoolItem[]>([]);
  const [logs, setLogs] = useState<RequestLog[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  const channels = useMemo(() => buildChannels(keys, logs), [keys, logs]);

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      const [nextKeys, nextLogs] = await Promise.all([listKeyPoolItems(), listRequestLogs()]);
      setKeys(nextKeys);
      setLogs(nextLogs);
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setLoading(false);
    }
  }

  return (
    <PageScaffold
      title="渠道状态"
      description="监控 Key / Channel 的可用性、耗时和最近请求状态；当前统计来自本地代理请求日志。"
      actions={
        <div className="flex items-center gap-2">
          <SegmentedControl
            value="recent"
            options={[
              { value: "recent", label: "最近请求" },
              { value: "24h", label: "24 小时" },
              { value: "7d", label: "7 天" },
            ]}
          />
          <Button variant="secondary" onClick={() => void refresh()}>
            <RefreshCw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      }
    >
      {error && <div className="mb-3 rounded-xl border border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">{error}</div>}
      {channels.length === 0 ? (
        <EmptyState
          title={loading ? "正在读取渠道状态" : "暂无可展示的 Key"}
          description="在 Key 池添加并启用 Station Key 后，本地代理请求会在这里形成真实状态。"
        />
      ) : (
        <div className="grid gap-3 md:grid-cols-2 2xl:grid-cols-3">
          {channels.map((channel) => (
            <ChannelHealthCard key={channel.id} channel={channel} />
          ))}
        </div>
      )}
    </PageScaffold>
  );
}

function ChannelHealthCard({ channel }: { channel: ChannelHealth }) {
  const typeLabel = stationTypeLabels[channel.stationType as keyof typeof stationTypeLabels] ?? channel.stationType;

  return (
    <section className="min-h-[248px] rounded-[var(--surface-radius)] border border-border bg-white p-4 shadow-[var(--surface-shadow)]">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <div className={cn("flex h-10 w-10 shrink-0 items-center justify-center rounded-[12px]", iconTone(channel.status))}>
            <Server className="h-4 w-4" />
          </div>
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold text-slate-800">{channel.keyName}</div>
            <div className="mt-0.5 truncate text-xs text-muted-foreground">
              {channel.stationName} · {typeLabel} · {channel.modelSummary}
            </div>
          </div>
        </div>
        <StatusBadge tone={statusTone[channel.status]}>{statusLabel[channel.status]}</StatusBadge>
      </div>

      <div className="mt-4 grid grid-cols-2 gap-2.5">
        <MetricCard
          icon={Timer}
          label="平均耗时"
          value={channel.latencyMs === null ? "--" : `${channel.latencyMs}ms`}
        />
        <MetricCard
          icon={Radio}
          label="最近使用"
          value={formatCompactTime(channel.lastUsedAt)}
        />
      </div>

      <div className="mt-4 rounded-[var(--surface-radius)] border border-cyan-100 bg-cyan-50/45 p-3">
        <div className="flex items-end justify-between gap-3">
          <div>
            <div className="text-[11px] text-muted-foreground">可用性 · 最近日志</div>
            <div className="mt-0.5 text-3xl font-semibold tracking-normal text-slate-800">
              {channel.availabilityPercent === null ? "--" : `${channel.availabilityPercent.toFixed(1)}%`}
            </div>
          </div>
          <div className="pb-1 text-right text-[11px] text-muted-foreground">
            最近检查
            <div className="mt-0.5 font-medium text-slate-600">{formatCompactTime(channel.lastCheckedAt)}</div>
          </div>
        </div>
      </div>

      <div className="mt-4">
        <div className="mb-1.5 flex items-center justify-between text-[11px] text-muted-foreground">
          <span>PAST</span>
          <span>近 60 次请求</span>
          <span>NOW</span>
        </div>
        <div className="grid grid-cols-[repeat(60,minmax(0,1fr))] gap-[3px]">
          {channel.recentOutcomes.map((outcome, index) => (
            <span
              key={`${channel.id}-${index}`}
              className={cn("h-7 rounded-[3px]", outcomeClassName[outcome])}
              title={outcome}
            />
          ))}
        </div>
      </div>

      <div className="mt-3 truncate text-xs text-muted-foreground">
        {channel.lastError ?? "暂无错误摘要"}
      </div>
    </section>
  );
}

function buildChannels(keys: KeyPoolItem[], logs: RequestLog[]): ChannelHealth[] {
  return keys.map((key) => {
    const keyLogs = logs
      .filter((log) => log.stationKeyId === key.id)
      .sort((a, b) => toTime(a.startedAt) - toTime(b.startedAt));
    const completedLogs = keyLogs.filter((log) => log.durationMs !== null);
    const successLogs = keyLogs.filter((log) => log.status === "success");
    const availabilityPercent = keyLogs.length === 0 ? null : (successLogs.length / keyLogs.length) * 100;
    const latencyMs =
      completedLogs.length === 0
        ? null
        : Math.round(completedLogs.reduce((sum, log) => sum + (log.durationMs ?? 0), 0) / completedLogs.length);
    const recentOutcomes = keyLogs.slice(-60).map(logToOutcome);
    const unknownOutcomes: RecentOutcome[] = Array.from({ length: 60 - recentOutcomes.length }, () => "unknown");
    const paddedOutcomes = unknownOutcomes.concat(recentOutcomes);
    const lastError = [...keyLogs].reverse().find((log) => log.errorMessage)?.errorMessage ?? null;

    return {
      id: key.id,
      keyName: key.name,
      stationName: key.stationName,
      stationType: key.stationType,
      modelSummary: key.groupName ?? key.tierLabel ?? "全部模型",
      status: key.enabled ? key.status : "disabled",
      latencyMs,
      availabilityPercent,
      lastCheckedAt: key.lastCheckedAt,
      lastUsedAt: key.lastUsedAt,
      lastError,
      recentOutcomes: paddedOutcomes,
    };
  });
}

function logToOutcome(log: RequestLog): RecentOutcome {
  if (log.status === "success") {
    return "success";
  }
  if (log.status === "fallback" || log.fallbackCount > 0) {
    return "warning";
  }
  if (log.status === "failed") {
    return "failed";
  }
  return "unknown";
}

function iconTone(status: StationKeyStatus) {
  if (status === "healthy") {
    return "bg-emerald-100 text-emerald-700";
  }
  if (status === "warning") {
    return "bg-amber-100 text-amber-700";
  }
  if (status === "error") {
    return "bg-rose-100 text-rose-700";
  }
  return "bg-slate-100 text-slate-500";
}

function formatCompactTime(value: string | null) {
  if (!value) {
    return "--";
  }
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function toTime(value: string) {
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return date.getTime();
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
