import { Check, ChevronDown, LayoutGrid, Play, RefreshCw, Table2 } from "lucide-react";
import { createPortal } from "react-dom";
import { useEffect, useRef, useState } from "react";
import { Button, SegmentedControl, SelectControl } from "@/components/ui";
import type { ChannelStatusController, ChannelStatusTestScope } from "../useChannelStatusController";

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
  const [testScope, setTestScope] = useState<ChannelStatusTestScope>("enabled");
  const [scopeMenuOpen, setScopeMenuOpen] = useState(false);
  const scopeButtonRef = useRef<HTMLButtonElement | null>(null);
  const scopeMenuRef = useRef<HTMLDivElement | null>(null);
  const [scopeMenuPosition, setScopeMenuPosition] = useState<{ top: number; left: number } | null>(null);
  const testing = controller.isRunningAction;

  useEffect(() => {
    if (!scopeMenuOpen) return;
    const updatePosition = () => {
      const rect = scopeButtonRef.current?.getBoundingClientRect();
      if (!rect) return;
      const width = 210;
      setScopeMenuPosition({
        top: rect.bottom + 4,
        left: Math.max(8, Math.min(rect.right - width, window.innerWidth - width - 8)),
      });
    };
    const closeOnOutside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!scopeButtonRef.current?.contains(target) && !scopeMenuRef.current?.contains(target)) setScopeMenuOpen(false);
    };
    updatePosition();
    document.addEventListener("pointerdown", closeOnOutside);
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutside);
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [scopeMenuOpen]);
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
        <div className="inline-flex h-8 items-stretch">
          <Button
            className="rounded-r-none border-r border-primary-foreground/25"
            disabled={testing || controller.statusQuery.isFetching}
            onClick={() => void controller.testAll(testScope)}
            title={testScope === "enabled" ? "测试所有启用渠道" : "测试所有有余额的监控"}
          >
            <Play className="h-4 w-4" />
            {testing ? "测试中" : "一键测试"}
          </Button>
          <button
            ref={scopeButtonRef}
            type="button"
            aria-label="选择测试范围"
            title={testScope === "enabled" ? "切换为测试所有有余额的监控" : "切换为测试所有启用渠道"}
            className="inline-flex w-8 items-center justify-center rounded-r-[var(--surface-radius)] bg-primary-solid text-primary-foreground hover:bg-primary-solid/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:pointer-events-none disabled:opacity-50"
            disabled={testing || controller.statusQuery.isFetching}
            onClick={() => setScopeMenuOpen((open) => !open)}
          >
            <ChevronDown className="h-4 w-4" />
          </button>
          {scopeMenuOpen && scopeMenuPosition ? createPortal(
            <div
              ref={scopeMenuRef}
              className="fixed z-[100] w-[210px] rounded-[var(--surface-radius)] border border-border bg-popover p-1 text-sm shadow-popover"
              style={{ top: scopeMenuPosition.top, left: scopeMenuPosition.left }}
            >
              {([
                ["enabled", "所有启用渠道"],
                ["with_balance", "所有有余额的监控"],
              ] as const).map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  className="flex w-full items-center justify-between rounded-[calc(var(--surface-radius)-3px)] px-2.5 py-2 text-left text-foreground hover:bg-hover"
                  onClick={() => {
                    setTestScope(value);
                    setScopeMenuOpen(false);
                  }}
                >
                  <span>{label}</span>
                  {testScope === value ? <Check className="h-4 w-4 text-primary" /> : null}
                </button>
              ))}
            </div>,
            document.body,
          ) : null}
        </div>
        <Button className="hidden" variant="secondary" disabled={controller.statusQuery.isFetching} onClick={() => void controller.refresh()}>
          <RefreshCw className="h-4 w-4" />
          刷新
        </Button>
      </div>
    </div>
  );
}
