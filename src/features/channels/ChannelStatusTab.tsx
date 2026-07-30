import { useState } from "react";
import { AlertTriangle } from "lucide-react";
import { readError } from "@/lib/errors";
import { monitoringCapabilitiesQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { ChannelStatusTable } from "./components/ChannelStatusTable";
import { ChannelStatusToolbar } from "./components/ChannelStatusToolbar";
import { MonitorDefinitionDialog } from "./components/MonitorDefinitionDialog";
import { MonitorExecutionDrawer } from "./components/MonitorExecutionDrawer";
import { useChannelStatusController } from "./useChannelStatusController";

export function ChannelStatusTab() {
  const controller = useChannelStatusController();
  const capabilitiesQuery = useActivityQuery(monitoringCapabilitiesQueryOptions());
  const [definitionDialogOpen, setDefinitionDialogOpen] = useState(false);
  const error = controller.statusQuery.error ? readError(controller.statusQuery.error) : null;

  return (
    <div className="space-y-3">
      <ChannelStatusToolbar
        controller={controller}
        capabilities={capabilitiesQuery.data}
        onCreateMonitor={() => setDefinitionDialogOpen(true)}
      />

      <div className="flex flex-wrap items-center justify-between gap-2 rounded-[var(--surface-radius)] border border-border bg-surface-subtle px-3 py-2 text-xs text-muted-foreground">
        <div>
          {controller.workspaceView.aggregateLabel} · 生成于 {controller.workspaceView.generatedAtLabel}
        </div>
        <div>{controller.workspaceView.freshnessLabel}</div>
      </div>

      {error && (
        <div className="flex items-start gap-2 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div>
            <div className="font-medium">状态 workspace 读取失败</div>
            <div className="text-xs">{error}</div>
          </div>
        </div>
      )}

      <ChannelStatusTable
        rows={controller.workspaceView.rows}
        loading={controller.statusQuery.isPending}
        actionPending={controller.isRunningAction}
        onRunNow={controller.runNow}
        onCancel={controller.cancel}
        onOpenExecution={controller.setSelectedExecutionId}
      />

      <MonitorExecutionDrawer
        executionId={controller.selectedExecutionId}
        onClose={() => controller.setSelectedExecutionId(null)}
      />

      <MonitorDefinitionDialog
        open={definitionDialogOpen}
        capabilities={capabilitiesQuery.data}
        onClose={() => setDefinitionDialogOpen(false)}
      />
    </div>
  );
}
