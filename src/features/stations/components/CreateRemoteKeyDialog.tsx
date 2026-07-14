import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Button, Dialog, SelectControl } from "@/components/ui";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import {
  formatMultiplier,
  noGroupOptionValue,
  normalizeStationGroupOptions,
  stationGroupSelectValue,
} from "../groupOptionViewModels";

type CreateRemoteKeyDialogProps = {
  open: boolean;
  groups: StationGroupOption[];
  saving?: boolean;
  onClose: () => void;
  onSubmit: (input: {
    name: string;
    groupBindingId: string | null;
    groupIdHash: string | null;
    groupName: string | null;
  }) => void;
};

const inputClassName =
  "h-8 w-full rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-sm text-foreground outline-none transition placeholder:text-muted-foreground/70 focus:border-ring focus:ring-2 focus:ring-ring/30";

export function CreateRemoteKeyDialog({
  open,
  groups,
  saving = false,
  onClose,
  onSubmit,
}: CreateRemoteKeyDialogProps) {
  const [name, setName] = useState("");
  const [groupValue, setGroupValue] = useState(noGroupOptionValue);
  const [error, setError] = useState<string | null>(null);

  const normalizedGroups = useMemo(() => normalizeStationGroupOptions(groups), [groups]);

  const groupOptions = useMemo(
    () => [
      { value: noGroupOptionValue, label: "不指定分组", description: "按远端默认策略创建" },
      ...normalizedGroups.map((group) => ({
        value: stationGroupSelectValue(group),
        label: (
          <span className="inline-flex min-w-0 max-w-full items-center gap-2">
            <span className="min-w-0 truncate">{group.groupName}</span>
            <RemoteGroupRateTag rateMultiplier={group.rateMultiplier} />
          </span>
        ),
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
    setGroupValue(
      normalizedGroups.length > 0
        ? stationGroupSelectValue(normalizedGroups[0])
        : noGroupOptionValue,
    );
  }, [open, normalizedGroups]);

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmedName = name.trim();
    if (!trimmedName) {
      setError("请填写远端 Key 名称");
      return;
    }

    const selectedGroup =
      normalizedGroups.find((group) => stationGroupSelectValue(group) === groupValue) ?? null;
    onSubmit({
      name: trimmedName,
      groupBindingId: selectedGroup?.groupBindingId ?? null,
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
          <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-surface-subtle px-3 py-2 text-xs text-muted-foreground">
            暂未发现远端分组，可先不指定分组创建。
          </div>
        )}
        {error && <div className="text-xs text-danger-foreground">{error}</div>}
      </form>
    </Dialog>
  );
}

function RemoteGroupRateTag({ rateMultiplier }: { rateMultiplier: number | null }) {
  return (
    <span className="shrink-0 rounded-[calc(var(--surface-radius)-3px)] border border-border bg-surface-subtle px-1.5 py-0.5 text-[11px] font-medium leading-none text-muted-foreground">
      {rateMultiplier === null ? "倍率未采集" : `${formatMultiplier(rateMultiplier)}x`}
    </span>
  );
}
