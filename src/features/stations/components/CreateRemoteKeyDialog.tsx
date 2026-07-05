import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Button, Dialog, SelectControl } from "@/components/ui";

type RemoteKeyGroupOption = {
  groupIdHash: string | null;
  groupName: string;
};

type CreateRemoteKeyDialogProps = {
  open: boolean;
  groups: RemoteKeyGroupOption[];
  saving?: boolean;
  onClose: () => void;
  onSubmit: (input: { name: string; groupIdHash: string | null; groupName: string | null }) => void;
};

const noGroupValue = "__none__";
const inputClassName =
  "h-8 w-full rounded-[var(--surface-radius)] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition placeholder:text-slate-400 focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)]";

export function CreateRemoteKeyDialog({
  open,
  groups,
  saving = false,
  onClose,
  onSubmit,
}: CreateRemoteKeyDialogProps) {
  const [name, setName] = useState("");
  const [groupValue, setGroupValue] = useState(noGroupValue);
  const [error, setError] = useState<string | null>(null);

  const normalizedGroups = useMemo(() => {
    const seen = new Set<string>();
    return groups.filter((group) => {
      const key = `${group.groupIdHash ?? ""}|${group.groupName}`;
      if (seen.has(key)) {
        return false;
      }
      seen.add(key);
      return true;
    });
  }, [groups]);

  const groupOptions = useMemo(
    () => [
      { value: noGroupValue, label: "不指定分组", description: "按远端默认策略创建" },
      ...normalizedGroups.map((group, index) => ({
        value: groupOptionValue(index),
        label: group.groupName,
        description: group.groupIdHash ? `ID ${group.groupIdHash.slice(0, 8)}` : "无分组 ID",
      })),
    ],
    [normalizedGroups],
  );

  useEffect(() => {
    if (!open) {
      return;
    }
    setName("");
    setError(null);
    setGroupValue(normalizedGroups.length > 0 ? groupOptionValue(0) : noGroupValue);
  }, [open, normalizedGroups]);

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmedName = name.trim();
    if (!trimmedName) {
      setError("请填写远端 Key 名称");
      return;
    }

    const groupIndex = groupValue.startsWith("group-")
      ? Number(groupValue.replace("group-", ""))
      : -1;
    const selectedGroup = groupIndex >= 0 ? normalizedGroups[groupIndex] ?? null : null;
    onSubmit({
      name: trimmedName,
      groupIdHash: selectedGroup?.groupIdHash ?? null,
      groupName: selectedGroup?.groupName ?? null,
    });
  }

  return (
    <Dialog
      open={open}
      title="新建远端 Key"
      description="创建后会同步保存为本地 Key。"
      className="max-w-[520px]"
      onClose={onClose}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={onClose} disabled={saving}>
            取消
          </Button>
          <Button type="submit" form="create-remote-key-form" disabled={saving}>
            {saving ? "创建中" : "创建"}
          </Button>
        </div>
      }
    >
      <form id="create-remote-key-form" className="grid gap-4 p-5" onSubmit={handleSubmit}>
        <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
          Key 名称
          <input
            className={inputClassName}
            disabled={saving}
            value={name}
            onChange={(event) => {
              setName(event.target.value);
              setError(null);
            }}
            placeholder="例如 默认转发 Key"
            required
          />
        </label>
        <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
          远端分组
          <SelectControl
            ariaLabel="远端分组"
            className="w-full"
            disabled={saving}
            options={groupOptions}
            value={groupValue}
            onChange={setGroupValue}
          />
        </label>
        {normalizedGroups.length === 0 && (
          <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-slate-50 px-3 py-2 text-xs text-muted-foreground">
            暂未发现远端分组，可先不指定分组创建。
          </div>
        )}
        {error && <div className="text-xs text-rose-600">{error}</div>}
      </form>
    </Dialog>
  );
}

function groupOptionValue(index: number) {
  return `group-${index}`;
}
