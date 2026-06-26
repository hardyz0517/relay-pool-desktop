import { Radio, RefreshCcw, Server, Timer } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, SegmentedControl, StatusBadge } from "@/components/ui";
import { mockChannelHealths, type MockChannelHealth } from "@/lib/mock";
import { stationStatusLabels, stationTypeLabels } from "@/lib/types/stations";
import { cn } from "@/lib/utils";

const statusTone: Record<MockChannelHealth["status"], "healthy" | "warning" | "error" | "disabled" | "info"> = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
  unchecked: "info",
};

const outcomeClassName: Record<"success" | "warning" | "failed" | "unknown", string> = {
  success: "bg-emerald-500/85",
  warning: "bg-amber-400/90",
  failed: "bg-rose-500/90",
  unknown: "bg-slate-300",
};

export function ChannelStatusPage() {
  return (
    <PageScaffold
      title="渠道状态"
      description="延迟、PING、可用率和近 60 次请求状态；仅展示健康信息。"
      actions={
        <div className="flex items-center gap-2">
          <SegmentedControl
            value="7d"
            options={[
              { value: "1h", label: "1 小时" },
              { value: "24h", label: "24 小时" },
              { value: "7d", label: "7 天" },
            ]}
          />
          <Button variant="secondary">
            <RefreshCcw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      }
    >
      <div className="grid gap-3 md:grid-cols-2 2xl:grid-cols-3">
        {mockChannelHealths.map((channel) => (
          <ChannelHealthCard key={channel.stationId} channel={channel} />
        ))}
      </div>
    </PageScaffold>
  );
}

function ChannelHealthCard({ channel }: { channel: MockChannelHealth }) {
  const statusLabel = stationStatusLabels[channel.status];
  const typeLabel = stationTypeLabels[channel.stationType as keyof typeof stationTypeLabels];

  return (
    <section className="min-h-[248px] rounded-[18px] border border-white/75 bg-white/95 p-4 shadow-[0_14px_34px_rgba(33,79,88,0.075)]">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <div className={cn("flex h-10 w-10 shrink-0 items-center justify-center rounded-2xl", iconTone(channel.status))}>
            <Server className="h-4 w-4" />
          </div>
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold text-slate-800">
              {channel.stationName}
            </div>
            <div className="mt-0.5 truncate text-xs text-muted-foreground">
              {typeLabel} · {channel.modelSummary}
            </div>
          </div>
        </div>
        <StatusBadge tone={statusTone[channel.status]}>{statusLabel}</StatusBadge>
      </div>

      <div className="mt-4 grid grid-cols-2 gap-2.5">
        <HealthMetric
          icon={Timer}
          label="对话延迟"
          value={channel.latencyMs === null ? "--" : `${channel.latencyMs}ms`}
        />
        <HealthMetric
          icon={Radio}
          label="站点 PING"
          value={channel.pingMs === null ? "--" : `${channel.pingMs}ms`}
        />
      </div>

      <div className="mt-4 rounded-2xl border border-cyan-100 bg-cyan-50/45 p-3">
        <div className="flex items-end justify-between gap-3">
          <div>
            <div className="text-[11px] text-muted-foreground">可用性 · 7 天</div>
            <div className="mt-0.5 text-3xl font-semibold tracking-normal text-slate-800">
              {channel.availabilityPercent.toFixed(1)}%
            </div>
          </div>
          <div className="pb-1 text-right text-[11px] text-muted-foreground">
            最近刷新
            <div className="mt-0.5 font-medium text-slate-600">{channel.lastCheckedAt}</div>
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
              key={`${channel.stationId}-${index}`}
              className={cn("h-7 rounded-[3px]", outcomeClassName[outcome])}
              title={outcome}
            />
          ))}
        </div>
      </div>

      <div className="mt-3 truncate text-xs text-muted-foreground">
        {channel.lastError}
      </div>
    </section>
  );
}

function HealthMetric({
  icon: Icon,
  label,
  value,
}: {
  icon: typeof Timer;
  label: string;
  value: string;
}) {
  return (
    <div className="rounded-2xl border border-cyan-100 bg-white/85 px-3 py-2.5">
      <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        <Icon className="h-3.5 w-3.5 text-teal-600" />
        {label}
      </div>
      <div className="mt-1 text-lg font-semibold leading-6 text-slate-800">{value}</div>
    </div>
  );
}

function iconTone(status: MockChannelHealth["status"]) {
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
