import { ArrowDownToLine, ArrowUpFromLine, RefreshCw, ShieldCheck } from "lucide-react";
import { Button, SectionCard, StatusBadge } from "@/components/ui";
import { describeCapability, operationProgressLabel, terminalLabel } from "./migrationViewModel";
import { ExportMigrationDialog } from "./ExportMigrationDialog";
import { ImportMigrationDialog } from "./ImportMigrationDialog";
import { useDataMigrationController } from "./useDataMigrationController";

export function DataMigrationSection() {
  const controller = useDataMigrationController();
  const capability = describeCapability(controller.capability);
  return (
    <SectionCard
      contentClassName="p-0"
      title="跨设备搬家"
      action={<StatusBadge tone={capability.tone}>{capability.title}</StatusBadge>}
    >
      <div className="grid gap-4 px-5 py-4 text-sm">
        <div className="flex gap-3">
          <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-info-surface text-info-foreground">
            <ShieldCheck className="h-4 w-4" />
          </div>
          <div className="min-w-0">
            <p className="font-medium text-foreground">用密码保护的搬家包把本机数据带到另一台电脑</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              {capability.detail}
            </p>
          </div>
        </div>
        <div className="grid gap-2 rounded-[var(--surface-radius)] border border-border bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
          <div>导出流程：选择位置 → 设置密码 → 生成加密包 → 校验完成。</div>
          <div>导入流程：选择包 → 输入密码 → 检查摘要 → 确认模式 → 准备重启激活。</div>
          {controller.operation ? (
            <div>
              当前操作：{operationProgressLabel(controller.operation)}
              {terminalLabel(controller.operation) ? `，${terminalLabel(controller.operation)}` : ""}
            </div>
          ) : null}
          {controller.message ? <div className="text-warning-foreground">{controller.message}</div> : null}
        </div>
        <div className="flex flex-wrap gap-2">
          <Button disabled={controller.busy || !capability.enabled} variant="secondary" onClick={controller.openExportDialog}>
            <ArrowUpFromLine className="h-4 w-4" />
            导出搬家包
          </Button>
          <Button disabled={controller.busy || !capability.enabled} variant="outline" onClick={controller.openImportDialog}>
            <ArrowDownToLine className="h-4 w-4" />
            导入搬家包
          </Button>
          <Button disabled={controller.loading} variant="ghost" onClick={() => void controller.refresh()}>
            <RefreshCw className={controller.loading ? "h-4 w-4 animate-spin" : "h-4 w-4"} />
            刷新状态
          </Button>
        </div>
      </div>
      <ExportMigrationDialog
        busy={controller.busy}
        capability={controller.capability}
        open={controller.exportOpen}
        onClose={controller.closeExportDialog}
        onSubmit={controller.startExport}
      />
      <ImportMigrationDialog
        capability={controller.capability}
        controller={controller}
        open={controller.importOpen}
        onClose={controller.closeImportDialog}
      />
    </SectionCard>
  );
}
