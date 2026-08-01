import { useMemo, useState } from "react";
import { Eye, EyeOff } from "lucide-react";
import { Button, Dialog } from "@/components/ui";
import type { PortableMigrationCapability } from "@/lib/types/dataMigration";
import {
  defaultIncludeHistory,
  MIN_PASSPHRASE_SCALARS,
  validatePassphrase,
} from "./migrationViewModel";
import type { ExportMigrationDraft } from "./useDataMigrationController";

type ExportMigrationDialogProps = {
  capability: PortableMigrationCapability | null;
  busy: boolean;
  open: boolean;
  onClose: () => void;
  onSubmit: (draft: ExportMigrationDraft) => Promise<void>;
};

export function ExportMigrationDialog({
  capability,
  busy,
  open,
  onClose,
  onSubmit,
}: ExportMigrationDialogProps) {
  const [passphrase, setPassphrase] = useState("");
  const [passphraseConfirmation, setPassphraseConfirmation] = useState("");
  const [includeHistory, setIncludeHistory] = useState(defaultIncludeHistory(capability));
  const [showPassword, setShowPassword] = useState(false);
  const validation = useMemo(
    () => validatePassphrase(passphrase, passphraseConfirmation, capability?.limits.maxPassphraseUtf8Bytes ?? 1024),
    [capability?.limits.maxPassphraseUtf8Bytes, passphrase, passphraseConfirmation],
  );
  const passwordType = showPassword ? "text" : "password";

  async function submit() {
    await onSubmit({ passphrase, passphraseConfirmation, includeHistory });
    setPassphrase("");
    setPassphraseConfirmation("");
  }

  return (
    <Dialog
      open={open}
      title="导出跨设备数据包"
      description="生成一个显式密码保护的 .rpd-move 文件。"
      onClose={onClose}
      footer={(
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={onClose}>取消</Button>
          <Button disabled={busy || !capability?.enabled || !validation.ok} onClick={() => void submit()}>
            选择位置并导出
          </Button>
        </div>
      )}
    >
      <div className="grid gap-4 px-5 py-4 text-sm">
        <PasswordField
          label="迁移密码"
          placeholder={`至少输入 ${MIN_PASSPHRASE_SCALARS} 个字符`}
          type={passwordType}
          value={passphrase}
          onChange={setPassphrase}
          onToggle={() => setShowPassword((value) => !value)}
          visible={showPassword}
        />
        <PasswordField
          label="再次输入"
          placeholder="再次输入迁移密码"
          type={passwordType}
          value={passphraseConfirmation}
          onChange={setPassphraseConfirmation}
          onToggle={() => setShowPassword((value) => !value)}
          visible={showPassword}
        />
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <input
            checked={includeHistory}
            disabled={!capability?.historySupported}
            type="checkbox"
            onChange={(event) => setIncludeHistory(event.target.checked)}
          />
          包含历史记录（默认关闭，文件会更大）
        </label>
        <p className="text-xs text-muted-foreground">
          已输入 {validation.scalarCount} 个 Unicode 字符 / {validation.utf8Bytes} UTF-8 字节。
        </p>
      </div>
    </Dialog>
  );
}

function PasswordField({
  label,
  placeholder,
  type,
  value,
  visible,
  onChange,
  onToggle,
}: {
  label: string;
  placeholder: string;
  type: "password" | "text";
  value: string;
  visible: boolean;
  onChange: (value: string) => void;
  onToggle: () => void;
}) {
  return (
    <label className="grid gap-1 text-xs font-medium text-muted-foreground">
      {label}
      <div className="flex gap-2">
        <input
          className="h-8 min-w-0 flex-1 rounded-[var(--surface-radius)] border border-border bg-control px-3 text-sm text-foreground outline-none focus:border-ring"
          type={type}
          placeholder={placeholder}
          value={value}
          onChange={(event) => onChange(event.target.value)}
        />
        <Button aria-label={visible ? "隐藏密码" : "显示密码"} size="icon" title={visible ? "隐藏密码" : "显示密码"} variant="outline" onClick={onToggle}>
          {visible ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
        </Button>
      </div>
    </label>
  );
}
