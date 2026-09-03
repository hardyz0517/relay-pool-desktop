import { Profiler, useEffect, useRef, useState } from "react";
import { AlertTriangle } from "lucide-react";
import { readError } from "@/lib/errors";
import { ChannelStatusCardGrid } from "./components/ChannelStatusCardGrid";
import { ChannelStatusTable } from "./components/ChannelStatusTable";
import {
  ChannelStatusToolbar,
  type ChannelStatusViewMode,
} from "./components/ChannelStatusToolbar";
import { MonitorExecutionDrawer } from "./components/MonitorExecutionDrawer";
import { useChannelStatusController } from "./useChannelStatusController";
import { recordMonitoringPerformance } from "@/lib/monitoringPerformance";

export function ChannelStatusTab() {
  const [viewMode, setViewMode] = useState<ChannelStatusViewMode>("table");
  const controller = useChannelStatusController(viewMode);
  const [, setFreshnessTick] = useState(0);
  const firstContentRecorded = useRef(false);
  const rawError = controller.statusQuery.error ? readError(controller.statusQuery.error) : null;
  const error = rawError === "The desktop operation failed."
    ? "状态数据读取失败，请刷新重试。"
    : rawError;
  const dataUpdatedAt = controller.statusQuery.dataUpdatedAt;
  const hasData = controller.statusQuery.data !== undefined;
  const stale = hasData && dataUpdatedAt > 0 && Date.now() - dataUpdatedAt > 15_000;

  useEffect(() => {
    if (!hasData || dataUpdatedAt <= 0) return;
    const delay = Math.max(0, dataUpdatedAt + 15_000 - Date.now());
    const timeout = window.setTimeout(() => setFreshnessTick((current) => current + 1), delay);
    return () => window.clearTimeout(timeout);
  }, [dataUpdatedAt, hasData]);

  return (
    <div className="space-y-3">
      <div data-tour="channels-local-toolbar">
        <ChannelStatusToolbar
          controller={controller}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
        />
      </div>

      {error && (
        <div className="flex items-start justify-between gap-3 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div className="min-w-0 flex-1">
            <div className="font-medium">状态 workspace 读取失败</div>
            <div className="text-xs">{error}</div>
          </div>
          <button
            type="button"
            className="shrink-0 text-danger-foreground underline-offset-2 hover:underline"
            onClick={() => void controller.refresh()}
            disabled={controller.statusQuery.isFetching}
          >
            重试
          </button>
        </div>
      )}

      <Profiler
        id="channel-status-results"
        onRender={(_id, phase, actualDuration) => {
          recordMonitoringPerformance({
            name: "channel-status-react-commit",
            durationMs: actualDuration,
            phase,
          });
          if (phase === "mount" && !firstContentRecorded.current) {
            firstContentRecorded.current = true;
            recordMonitoringPerformance({
              name: "channel-status-first-content",
              durationMs: actualDuration,
              phase,
            });
          }
        }}
      >
      <div data-tour="channels-local-results">
        {stale && (
          <div className="mb-2 flex items-center justify-between gap-2 text-xs text-muted-foreground" role="status">
            <span>状态数据可能已过期</span>
            <button
              type="button"
              className="text-primary underline-offset-2 hover:underline disabled:cursor-not-allowed disabled:opacity-60"
              onClick={() => void controller.refresh()}
              disabled={controller.statusQuery.isFetching}
            >
              立即刷新
            </button>
          </div>
        )}
        {viewMode === "cards" ? (
          <ChannelStatusCardGrid
            rows={controller.workspaceView.rows}
            loading={controller.statusQuery.isPending}
          />
        ) : (
          <ChannelStatusTable
            rows={controller.workspaceView.rows}
            loading={controller.statusQuery.isPending}
            actionPending={controller.isRunningAction}
            onRunNow={controller.runNow}
            onCancel={controller.cancel}
            onOpenExecution={controller.setSelectedExecutionId}
          />
        )}
      </div>
      </Profiler>

      <MonitorExecutionDrawer
        executionId={controller.selectedExecutionId}
        onClose={() => controller.setSelectedExecutionId(null)}
      />
    </div>
  );
}
