import type { CSSProperties, ReactNode } from "react";
import { Gauge, Timer } from "lucide-react";
import { Sub2ApiPlatformIcon } from "@/components/group/Sub2ApiPlatformIcon";
import { EmptyState, StatusBadge } from "@/components/ui";
import { groupVisualClassNames } from "@/lib/groupVisualStyles";
import { cn } from "@/lib/utils";
import {
  availabilityHue,
  type ChannelStatusRowView,
  type StatusTone,
} from "../channelStatusViewModel";
import { StatusTrend } from "./StatusTrend";

type ChannelStatusCardGridProps = {
  rows: ChannelStatusRowView[];
  loading: boolean;
};

const badgeTone: Record<StatusTone, "healthy" | "warning" | "error" | "disabled" | "info"> = {
  available: "healthy",
  degraded: "warning",
  unavailable: "error",
  skipped: "info",
  missing: "disabled",
  running: "info",
  disabled: "disabled",
};

export function ChannelStatusCardGrid({
  rows,
  loading,
}: ChannelStatusCardGridProps) {
  if (rows.length === 0) {
    return (
      <EmptyState
        title={loading ? "正在读取状态监控" : "暂无状态监控卡片"}
        description="创建或启用监控后，卡片视图会显示每个密钥的当前状态、可用性、延迟和趋势。"
      />
    );
  }

  return (
    <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
      {rows.map((row) => (
        <ChannelStatusCard
          key={row.rowKey}
          row={row}
        />
      ))}
    </div>
  );
}

type ChannelStatusCardProps = {
  row: ChannelStatusRowView;
};

function ChannelStatusCard({ row }: ChannelStatusCardProps) {
  const availabilityHueValue = availabilityHue(row.availabilityPercent);
  const platformClassNames = groupVisualClassNames[row.visualPlatform];

  return (
    <article className="flex h-full flex-col rounded-[var(--surface-radius)] border border-border bg-surface p-3.5 shadow-[var(--surface-shadow)]">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2.5">
          <span
            className={cn(
              "flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px]",
              platformClassNames.rateBadge,
            )}
            title={`${row.visualPlatformLabel}${row.groupName ? ` · ${row.groupName}` : ""}`}
          >
            <Sub2ApiPlatformIcon
              platform={row.visualPlatform}
              className={cn("h-4 w-4", platformClassNames.icon)}
            />
          </span>
          <div className="min-w-0">
            <div className="truncate text-[15px] font-semibold leading-5 text-foreground" title={row.targetName}>
              {row.targetName}
            </div>
            <div className="mt-1 flex min-w-0 items-center gap-1.5">
              <span
                className="min-w-0 truncate text-xs text-muted-foreground"
                title={`${row.stationName} · ${row.modelLabel}`}
              >
                {row.stationName} · {row.modelLabel}
              </span>
            </div>
          </div>
        </div>
        <StatusBadge
          tone={badgeTone[row.currentTone]}
          className={cn(
            "shrink-0 border-0 px-2.5",
            row.currentTone === "available" && "bg-channel-health-surface text-channel-health-label",
          )}
        >
          {row.currentLabel}
        </StatusBadge>
      </div>

      <div className="mt-3 grid grid-cols-2 gap-2">
        <MetricTile icon={<Timer className="h-3.5 w-3.5" />} label="模型延迟" value={row.latencyLabel} />
        <MetricTile icon={<Gauge className="h-3.5 w-3.5" />} label="端点 Ping" value={row.endpointPingLabel} />
      </div>

      <div className="mt-3 border-t border-border pt-3">
        <div className="flex items-end justify-between gap-3">
          <div className="min-w-0 pb-0.5 text-xs font-medium text-muted-foreground">
            <div>可用性</div>
          </div>
          <div
            className={cn(
              "shrink-0 text-3xl font-semibold leading-8 tracking-normal",
              availabilityHueValue === null ? "text-muted-foreground" : "text-channel-availability",
            )}
            style={availabilityHueValue === null
              ? undefined
              : ({ "--channel-availability-hue": availabilityHueValue } as CSSProperties)}
          >
            {row.availabilityLabel}
          </div>
        </div>
      </div>

      <div className="mt-2.5 border-t border-border pt-2.5">
        <div className="mb-1.5 flex items-center justify-between gap-2 text-[11px] text-muted-foreground/70">
          <span>近 60 次记录</span>
          <span className="truncate" title={row.lastCheckedLabel}>最后检查 {row.lastCheckedLabel}</span>
        </div>
        <StatusTrend cells={row.recentTrend} compact variant="bars" slotCount={60} />
        <div className="mt-1 flex justify-between text-[10px] leading-3 text-muted-foreground/70">
          <span>过去</span>
          <span>现在</span>
        </div>
      </div>

    </article>
  );
}

function MetricTile({ icon, label, value }: { icon: ReactNode; label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-[8px] border border-border bg-surface-subtle px-3 py-2.5">
      <div className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground/70">
        {icon}
        <span className="truncate">{label}</span>
      </div>
      <div className="mt-2 truncate text-[18px] font-semibold leading-6 text-foreground" title={value}>
        {value}
      </div>
    </div>
  );
}
