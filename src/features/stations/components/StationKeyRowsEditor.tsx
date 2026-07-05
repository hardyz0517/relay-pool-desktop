import { Plus, Trash2 } from "lucide-react";
import { Button, SwitchControl } from "@/components/ui";
import { cn } from "@/lib/utils";

export type StationKeyDraft = {
  clientId: string;
  id: string | null;
  name: string;
  apiKey: string;
  groupName: string;
  rateMultiplier: string;
  enabled: boolean;
  note: string;
  deleteRequested: boolean;
};

type StationKeyRowsEditorProps = {
  rows: StationKeyDraft[];
  disabled?: boolean;
  onRowsChange: (rows: StationKeyDraft[]) => void;
};

const inputClassName =
  "h-8 w-full min-w-0 rounded-[var(--surface-radius)] border border-border bg-white px-2.5 text-xs text-slate-800 outline-none transition placeholder:text-slate-400 focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.16)] disabled:bg-slate-50 disabled:text-slate-500";

export function createEmptyStationKeyDraft(index: number): StationKeyDraft {
  return {
    clientId: `station-key-draft-${Date.now()}-${index}`,
    id: null,
    name: "",
    apiKey: "",
    groupName: "",
    rateMultiplier: "",
    enabled: true,
    note: "",
    deleteRequested: false,
  };
}

export function StationKeyRowsEditor({ rows, disabled, onRowsChange }: StationKeyRowsEditorProps) {
  const visibleRows = rows.filter((row) => !row.deleteRequested);

  function updateRow(clientId: string, patch: Partial<StationKeyDraft>) {
    onRowsChange(rows.map((row) => (row.clientId === clientId ? { ...row, ...patch } : row)));
  }

  function deleteRow(target: StationKeyDraft) {
    if (target.id) {
      updateRow(target.clientId, { deleteRequested: true });
      return;
    }
    onRowsChange(rows.filter((row) => row.clientId !== target.clientId));
  }

  return (
    <div className="grid gap-2">
      <div className="overflow-x-auto">
        <div className="min-w-[760px]">
          <div className="grid h-7 grid-cols-[minmax(7rem,1fr)_minmax(11rem,1.35fr)_minmax(7rem,0.85fr)_6rem_5.5rem_2.5rem] items-center gap-2 border-b border-border px-1 text-[11px] font-medium text-muted-foreground">
            <span>名称</span>
            <span>密钥</span>
            <span>分组</span>
            <span>倍率</span>
            <span>启用</span>
            <span className="text-right">操作</span>
          </div>

          <div className="grid gap-1.5 py-2">
            {visibleRows.map((row, index) => (
              <div
                key={row.clientId}
                className="grid min-h-9 grid-cols-[minmax(7rem,1fr)_minmax(11rem,1.35fr)_minmax(7rem,0.85fr)_6rem_5.5rem_2.5rem] items-center gap-2"
              >
                <input
                  className={inputClassName}
                  disabled={disabled}
                  value={row.name}
                  onChange={(event) => updateRow(row.clientId, { name: event.target.value })}
                  placeholder={`密钥 ${index + 1}`}
                />
                <input
                  className={inputClassName}
                  disabled={disabled}
                  type="password"
                  value={row.apiKey}
                  onChange={(event) => updateRow(row.clientId, { apiKey: event.target.value })}
                  placeholder={row.id ? "留空保留旧密钥" : "sk-..."}
                />
                <input
                  className={inputClassName}
                  disabled={disabled}
                  value={row.groupName}
                  onChange={(event) => updateRow(row.clientId, { groupName: event.target.value })}
                  placeholder="默认分组"
                />
                <input
                  className={inputClassName}
                  disabled={disabled}
                  inputMode="decimal"
                  value={row.rateMultiplier}
                  onChange={(event) => updateRow(row.clientId, { rateMultiplier: event.target.value })}
                  placeholder="1"
                />
                <SwitchControl
                  ariaLabel={`切换密钥 ${index + 1}`}
                  checked={row.enabled}
                  className="h-8 min-w-0 justify-center px-1"
                  disabled={disabled}
                  offLabel="停用"
                  onCheckedChange={() => updateRow(row.clientId, { enabled: !row.enabled })}
                  onLabel="启用"
                  showLabel={false}
                />
                <Button
                  aria-label={`删除密钥 ${index + 1}`}
                  className={cn("justify-self-end", row.id && "text-rose-700")}
                  disabled={disabled}
                  size="icon"
                  variant="ghost"
                  onClick={() => deleteRow(row)}
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>
            ))}
          </div>
        </div>
      </div>

      {visibleRows.length === 0 && (
        <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-slate-50 px-3 py-2 text-xs text-muted-foreground">
          暂无本地密钥，点击添加后录入。
        </div>
      )}

      <div className="flex justify-start">
        <Button
          disabled={disabled}
          size="sm"
          variant="outline"
          onClick={() => onRowsChange([...rows, createEmptyStationKeyDraft(rows.length)])}
        >
          <Plus className="h-3.5 w-3.5" />
          添加密钥
        </Button>
      </div>
    </div>
  );
}
