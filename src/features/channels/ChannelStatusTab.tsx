import { useState } from "react";
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

export function ChannelStatusTab() {
  const controller = useChannelStatusController();
  const [viewMode, setViewMode] = useState<ChannelStatusViewMode>("table");
  const rawError = controller.statusQuery.error ? readError(controller.statusQuery.error) : null;
  const error = rawError === "The desktop operation failed."
    ? "状态数据读取失败，请刷新重试。"
    : rawError;

  return (
    <div className="space-y-3">
      <ChannelStatusToolbar
        controller={controller}
        viewMode={viewMode}
        onViewModeChange={setViewMode}
      />

      {error && (
        <div className="flex items-start gap-2 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div>
            <div className="font-medium">状态 workspace 读取失败</div>
            <div className="text-xs">{error}</div>
          </div>
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

      <MonitorExecutionDrawer
        executionId={controller.selectedExecutionId}
        onClose={() => controller.setSelectedExecutionId(null)}
      />
    </div>
  );
}
