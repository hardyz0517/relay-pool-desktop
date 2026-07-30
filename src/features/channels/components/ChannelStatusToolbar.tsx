import { Plus, RefreshCw } from "lucide-react";
import { Button, SegmentedControl, SelectControl } from "@/components/ui";
import type { MonitoringCapabilityCatalog } from "@/lib/types/channelMonitors";
import type { ChannelStatusController } from "../useChannelStatusController";
import { MonitorProfileSelector } from "./MonitorProfileSelector";

const windowOptions = [
  { value: "recent", label: "最近" },
  { value: "last24h", label: "24h" },
  { value: "last7d", label: "7d" },
  { value: "last30d", label: "30d" },
] as const;

type ChannelStatusToolbarProps = {
  controller: ChannelStatusController;
  capabilities: MonitoringCapabilityCatalog | undefined;
  onCreateMonitor: () => void;
};

export function ChannelStatusToolbar({
  controller,
  capabilities,
  onCreateMonitor,
}: ChannelStatusToolbarProps) {
  const protocolOptions = [
    { value: "", label: "全部协议" },
    ...(capabilities?.protocols ?? []).map((protocol) => ({
      value: protocol.id,
      label: protocol.id,
      disabled: !protocol.enabled,
    })),
  ];

  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 shadow-[var(--surface-shadow)]">
      <div className="flex flex-wrap items-center gap-2">
        <SegmentedControl
          ariaLabel="监控窗口"
          value={controller.window}
          options={[...windowOptions]}
          onChange={controller.setWindow}
        />
        <input
          value={controller.filters.search}
          onChange={(event) => controller.setSearch(event.target.value)}
          placeholder="搜索 Key / Station / Monitor"
          className="h-8 min-w-[220px] flex-1 rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-sm outline-none transition focus:border-ring/40 focus:ring-2 focus:ring-ring/20"
        />
        <SelectControl
          ariaLabel="启用状态"
          value={controller.filters.enabled}
          options={[
            { value: "all", label: "全部" },
            { value: "enabled", label: "已启用" },
            { value: "disabled", label: "已停用" },
          ]}
          onChange={controller.setEnabled}
          className="min-w-[104px]"
        />
        <SelectControl
          ariaLabel="当前状态"
          value={controller.filters.outcome}
          options={[
            { value: "all", label: "全部状态" },
            { value: "available", label: "可用" },
            { value: "degraded", label: "降级" },
            { value: "unavailable", label: "不可用" },
            { value: "skipped", label: "跳过" },
            { value: "missing", label: "无数据" },
          ]}
          onChange={controller.setOutcome}
          className="min-w-[120px]"
        />
        <SelectControl
          ariaLabel="协议筛选"
          value={controller.filters.protocolKind}
          options={protocolOptions}
          onChange={controller.setProtocolKind}
          className="min-w-[140px]"
        />
        <MonitorProfileSelector
          value={controller.filters.clientProfileId}
          capabilities={capabilities}
          onChange={controller.setClientProfileId}
        />
        <Button variant="secondary" disabled={controller.statusQuery.isFetching} onClick={() => void controller.refresh()}>
          <RefreshCw className="h-4 w-4" />
          刷新
        </Button>
        <Button onClick={onCreateMonitor}>
          <Plus className="h-4 w-4" />
          新建
        </Button>
      </div>
    </div>
  );
}
