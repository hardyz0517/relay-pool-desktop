import { useState } from "react";
import { Eye, EyeOff } from "lucide-react";
import { Button, Dialog } from "@/components/ui";
import { REPLACE_CURRENT_CONFIRMATION, type PortableMigrationCapability, type PortableImportMode } from "@/lib/types/dataMigration";
import { ImportMigrationSummary } from "./ImportMigrationSummary";
import type { ImportInspectionDraft, ImportPrepareDraft, MigrationControllerState } from "./useDataMigrationController";

type ImportMigrationDialogProps = {
  capability: PortableMigrationCapability | null;
  controller: Pick<MigrationControllerState, "busy" | "operation" | "startImportInspection" | "prepareImport">;
  open: boolean;
  onClose: () => void;
};

export function ImportMigrationDialog({
  capability,
  controller,
  open,
  onClose,
}: ImportMigrationDialogProps) {
  const [passphrase, setPassphrase] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [inspectedImportId, setInspectedImportId] = useState("");
  const [mode, setMode] = useState<PortableImportMode>("restoreIntoEmpty");
  const [confirmationText, setConfirmationText] = useState("");

  async function inspect() {
    const draft: ImportInspectionDraft = { passphrase };
    await controller.startImportInspection(draft);
    setPassphrase("");
  }

  async function prepare() {
    const draft: ImportPrepareDraft = { inspectedImportId, mode, confirmationText };
    await controller.prepareImport(draft);
  }

  return (
    <Dialog
      open={open}
      title="导入跨设备数据包"
      description="先检查包内容，再选择恢复到空数据库或替换当前数据。"
      onClose={onClose}
      footer={(
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={onClose}>关闭</Button>
          <Button disabled={controller.busy || !capability?.enabled || !inspectedImportId} onClick={() => void prepare()}>
            准备导入
          </Button>
        </div>
      )}
    >
      <div className="grid gap-4 px-5 py-4 text-sm">
        <label className="grid gap-1 text-xs font-medium text-muted-foreground">
          迁移密码
          <div className="flex gap-2">
            <input
              className="h-8 min-w-0 flex-1 rounded-[var(--surface-radius)] border border-border bg-control px-3 text-sm text-foreground outline-none focus:border-ring"
              type={showPassword ? "text" : "password"}
              value={passphrase}
              onChange={(event) => setPassphrase(event.target.value)}
            />
            <Button
              aria-label={showPassword ? "隐藏密码" : "显示密码"}
              size="icon"
              title={showPassword ? "隐藏密码" : "显示密码"}
              variant="outline"
              onClick={() => setShowPassword((value) => !value)}
            >
              {showPassword ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
            </Button>
          </div>
        </label>
        <Button disabled={controller.busy || !capability?.enabled || !passphrase} variant="secondary" onClick={() => void inspect()}>
          选择数据包并检查
        </Button>
        <ImportMigrationSummary operation={controller.operation} />
        <label className="grid gap-1 text-xs font-medium text-muted-foreground">
          已检查的导入 ID
          <input
            className="h-8 rounded-[var(--surface-radius)] border border-border bg-control px-3 text-sm text-foreground outline-none focus:border-ring"
            placeholder="检查完成后填入 inspectionId"
            value={inspectedImportId}
            onChange={(event) => setInspectedImportId(event.target.value)}
          />
        </label>
        <div className="grid gap-2 text-xs text-muted-foreground">
          <span className="font-medium">恢复模式</span>
          <label className="flex items-center gap-2">
            <input checked={mode === "restoreIntoEmpty"} type="radio" onChange={() => setMode("restoreIntoEmpty")} />
            恢复到空数据库
          </label>
          <label className="flex items-center gap-2">
            <input checked={mode === "replaceCurrent"} type="radio" onChange={() => setMode("replaceCurrent")} />
            替换当前数据
          </label>
        </div>
        {mode === "replaceCurrent" ? (
          <label className="grid gap-1 text-xs font-medium text-muted-foreground">
            替换确认文本
            <input
              className="h-8 rounded-[var(--surface-radius)] border border-danger-border bg-control px-3 text-sm text-foreground outline-none focus:border-ring"
              placeholder={REPLACE_CURRENT_CONFIRMATION}
              value={confirmationText}
              onChange={(event) => setConfirmationText(event.target.value)}
            />
          </label>
        ) : null}
      </div>
    </Dialog>
  );
}
