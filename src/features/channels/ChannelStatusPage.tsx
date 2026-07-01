import { useEffect, useMemo, useState } from "react";
import { Radio, RefreshCw, Server, Timer } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, EmptyState, MetricCard, SegmentedControl, StatusBadge, useToast } from "@/components/ui";
import { listRequestLogs } from "@/lib/api/proxy";
import { listStationKeyHealth } from "@/lib/api/routing";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import type { RequestLog } from "@/lib/types/proxy";
import type { StationKeyHealth } from "@/lib/types/routing";
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
  successCount: number;
  failureCount: number;
  consecutiveFailures: number;
  cooldownUntil: string | null;
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
  const toast = useToast();
  const [keys, setKeys] = useState<KeyPoolItem[]>([]);
  const [logs, setLogs] = useState<RequestLog[]>([]);
  const [health, setHealth] = useState<StationKeyHealth[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  const channels = useMemo(() => buildChannels(keys, logs, health), [health, keys, logs]);

  async function refresh(showSuccess = false) {
    setLoading(true);
    setError(null);
    try {
      const [nextKeys, nextLogs, nextHealth] = await Promise.all([
        listKeyPoolItems(),
        listRequestLogs(),
        listStationKeyHealth(),
      ]);
      setKeys(nextKeys);
      setLogs(nextLogs);
      setHealth(nextHealth);
      if (showSuccess) {
        toast.success("渠道状态已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新渠道状态失败", message);
    } finally {
      setLoading(false);
    }
  }

  return (
    <PageScaffold
      title="渠道状态"
      description="监控 Key / Channel 的可用性、延迟、成功率和最近请求状态；健康状态来自本地代理真实请求。"
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
          <Button variant="secondary" onClick={() => void refresh(true)}>
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
  const cooldownActive = isFutureTime(channel.cooldownUntil);

  return (
    <section className="min-h-[248px] rounded-[var(--surface-radius)] border border-border bg-white p-4 shadow-[var(--surface-shadow)]">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <div className={cn("flex h-10 w-10 shrink-0 items-center justify-center rounded-[var(--surface-radius)] border border-border", iconTone(channel.status))}>
            <Server className="h-4 w-4" />
          </div>
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold text-slate-800">{channel.keyName}</div>
            <div className="mt-0.5 truncate text-xs text-muted-foreground">
              {channel.stationName} · {typeLabel} · {channel.modelSummary}
            </div>
          </div>
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1">
          <StatusBadge tone={statusTone[channel.status]}>{statusLabel[channel.status]}</StatusBadge>
          {cooldownActive && <StatusBadge tone="warning">冷却中</StatusBadge>}
        </div>
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

      <div className="mt-3 grid grid-cols-3 gap-2 text-xs">
        <div className="rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2">
          <div className="text-muted-foreground">成功</div>
          <div className="mt-1 font-semibold text-slate-800">{channel.successCount}</div>
        </div>
        <div className="rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2">
          <div className="text-muted-foreground">失败</div>
          <div className="mt-1 font-semibold text-slate-800">{channel.failureCount}</div>
        </div>
        <div className="rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2">
          <div className="text-muted-foreground">连续失败</div>
          <div className="mt-1 font-semibold text-slate-800">{channel.consecutiveFailures}</div>
        </div>
      </div>

      <div className="mt-4 rounded-[var(--surface-radius)] border border-border bg-slate-50 p-3">
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
        {cooldownActive ? `冷却至 ${formatCompactTime(channel.cooldownUntil)} · ` : ""}
        {channel.lastError ?? "暂无错误摘要"}
      </div>
    </section>
  );
}

function buildChannels(keys: KeyPoolItem[], logs: RequestLog[], health: StationKeyHealth[]): ChannelHealth[] {
  const healthByKey = new Map(health.map((item) => [item.stationKeyId, item] as const));
  return keys.map((key) => {
    const keyHealth = healthByKey.get(key.id);
    const keyLogs = logs
      .filter((log) => log.stationKeyId === key.id)
      .sort((a, b) => toTime(a.startedAt) - toTime(b.startedAt));
    const totalHealthRequests = (keyHealth?.successCount ?? 0) + (keyHealth?.failureCount ?? 0);
    const availabilityPercent =
      totalHealthRequests === 0 ? key.successRate === null ? null : key.successRate * 100 : ((keyHealth?.successCount ?? 0) / totalHealthRequests) * 100;
    const latencyMs = keyHealth?.avgLatencyMs ?? key.avgLatencyMs;
    const recentOutcomes = keyLogs.slice(-60).map(logToOutcome);
    const unknownOutcomes: RecentOutcome[] = Array.from({ length: 60 - recentOutcomes.length }, () => "unknown");
    const paddedOutcomes = unknownOutcomes.concat(recentOutcomes);
    const lastError =
      keyHealth?.lastErrorSummary ?? key.lastErrorSummary ?? [...keyLogs].reverse().find((log) => log.errorMessage)?.errorMessage ?? null;

    return {
      id: key.id,
      keyName: key.name,
      stationName: key.stationName,
      stationType: key.stationType,
      modelSummary: key.modelScopeSummary || key.groupName || key.tierLabel || "全部模型",
      status: key.enabled ? cooldownStatus(key.status, keyHealth?.cooldownUntil ?? key.cooldownUntil) : "disabled",
      latencyMs,
      availabilityPercent,
      lastCheckedAt: keyHealth?.updatedAt ?? key.lastCheckedAt,
      lastUsedAt: keyHealth?.lastSuccessAt ?? key.lastUsedAt,
      lastError,
      successCount: keyHealth?.successCount ?? 0,
      failureCount: keyHealth?.failureCount ?? 0,
      consecutiveFailures: keyHealth?.consecutiveFailures ?? key.consecutiveFailures,
      cooldownUntil: keyHealth?.cooldownUntil ?? key.cooldownUntil,
      recentOutcomes: paddedOutcomes,
    };
  });
}

function cooldownStatus(status: StationKeyStatus, cooldownUntil: string | null): StationKeyStatus {
  if (isFutureTime(cooldownUntil)) {
    return "warning";
  }
  return status;
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

function isFutureTime(value: string | null) {
  if (!value) {
    return false;
  }
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return !Number.isNaN(date.getTime()) && date.getTime() > Date.now();
}

function toTime(value: string) {
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return date.getTime();
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
