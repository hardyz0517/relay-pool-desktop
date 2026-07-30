import { LayoutGrid, RefreshCw, Table2 } from "lucide-react";
import { Button, SegmentedControl, SelectControl } from "@/components/ui";
import type { ChannelStatusController } from "../useChannelStatusController";

const windowOptions = [
  { value: "recent", label: "最近" },
  { value: "last24h", label: "24h" },
  { value: "last7d", label: "7d" },
  { value: "last30d", label: "30d" },
] as const;

export type ChannelStatusViewMode = "table" | "cards";

const viewModeOptions = [
  { value: "table", label: "表格", icon: Table2 },
  { value: "cards", label: "卡片", icon: LayoutGrid },
] as const;

type ChannelStatusToolbarProps = {
  controller: ChannelStatusController;
  viewMode: ChannelStatusViewMode;
  onViewModeChange: (value: ChannelStatusViewMode) => void;
};

export function ChannelStatusToolbar({
  controller,
  viewMode,
  onViewModeChange,
}: ChannelStatusToolbarProps) {
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 shadow-[var(--surface-shadow)]">
      <div className="flex flex-wrap items-center gap-2">
        <SegmentedControl
          ariaLabel="监控窗口"
          value={controller.window}
          options={[...windowOptions]}
          onChange={controller.setWindow}
        />
        <SegmentedControl
          ariaLabel="状态监控视图"
          value={viewMode}
          options={[...viewModeOptions]}
          onChange={onViewModeChange}
        />
        <input
          value={controller.filters.search}
          onChange={(event) => controller.setSearch(event.target.value)}
          placeholder="搜索密钥 / 站点 / 监控"
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
            { value: "available", label: "正常" },
            { value: "degraded", label: "降级" },
            { value: "unavailable", label: "错误" },
            { value: "skipped", label: "跳过" },
            { value: "missing", label: "无数据" },
          ]}
          onChange={controller.setOutcome}
          className="min-w-[120px]"
        />
        <Button variant="secondary" disabled={controller.statusQuery.isFetching} onClick={() => void controller.refresh()}>
          <RefreshCw className="h-4 w-4" />
          刷新
        </Button>
      </div>
    </div>
  );
}
